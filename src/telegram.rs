use crate::args::Args;
use crate::utils;
use crate::{log_debug, log_error, log_info};
use anyhow::{Result, anyhow};
use rand::{Rng, SeedableRng, rngs::StdRng};
use reqwest::StatusCode;
use reqwest::blocking::{Client, multipart};
use serde::Serialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Instant;

const PHOTO_MAX_BYTES: u64 = 10 * 1024 * 1024;

pub struct SendTg {
    api_url: String,
    bot_token: String,
    pub chat_id: String,
    chat_name: String,
    client: Client,
}

impl SendTg {
    pub fn new(api_url: String, bot_token: String, chat_id: String) -> Result<Self> {
        if bot_token.trim().is_empty() {
            log_error!("Bot token is required!");
            return Err(anyhow!("Bot token is missing!"));
        }

        if chat_id.trim().is_empty() {
            log_error!("Chat ID is required!");
            return Err(anyhow!("Chat ID is missing!"));
        }

        if api_url.trim().is_empty() {
            log_error!("API URL is required!");
            return Err(anyhow!("API URL is missing!"));
        }

        Ok(Self {
            api_url,
            bot_token,
            chat_id,
            chat_name: "Unknown".to_string(),
            client: Client::builder().timeout(None).build()?,
        })
    }

    pub fn run(&mut self, args: &Args) -> Result<()> {
        if args.media_paths.is_empty() && args.message.is_none() {
            if args.check {
                let chat_id = self.chat_id.clone();
                self.check(&chat_id)?;
                return Ok(());
            }
            return Err(anyhow!(
                "No message or media provided, use -h/--help for help."
            ));
        }

        utils::validate_defaults(
            args.provided_api_url,
            args.provided_bot_token,
            args.provided_chat_id,
            &self.api_url,
            &self.bot_token,
            &self.chat_id,
        );

        if !args.media_paths.is_empty() {
            let chat_id = self.chat_id.clone();
            self.send_media(
                &chat_id,
                &args.media_paths,
                args.caption.as_deref(),
                args.as_file,
                args.no_group,
                args.button_text.clone(),
                args.button_url.clone(),
                args.spoiler,
            )?;
            return Ok(());
        }

        if let Some(message) = &args.message {
            let reply_markup = utils::create_reply_markup(&args.button_text, &args.button_url);
            let chat_id = self.chat_id.clone();
            self.send_message(&chat_id, message, args.silent, reply_markup.as_ref())?;
            return Ok(());
        }

        Err(anyhow!("No message or media provided."))
    }

