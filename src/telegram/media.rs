use crate::utils;
use crate::{log_debug, log_error, log_info};
use anyhow::Result;
use reqwest::blocking::multipart;

use super::SendTg;
use super::types::{InputMedia, MediaItem};

impl SendTg {
    pub(super) fn send_media_group(
        &self,
        chat_id: &str,
        items: &[MediaItem],
        reply_markup: Option<&str>,
        streaming: bool,
        thread_id: Option<i64>,
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
                supports_streaming: None,
            };

            if streaming && item.media_type == "video" {
                entry.supports_streaming = Some(true);
            }

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

        let url = format!("{}{}/sendMediaGroup", self.api_url, self.bot_token);
        self.send_multipart_with_retry("Failed to send media group:", &url, || {
            let mut rebuilt_form = multipart::Form::new()
                .text("chat_id", chat_id.to_string())
                .text("media", serde_json::to_string(&media_payload)?);

            if let Some(id) = thread_id {
                rebuilt_form = rebuilt_form.text("message_thread_id", id.to_string());
            }

            if let Some(markup) = reply_markup {
                rebuilt_form = rebuilt_form.text("reply_markup", markup.to_string());
            }

            for item in items {
                let reader = utils::progress_reader_for_path(&item.path, &item.file_name)?;
                let part = multipart::Part::reader(reader).file_name(item.file_name.clone());
                rebuilt_form = rebuilt_form.part(item.part_name.clone(), part);
            }

            for (name, bytes) in &thumbnails {
                let part = multipart::Part::bytes(bytes.clone())
                    .file_name(format!("{}.jpg", name))
                    .mime_str("image/jpeg")?;
                rebuilt_form = rebuilt_form.part(name.clone(), part);
            }

            Ok(rebuilt_form)
        })?;

