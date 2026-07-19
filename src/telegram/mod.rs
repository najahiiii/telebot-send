mod media;
mod message;
pub mod types;

use crate::args::Args;
use crate::utils;
use crate::{log_debug, log_error, log_info};
use anyhow::{Result, anyhow};
use rand::{Rng, SeedableRng, rngs::StdRng};
use reqwest::StatusCode;
use reqwest::blocking::{Client, multipart};
use serde_json::Value;
use std::time::{Duration, Instant};

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
                self.check(&chat_id, args.thread_id)?;
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
                args.video_note,
                args.voice,
                args.no_group,
                &args.buttons,
                args.spoiler,
                args.streaming,
                args.delay_secs,
                args.thread_id,
            )?;
            return Ok(());
        }

        if let Some(message) = &args.message {
            let reply_markup = utils::create_reply_markup(&args.buttons);
            let chat_id = self.chat_id.clone();
            self.send_message(
                &chat_id,
                message,
                args.silent,
                reply_markup.as_ref(),
                args.thread_id,
            )?;
            return Ok(());
        }

        Err(anyhow!("No message or media provided."))
    }

    pub(super) fn target_label(&self, thread_id: Option<i64>) -> String {
        if let Some(id) = thread_id {
            return format!("{} · Topic #{}", self.chat_name, id);
        }
        self.chat_name.clone()
    }

    fn send_media(
        &mut self,
        chat_id: &str,
        media_paths: &[std::path::PathBuf],
        caption: Option<&str>,
        as_file: bool,
        video_note: bool,
        voice: bool,
        no_group: bool,
        buttons: &[crate::args::ButtonSpec],
        spoiler: bool,
        streaming: bool,
        delay_secs: Option<u64>,
        thread_id: Option<i64>,
    ) -> Result<()> {
        use types::MediaItem;

        let reply_markup_json = utils::create_reply_markup(buttons);
        let reply_markup_text = reply_markup_json
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok());

        let mut media_items = Vec::new();
        let mut caption_assigned = false;
        let mut send_calls = 0usize;
        let delay = delay_secs.unwrap_or(0);
        let maybe_delay = |calls: usize| {
            if calls > 0 && delay > 0 {
                log_info!(
                    "Waiting {} s before next media request to reduce rate limiting",
                    delay
                );
                std::thread::sleep(Duration::from_secs(delay));
            }
        };

        for path in media_paths {
            if !utils::is_regular_file(path) {
                log_error!("File not found: {}", path.display());
                continue;
            }

            let mime_type = utils::detect_mime_type(path);
            let mut media_type = if as_file {
                "document"
            } else if video_note
                && matches!(mime_type.as_deref(), Some(mt) if mt.starts_with("video/"))
            {
                "video_note"
            } else {
                utils::determine_media_type(mime_type.as_deref())
            };

            if !matches!(
                media_type,
                "photo" | "video" | "audio" | "document" | "animation" | "voice" | "video_note"
            ) {
                log_error!(
                    "Unsupported media type for {}: {}",
                    path.display(),
                    media_type
                );
                continue;
            }

            let mut converted_path: Option<std::path::PathBuf> = None;
            if voice && !utils::is_ogg(mime_type.as_deref()) {
                match utils::convert_to_ogg(path) {
                    Ok(Some(ogg_path)) => {
                        converted_path = Some(ogg_path.clone());
                        media_type = "voice";
                        log_info!("File will be sent as voice message: {}", ogg_path.display());
                    }
                    Ok(None) => {
                        log_info!(
                            "OGG conversion not available, sending as {} : {}",
                            media_type,
                            path.display()
                        );
                    }
                    Err(err) => {
                        log_error!(
                            "Failed to convert to OGG: {}. Sending as {}.",
                            err,
                            media_type
                        );
                    }
                }
            } else if voice && utils::is_ogg(mime_type.as_deref()) {
                media_type = "voice";
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
            let is_animation_file = utils::is_animation(mime_type.as_deref());
            let is_image_file = matches!(mime_type.as_deref(), Some(mt) if mt.starts_with("image/") && mt != "image/gif");

            let metadata = if media_type == "voice" && converted_path.is_some() {
                let ogg_path = converted_path.as_ref().unwrap();
                log_info!("Extracting audio duration from: {}", ogg_path.display());
                match utils::extract_audio_duration(ogg_path) {
                    Ok(Some(duration)) => {
                        log_info!("Audio duration: {}s", duration);
                        Some(utils::MediaMetadata::Video(utils::VideoMetadata {
                            duration: Some(duration),
                            width: None,
                            height: None,
                            thumbnail: None,
                        }))
                    }
                    Ok(None) => {
                        log_debug!("No duration found in OGG file");
                        None
                    }
                    Err(err) => {
                        log_debug!("Failed to extract audio duration: {}", err);
                        None
                    }
                }
            } else if is_video_file || is_animation_file {
                log_info!("Extracting metadata from {}", path.display());
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
            let effective_path = converted_path.as_ref().unwrap_or(path);
            let effective_name = if converted_path.is_some() {
                effective_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("voice.ogg")
                    .to_string()
            } else {
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("media")
                    .to_string()
            };

            media_items.push(MediaItem {
                media_type: media_type.to_string(),
                file_name: effective_name,
                path: effective_path.clone(),
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
            let current_type = media_items[index].media_type.as_str();

            if current_type == "animation" {
                let item = &media_items[index];
                self.send_chat_action(chat_id, "upload_document", thread_id);
                let caption_to_use = item.caption.as_deref().or(caption);
                maybe_delay(send_calls);
                self.send_animation(
                    chat_id,
                    item,
                    caption_to_use,
                    reply_markup_text.as_deref(),
                    item.spoiler,
                    thread_id,
                )?;
                send_calls += 1;
                index += 1;
                continue;
            }

            if current_type == "voice" {
                let item = &media_items[index];
                self.send_chat_action(chat_id, "upload_voice", thread_id);
                let caption_to_use = item.caption.as_deref().or(caption);
                maybe_delay(send_calls);
                self.send_voice(
                    chat_id,
                    item,
                    caption_to_use,
                    reply_markup_text.as_deref(),
                    thread_id,
                )?;
                send_calls += 1;
                index += 1;
                continue;
            }

            if current_type == "video_note" {
                let item = &media_items[index];
                self.send_chat_action(chat_id, "upload_video_note", thread_id);
                maybe_delay(send_calls);
                self.send_video_note(chat_id, item, reply_markup_text.as_deref(), thread_id)?;
                send_calls += 1;
                index += 1;
                continue;
            }

            if current_type == "photo"
                && index + 1 < media_items.len()
                && media_items[index + 1].media_type == "video"
            {
                let photo_item = &media_items[index];
                let video_item = &media_items[index + 1];
                self.send_chat_action(chat_id, "upload_photo", thread_id);
                let caption_to_use = photo_item.caption.as_deref().or(caption);
                maybe_delay(send_calls);
                self.send_live_photo(
                    chat_id,
                    photo_item,
                    video_item,
                    caption_to_use,
                    reply_markup_text.as_deref(),
                    photo_item.spoiler,
                    thread_id,
                )?;
                send_calls += 1;
                index += 2;
                continue;
            }

            if current_type == "document" {
                if no_group {
                    let item = &media_items[index];
                    self.send_chat_action(chat_id, "upload_document", thread_id);
                    let caption_to_use = item.caption.as_deref().or(caption);
                    maybe_delay(send_calls);
                    self.send_single_media(
                        chat_id,
                        item,
                        caption_to_use,
                        reply_markup_text.as_deref(),
                        item.spoiler,
                        streaming,
                        thread_id,
                    )?;
                    send_calls += 1;
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
                    self.send_chat_action(chat_id, "upload_document", thread_id);
                    let caption_to_use = item.caption.as_deref().or(caption);
                    maybe_delay(send_calls);
                    self.send_single_media(
                        chat_id,
                        item,
                        caption_to_use,
                        reply_markup_text.as_deref(),
                        item.spoiler,
                        streaming,
                        thread_id,
                    )?;
                    send_calls += 1;
                    continue;
                }

                self.send_chat_action(chat_id, "upload_document", thread_id);
                let chunk_items: Vec<MediaItem> = chunk_indices
                    .iter()
                    .map(|&idx| media_items[idx].clone())
                    .collect();
                maybe_delay(send_calls);
                self.send_media_group(
                    chat_id,
                    &chunk_items,
                    reply_markup_text.as_deref(),
                    streaming,
                    thread_id,
                )?;
                send_calls += 1;
                continue;
            }

            let mut chunk_indices = Vec::new();
            while index < media_items.len()
                && chunk_indices.len() < 10
                && !matches!(
                    media_items[index].media_type.as_str(),
                    "document" | "animation" | "voice" | "video_note"
                )
            {
                chunk_indices.push(index);
                index += 1;
            }

            if chunk_indices.is_empty() {
                index += 1;
                continue;
            }

            if no_group || chunk_indices.len() == 1 {
                for idx in chunk_indices {
                    let item = &media_items[idx];
                    let action = format!("upload_{}", item.media_type.to_lowercase());
                    self.send_chat_action(chat_id, &action, thread_id);
                    let caption_to_use = item.caption.as_deref().or(caption);
                    maybe_delay(send_calls);
                    self.send_single_media(
                        chat_id,
                        item,
                        caption_to_use,
                        reply_markup_text.as_deref(),
                        item.spoiler,
                        streaming,
                        thread_id,
                    )?;
                    send_calls += 1;
                }
                continue;
            }

            let first_item = &media_items[chunk_indices[0]];
            let action = format!("upload_{}", first_item.media_type.to_lowercase());
            self.send_chat_action(chat_id, &action, thread_id);
            let chunk_items: Vec<MediaItem> = chunk_indices
                .iter()
                .map(|&idx| media_items[idx].clone())
                .collect();
            maybe_delay(send_calls);
            self.send_media_group(
                chat_id,
                &chunk_items,
                reply_markup_text.as_deref(),
                streaming,
                thread_id,
            )?;
            send_calls += 1;
        }

        Ok(())
    }

    fn send_chat_action(&mut self, chat_id: &str, action: &str, thread_id: Option<i64>) {
        self.chat_name = "Unknown".to_string();

        let action_url = format!("{}{}/sendChatAction", self.api_url, self.bot_token);
        let mut form = vec![
            ("chat_id".to_string(), chat_id.to_string()),
            ("action".to_string(), action.to_string()),
        ];
        if let Some(id) = thread_id {
            form.push(("message_thread_id".to_string(), id.to_string()));
        }
        let response = self.client.post(&action_url).form(&form).send();

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
            if let Ok(chat_info) = serde_json::from_str::<types::ChatResponse>(&text) {
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

    fn check(&mut self, chat_id: &str, thread_id: Option<i64>) -> Result<()> {
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

        let mut payload = serde_json::json!({
            "chat_id": chat_id,
            "action": action,
        });

        if let Some(id) = thread_id {
            payload["message_thread_id"] = serde_json::json!(id);
        }

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

    pub(super) fn send_multipart_with_retry<F>(
        &self,
        context: &str,
        endpoint: &str,
        build_form: F,
    ) -> Result<String>
    where
        F: Fn() -> Result<multipart::Form>,
    {
        let mut attempt = 0;
        let max_retries = 3;

        loop {
            let form = build_form()?;
            let response = self.client.post(endpoint).multipart(form).send();

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.text().unwrap_or_default();
                    if status.is_success() {
                        return Ok(text);
                    }

                    if attempt < max_retries {
                        if let Some(wait) = Self::retry_after_secs(status, &text) {
                            attempt += 1;
                            log_info!(
                                "Rate limited: retrying in {} s (attempt {} of {})",
                                wait,
                                attempt,
                                max_retries
                            );
                            std::thread::sleep(Duration::from_secs(wait));
                            continue;
                        }
                    }

                    let err = anyhow!("telegram API returned status {}", status);
                    self.log_exception(context, &err, Some(status), Some(&text));
                    return Err(err);
                }
                Err(err) => {
                    let error = anyhow!(err);
                    self.log_exception(context, &error, None, None);
                    return Err(error);
                }
            }
        }
    }

    fn retry_after_secs(status: StatusCode, body: &str) -> Option<u64> {
        if status == StatusCode::TOO_MANY_REQUESTS {
            if let Some(val) = Self::retry_after_from_body(body) {
                return Some(val);
            }
            return Some(1);
        }

        if body.to_ascii_lowercase().contains("too many requests") {
            if let Some(val) = Self::retry_after_from_body(body) {
                return Some(val);
            }
        }

        None
    }

    fn retry_after_from_body(body: &str) -> Option<u64> {
        if let Ok(value) = serde_json::from_str::<Value>(body) {
            if let Some(val) = value
                .get("parameters")
                .and_then(|p| p.get("retry_after"))
                .and_then(|v| v.as_u64())
            {
                return Some(val);
            }
        }

        let needle = "retry after";
        if let Some(idx) = body.to_ascii_lowercase().find(needle) {
            let tail = &body[idx + needle.len()..];
            for token in tail.split(|c: char| !c.is_ascii_digit()) {
                if token.is_empty() {
                    continue;
                }
                if let Ok(val) = token.parse::<u64>() {
                    return Some(val);
                }
            }
        }

        None
    }
}