    fn send_message(
        &mut self,
        chat_id: &str,
        message: &str,
        silent: bool,
        reply_markup: Option<&Value>,
    ) -> Result<()> {
        self.send_chat_action(chat_id, "typing");

        let mut payload = json!({
            "chat_id": chat_id,
            "text": message.replace("\\n", "\n"),
            "parse_mode": "HTML",
            "disable_notification": silent,
        });

        if let Some(markup) = reply_markup {
            payload["reply_markup"] = markup.clone();
        }

        let url = format!("{}{}/sendMessage", self.api_url, self.bot_token);
        let response = self.client.post(&url).json(&payload).send();

        match self.handle_response("Failed to send message:", response) {
            Ok(_) => {
                log_info!("Message sent to {}: {}", self.chat_name, message);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn send_media(
        &mut self,
        chat_id: &str,
        media_paths: &[PathBuf],
        caption: Option<&str>,
        as_file: bool,
        no_group: bool,
        button_text: Option<String>,
        button_url: Option<String>,
        spoiler: bool,
    ) -> Result<()> {
        let reply_markup_json = utils::create_reply_markup(&button_text, &button_url);
        let reply_markup_text = reply_markup_json
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok());

        let mut media_items = Vec::new();
        let mut caption_assigned = false;

        for path in media_paths {
            if !utils::is_regular_file(path) {
                log_error!("File not found: {}", path.display());
                continue;
            }

            let mime_type = utils::detect_mime_type(path);
            let mut media_type = if as_file {
                "document"
            } else {
                utils::determine_media_type(mime_type.as_deref())
            };

            if !matches!(media_type, "photo" | "video" | "audio" | "document") {
                log_error!(
                    "Unsupported media type for {}: {}",
                    path.display(),
                    media_type
                );
                continue;
            }

            if media_type == "photo" {
                match std::fs::metadata(path) {
                    Ok(meta) => {
                        if meta.len() > PHOTO_MAX_BYTES {
                            log_info!(
                                "Photo {} exceeds 10 MB ({} bytes); sending as document.",
                                path.display(),
                                meta.len()
                            );
                            media_type = "document";
                        }
                    }
                    Err(err) => {
                        log_error!("Failed to read photo metadata {}: {}", path.display(), err);
                        media_type = "document";
                    }
                }
            }

            let is_video_file =
                matches!(mime_type.as_deref(), Some(mt) if mt.starts_with("video/"));
            let is_image_file =
                matches!(mime_type.as_deref(), Some(mt) if mt.starts_with("image/"));

            let metadata = if is_video_file {
                log_info!("Extracting video metadata from {}", path.display());
                match utils::extract_video_metadata(path) {
                    Ok(meta) => {
                        if meta.is_some() {
                            log_info!(
                                "Video metadata extracted successfully for {}",
                                path.display()
                            );
                        }
                        meta.map(utils::MediaMetadata::Video)
                    }
                    Err(err) => {
                        log_error!(
                            "Failed to extract video metadata for {}: {}",
                            path.display(),
                            err
                        );
                        None
                    }
                }
            } else if is_image_file {
                log_info!("Extracting photo thumbnail from {}", path.display());
                match utils::extract_photo_metadata(path) {
                    Ok(result) => {
                        if let Some(ref thumb) = result {
                            if thumb.is_some() {
                                log_info!(
                                    "Photo thumbnail generated successfully for {}",
                                    path.display()
                                );
                            }
                        }
                        result.map(|thumb_opt| utils::MediaMetadata::Photo {
                            thumbnail: thumb_opt,
                        })
                    }
                    Err(err) => {
                        log_error!(
                            "Failed to extract photo thumbnail for {}: {}",
                            path.display(),
                            err
                        );
                        None
                    }
                }
            } else {
                None
            };

            let caption_for_item = if !caption_assigned {
                caption.map(|c| c.to_string())
            } else {
                None
            };
            if caption_for_item.is_some() {
                caption_assigned = true;
            }

            let part_name = format!("file{}", media_items.len());

            media_items.push(MediaItem {
                media_type: media_type.to_string(),
                file_name: path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("media")
                    .to_string(),
                path: path.clone(),
                caption: caption_for_item,
                spoiler: spoiler && matches!(media_type, "photo" | "video"),
                metadata,
                part_name,
            });
        }

        if media_items.is_empty() {
            return Ok(());
        }

        let mut index = 0;
        while index < media_items.len() {
            if media_items[index].media_type == "document" {
                if no_group {
                    let item = &media_items[index];
                    self.send_chat_action(chat_id, "upload_document");
                    let caption_to_use = item.caption.as_deref().or(caption);
                    self.send_single_media(
                        chat_id,
                        item,
                        caption_to_use,
                        reply_markup_text.as_deref(),
                        item.spoiler,
                    )?;
                    index += 1;
                    continue;
                }

                let mut chunk_indices = Vec::new();
                while index < media_items.len()
                    && chunk_indices.len() < 10
                    && media_items[index].media_type == "document"
                {
                    chunk_indices.push(index);
                    index += 1;
                }

                if chunk_indices.len() == 1 {
                    let item = &media_items[chunk_indices[0]];
                    self.send_chat_action(chat_id, "upload_document");
                    let caption_to_use = item.caption.as_deref().or(caption);
                    self.send_single_media(
                        chat_id,
                        item,
                        caption_to_use,
                        reply_markup_text.as_deref(),
                        item.spoiler,
                    )?;
                    continue;
                }

                self.send_chat_action(chat_id, "upload_document");
                let chunk_items: Vec<MediaItem> = chunk_indices
                    .iter()
                    .map(|&idx| media_items[idx].clone())
                    .collect();
                self.send_media_group(chat_id, &chunk_items, reply_markup_text.as_deref())?;
                continue;
            }

            let mut chunk_indices = Vec::new();
            while index < media_items.len()
                && chunk_indices.len() < 10
                && media_items[index].media_type != "document"
            {
                chunk_indices.push(index);
                index += 1;
            }

            if chunk_indices.is_empty() {
                continue;
            }

            if no_group || chunk_indices.len() == 1 {
                for idx in chunk_indices {
                    let item = &media_items[idx];
                    let action = format!("upload_{}", item.media_type.to_lowercase());
                    self.send_chat_action(chat_id, &action);
                    let caption_to_use = item.caption.as_deref().or(caption);
                    self.send_single_media(
                        chat_id,
                        item,
                        caption_to_use,
                        reply_markup_text.as_deref(),
                        item.spoiler,
                    )?;
                }
                continue;
            }

            let first_item = &media_items[chunk_indices[0]];
            let action = format!("upload_{}", first_item.media_type.to_lowercase());
            self.send_chat_action(chat_id, &action);
            let chunk_items: Vec<MediaItem> = chunk_indices
                .iter()
                .map(|&idx| media_items[idx].clone())
                .collect();
            self.send_media_group(chat_id, &chunk_items, reply_markup_text.as_deref())?;
        }

        Ok(())
    }

    fn send_media_group(
        &self,
        chat_id: &str,
        items: &[MediaItem],
        reply_markup: Option<&str>,
    ) -> Result<()> {
        let mut media_payload = Vec::new();
        let mut thumbnails: Vec<(String, Vec<u8>)> = Vec::new();

        for item in items {
            let mut entry = InputMedia {
                media_type: item.media_type.clone(),
                media: format!("attach://{}", item.part_name),
                caption: item.caption.clone(),
                has_spoiler: if item.spoiler { Some(true) } else { None },
                width: None,
                height: None,
                duration: None,
                thumbnail: None,
            };

            if let Some(metadata) = item.metadata.as_ref() {
                match metadata {
                    utils::MediaMetadata::Video(video_meta) => {
                        entry.width = video_meta.width;
                        entry.height = video_meta.height;
                        entry.duration = video_meta.duration;
                        if let Some(bytes) = video_meta.thumbnail.as_ref() {
                            let name = format!("{}_thumb", item.part_name);
                            entry.thumbnail = Some(format!("attach://{}", name));
                            thumbnails.push((name, bytes.clone()));
                        }
                    }
                    utils::MediaMetadata::Photo { thumbnail } => {
                        if let Some(bytes) = thumbnail.as_ref() {
                            let name = format!("{}_thumb", item.part_name);
                            entry.thumbnail = Some(format!("attach://{}", name));
                            thumbnails.push((name, bytes.clone()));
                        }
                    }
                }
            }

            media_payload.push(entry);
        }

        let serialized_media = serde_json::to_string(&media_payload)?;

        let mut form = multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .text("media", serialized_media);

        if let Some(markup) = reply_markup {
            form = form.text("reply_markup", markup.to_string());
        }

        for item in items {
            let reader = utils::progress_reader_for_path(&item.path, &item.file_name)?;
            let part = multipart::Part::reader(reader).file_name(item.file_name.clone());
            form = form.part(item.part_name.clone(), part);
        }

        for (name, bytes) in thumbnails {
            let part = multipart::Part::bytes(bytes)
                .file_name(format!("{}.jpg", name))
                .mime_str("image/jpeg")?;
            form = form.part(name, part);
        }

        let url = format!("{}{}/sendMediaGroup", self.api_url, self.bot_token);
        let response = self.client.post(&url).multipart(form).send();

        match self.handle_response("Failed to send media group:", response) {
            Ok(_) => {
                log_info!(
                    "{} items sent to {} as media group",
                    items.len(),
                    self.chat_name
                );
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn send_single_media(
        &self,
        chat_id: &str,
        item: &MediaItem,
        caption: Option<&str>,
        reply_markup: Option<&str>,
        spoiler: bool,
    ) -> Result<()> {
        let reader = utils::progress_reader_for_path(&item.path, &item.file_name)?;

        let mut form = multipart::Form::new().part(
            item.media_type.clone(),
            multipart::Part::reader(reader).file_name(item.file_name.clone()),
        );

        form = form.text("chat_id", chat_id.to_string());

        if item.media_type == "video" {
            form = form.text("supports_streaming", "true");
        }

        if let Some(metadata) = item.metadata.as_ref() {
            match metadata {
                utils::MediaMetadata::Video(video_meta) => {
                    if let Some(duration) = video_meta.duration {
                        form = form.text("duration", duration.to_string());
                    }
                    if let Some(width) = video_meta.width {
                        form = form.text("width", width.to_string());
                    }
                    if let Some(height) = video_meta.height {
                        form = form.text("height", height.to_string());
                    }
                    if let Some(bytes) = video_meta.thumbnail.as_ref() {
                        let part = multipart::Part::bytes(bytes.clone())
                            .file_name("thumbnail.jpg")
                            .mime_str("image/jpeg")?;
                        form = form.part("thumbnail", part);
                    }
                }
                utils::MediaMetadata::Photo { thumbnail } => {
                    if let Some(bytes) = thumbnail.as_ref() {
                        let part = multipart::Part::bytes(bytes.clone())
                            .file_name("thumbnail.jpg")
                            .mime_str("image/jpeg")?;
                        form = form.part("thumbnail", part);
                    }
                }
            }
        }

        if let Some(caption) = caption {
            form = form.text("caption", caption.to_string());
        }
        if let Some(markup) = reply_markup {
            form = form.text("reply_markup", markup.to_string());
        }
        if spoiler && matches!(item.media_type.as_str(), "photo" | "video") {
            form = form.text("has_spoiler", "true".to_string());
        }

        let endpoint = format!(
            "{}{}/send{}",
            self.api_url,
            self.bot_token,
            utils::capitalize(&item.media_type)
        );
        let response = self.client.post(&endpoint).multipart(form).send();

        match self.handle_response("Failed to send media file:", response) {
            Ok(_) => {
                log_info!(
                    "Single media file sent to {}: {}",
                    self.chat_name,
                    item.file_name
                );
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn send_chat_action(&mut self, chat_id: &str, action: &str) {
        self.chat_name = "Unknown".to_string();

        let action_url = format!("{}{}/sendChatAction", self.api_url, self.bot_token);
        let response = self
            .client
            .post(&action_url)
            .form(&[("chat_id", chat_id), ("action", action)])
            .send();

        if let Err(err) = self.handle_response("Failed to send chat action:", response) {
            log_debug!("{}", err);
        }

        let chat_url = format!("{}{}/getChat", self.api_url, self.bot_token);
        let response = self
            .client
            .get(&chat_url)
            .query(&[("chat_id", chat_id)])
            .send();

        match response {
            Ok(resp) => self.apply_chat_name(resp),
            Err(err) => {
                let error = anyhow!(err);
                self.log_exception("Failed to get chat name:", &error, None, None);
            }
        }
    }

    fn apply_chat_name(&mut self, response: reqwest::blocking::Response) {
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if status.is_success() {
            if let Ok(chat_info) = serde_json::from_str::<ChatResponse>(&text) {
                if chat_info.ok {
                    if let Some(result) = chat_info.result {
                        if let Some(title) = result.title {
                            self.chat_name = title;
                            return;
                        }
                        if let Some(first) = result.first_name {
                            let mut full = first;
                            if let Some(last) = result.last_name {
                                if !last.trim().is_empty() {
                                    full.push(' ');
                                    full.push_str(&last);
                                }
                            }
                            let trimmed = full.trim();
                            self.chat_name = if trimmed.is_empty() {
                                "Unknown".to_string()
                            } else {
                                trimmed.to_string()
                            };
                            return;
                        }
                    }
                } else if let Some(description) = chat_info.description {
                    self.chat_name = format!("Error: {}", description);
                    return;
                }
            }
            self.chat_name = "Unknown".to_string();
        } else {
            let err = anyhow!("telegram API returned status {}", status.as_u16());
            self.log_exception("Failed to get chat name:", &err, Some(status), Some(&text));
        }
    }

    fn check(&mut self, chat_id: &str) -> Result<()> {
        let actions = [
            "typing",
            "upload_photo",
            "record_video",
            "upload_video",
            "record_voice",
            "upload_voice",
            "upload_document",
            "choose_sticker",
            "find_location",
            "record_video_note",
            "upload_video_note",
        ];

        let mut rng = StdRng::from_entropy();
        let action = actions[rng.gen_range(0..actions.len())];

        let payload = json!({
            "chat_id": chat_id,
            "action": action,
        });

        let url = format!("{}{}/sendChatAction", self.api_url, self.bot_token);
        let start = Instant::now();
        let response = self.client.post(&url).json(&payload).send();

        match self.handle_response("Failed to send chat action:", response) {
            Ok(_) => {
                let elapsed = start.elapsed().as_millis();
                log_info!("{} API Response time: {} ms", self.api_url, elapsed);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn handle_response(
        &self,
        context: &str,
        response: reqwest::Result<reqwest::blocking::Response>,
    ) -> Result<String> {
        match response {
            Ok(resp) => self.ensure_success(context, resp),
            Err(err) => {
                let error = anyhow!(err);
                self.log_exception(context, &error, None, None);
                Err(error)
            }
        }
    }

    fn ensure_success(
        &self,
        context: &str,
        response: reqwest::blocking::Response,
    ) -> Result<String> {
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if status.is_success() {
            Ok(text)
        } else {
            let err = anyhow!("telegram API returned status {}", status);
            self.log_exception(context, &err, Some(status), Some(&text));
            Err(err)
        }
    }

    fn log_exception(
        &self,
        context: &str,
        error: &anyhow::Error,
        status: Option<StatusCode>,
        response: Option<&str>,
    ) {
        let sanitized = error.to_string().replace(&self.bot_token, "REDACTED");
        log_error!("{} {}", context, sanitized);
        if let Some(status) = status {
            if let Some(body) = response {
                log_debug!("HTTP Status Code: {}, Response: {}", status.as_u16(), body);
            }
        }
    }
}

#[derive(Serialize)]
struct InputMedia {
    #[serde(rename = "type")]
    media_type: String,
    media: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    has_spoiler: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumbnail: Option<String>,
}

#[derive(serde::Deserialize)]
struct ChatResponse {
    ok: bool,
    result: Option<ChatResult>,
    description: Option<String>,
}

#[derive(serde::Deserialize)]
struct ChatResult {
    title: Option<String>,
    #[serde(rename = "first_name")]
    first_name: Option<String>,
    #[serde(rename = "last_name")]
    last_name: Option<String>,
}

#[derive(Clone)]
struct MediaItem {
    media_type: String,
    file_name: String,
    path: PathBuf,
    caption: Option<String>,
    spoiler: bool,
    metadata: Option<utils::MediaMetadata>,
    part_name: String,
}