        let target = self.target_label(thread_id);
        log_info!("{} items sent to {} as media group", items.len(), target);
        Ok(())
    }

    pub(super) fn send_animation(
        &self,
        chat_id: &str,
        item: &MediaItem,
        caption: Option<&str>,
        reply_markup: Option<&str>,
        spoiler: bool,
        thread_id: Option<i64>,
    ) -> Result<()> {
        let endpoint = format!("{}{}/sendAnimation", self.api_url, self.bot_token);
        self.send_multipart_with_retry("Failed to send animation:", &endpoint, || {
            let reader = utils::progress_reader_for_path(&item.path, &item.file_name)?;

            let mut fresh_form = multipart::Form::new().part(
                "animation",
                multipart::Part::reader(reader).file_name(item.file_name.clone()),
            );

            fresh_form = self.add_base_fields(fresh_form, chat_id, thread_id);

            if let Some(metadata) = item.metadata.as_ref() {
                if let utils::MediaMetadata::Video(video_meta) = metadata {
                    fresh_form = self.add_video_metadata(fresh_form, video_meta);
                }
            }

            fresh_form = self.add_optional_fields(fresh_form, caption, reply_markup, spoiler);

            Ok(fresh_form)
        })?;

        let target = self.target_label(thread_id);
        log_info!("Animation sent to {}: {}", target, item.file_name);
        Ok(())
    }

    pub(super) fn send_voice(
        &self,
        chat_id: &str,
        item: &MediaItem,
        caption: Option<&str>,
        reply_markup: Option<&str>,
        thread_id: Option<i64>,
    ) -> Result<()> {
        let endpoint = format!("{}{}/sendVoice", self.api_url, self.bot_token);
        self.send_multipart_with_retry("Failed to send voice:", &endpoint, || {
            let reader = utils::progress_reader_for_path(&item.path, &item.file_name)?;

            let mut fresh_form = multipart::Form::new().part(
                "voice",
                multipart::Part::reader(reader).file_name(item.file_name.clone()),
            );

            fresh_form = self.add_base_fields(fresh_form, chat_id, thread_id);

            if let Some(metadata) = item.metadata.as_ref() {
                if let utils::MediaMetadata::Video(video_meta) = metadata {
                    if let Some(duration) = video_meta.duration {
                        fresh_form = fresh_form.text("duration", duration.to_string());
                        log_debug!("Voice duration: {}s", duration);
                    }
                }
            }

            fresh_form = self.add_optional_fields(fresh_form, caption, reply_markup, false);

            Ok(fresh_form)
        })?;

        let target = self.target_label(thread_id);
        log_info!("Voice message sent to {}: {}", target, item.file_name);
        Ok(())
    }

    pub(super) fn send_video_note(
        &self,
        chat_id: &str,
        item: &MediaItem,
        reply_markup: Option<&str>,
        thread_id: Option<i64>,
    ) -> Result<()> {
        const VIDEO_NOTE_MAX_BYTES: u64 = 12 * 1024 * 1024;

        let file_size = std::fs::metadata(&item.path).map(|m| m.len()).unwrap_or(0);
        let effective_path = if file_size > VIDEO_NOTE_MAX_BYTES {
            let duration = item.metadata.as_ref().and_then(|m| match m {
                utils::MediaMetadata::Video(v) => v.duration,
                _ => None,
            });
            match utils::compress_video_for_note(&item.path, VIDEO_NOTE_MAX_BYTES, duration) {
                Ok(Some(compressed)) => compressed,
                Ok(None) => item.path.clone(),
                Err(err) => {
                    log_error!("Failed to compress video for note: {}", err);
                    item.path.clone()
                }
            }
        } else {
            item.path.clone()
        };

        let effective_name = effective_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&item.file_name)
            .to_string();

        let endpoint = format!("{}{}/sendVideoNote", self.api_url, self.bot_token);
        self.send_multipart_with_retry("Failed to send video note:", &endpoint, || {
            let reader = utils::progress_reader_for_path(&effective_path, &effective_name)?;

            let mut fresh_form = multipart::Form::new().part(
                "video_note",
                multipart::Part::reader(reader).file_name(effective_name.clone()),
            );

            fresh_form = self.add_base_fields(fresh_form, chat_id, thread_id);

            if let Some(metadata) = item.metadata.as_ref() {
                if let utils::MediaMetadata::Video(video_meta) = metadata {
                    if let Some(duration) = video_meta.duration {
                        let capped = duration.min(60);
                        fresh_form = fresh_form.text("duration", capped.to_string());
                    }
                    if let Some(bytes) = video_meta.thumbnail.as_ref() {
                        let part = multipart::Part::bytes(bytes.clone())
                            .file_name("thumbnail.jpg")
                            .mime_str("image/jpeg")?;
                        fresh_form = fresh_form.part("thumbnail", part);
                    }
                }
            }

            let length = if effective_path != item.path {
                360
            } else {
                item.metadata
                    .as_ref()
                    .and_then(|m| match m {
                        utils::MediaMetadata::Video(v) => v.width.or(v.height),
                        _ => None,
                    })
                    .unwrap_or(360)
            };
            fresh_form = fresh_form.text("length", length.to_string());

            if let Some(markup) = reply_markup {
                fresh_form = fresh_form.text("reply_markup", markup.to_string());
            }

            Ok(fresh_form)
        })?;

        let target = self.target_label(thread_id);
        log_info!("Video note sent to {}: {}", target, item.file_name);
        Ok(())
    }

    pub(super) fn send_live_photo(
        &self,
        chat_id: &str,
        photo_item: &MediaItem,
        video_item: &MediaItem,
        caption: Option<&str>,
        reply_markup: Option<&str>,
        spoiler: bool,
        thread_id: Option<i64>,
    ) -> Result<()> {
        let endpoint = format!("{}{}/sendLivePhoto", self.api_url, self.bot_token);
        self.send_multipart_with_retry("Failed to send live photo:", &endpoint, || {
            let photo_reader =
                utils::progress_reader_for_path(&photo_item.path, &photo_item.file_name)?;
            let video_reader =
                utils::progress_reader_for_path(&video_item.path, &video_item.file_name)?;

            let mut fresh_form = multipart::Form::new()
                .part(
                    "photo",
                    multipart::Part::reader(photo_reader).file_name(photo_item.file_name.clone()),
                )
                .part(
                    "live_photo",
                    multipart::Part::reader(video_reader).file_name(video_item.file_name.clone()),
                );

            fresh_form = self.add_base_fields(fresh_form, chat_id, thread_id);

            if let Some(metadata) = video_item.metadata.as_ref() {
                if let utils::MediaMetadata::Video(video_meta) = metadata {
                    if let Some(duration) = video_meta.duration {
                        fresh_form = fresh_form.text("duration", duration.to_string());
                    }
                }
            }

            fresh_form = self.add_optional_fields(fresh_form, caption, reply_markup, spoiler);

            Ok(fresh_form)
        })?;

        let target = self.target_label(thread_id);
        log_info!(
            "Live photo sent to {}: {} + {}",
            target,
            photo_item.file_name,
            video_item.file_name
        );
        Ok(())
    }

    pub(super) fn send_single_media(
        &self,
        chat_id: &str,
        item: &MediaItem,
        caption: Option<&str>,
        reply_markup: Option<&str>,
        spoiler: bool,
        streaming: bool,
        thread_id: Option<i64>,
    ) -> Result<()> {
        let endpoint = format!(
            "{}{}/send{}",
            self.api_url,
            self.bot_token,
            utils::capitalize(&item.media_type)
        );
        self.send_multipart_with_retry("Failed to send media file:", &endpoint, || {
            let reader = utils::progress_reader_for_path(&item.path, &item.file_name)?;

            let mut fresh_form = multipart::Form::new().part(
                item.media_type.clone(),
                multipart::Part::reader(reader).file_name(item.file_name.clone()),
            );

            fresh_form = self.add_base_fields(fresh_form, chat_id, thread_id);

            if streaming && item.media_type == "video" {
                fresh_form = fresh_form.text("supports_streaming", "true");
            }

            if let Some(metadata) = item.metadata.as_ref() {
                match metadata {
                    utils::MediaMetadata::Video(video_meta) => {
                        fresh_form = self.add_video_metadata(fresh_form, video_meta);
                    }
                    utils::MediaMetadata::Photo { thumbnail } => {
                        if let Some(bytes) = thumbnail.as_ref() {
                            let part = multipart::Part::bytes(bytes.clone())
                                .file_name("thumbnail.jpg")
                                .mime_str("image/jpeg")?;
                            fresh_form = fresh_form.part("thumbnail", part);
                        }
                    }
                }
            }

            fresh_form = self.add_optional_fields(fresh_form, caption, reply_markup, spoiler);

            Ok(fresh_form)
        })?;

        let target = self.target_label(thread_id);
        log_info!("Single media file sent to {}: {}", target, item.file_name);
        Ok(())
    }

    fn add_base_fields(
        &self,
        form: multipart::Form,
        chat_id: &str,
        thread_id: Option<i64>,
    ) -> multipart::Form {
        let mut form = form.text("chat_id", chat_id.to_string());
        if let Some(id) = thread_id {
            form = form.text("message_thread_id", id.to_string());
        }
        form
    }

    fn add_video_metadata(
        &self,
        form: multipart::Form,
        video_meta: &utils::VideoMetadata,
    ) -> multipart::Form {
        let mut form = form;
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
                .mime_str("image/jpeg")
                .unwrap();
            form = form.part("thumbnail", part);
        }
        form
    }

    fn add_optional_fields(
        &self,
        form: multipart::Form,
        caption: Option<&str>,
        reply_markup: Option<&str>,
        spoiler: bool,
    ) -> multipart::Form {
        let mut form = form;
        if let Some(caption) = caption {
            form = form.text("caption", caption.to_string());
        }
        if let Some(markup) = reply_markup {
            form = form.text("reply_markup", markup.to_string());
        }
        if spoiler {
            form = form.text("has_spoiler", "true".to_string());
        }
        form
    }
}
