use crate::utils;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct InputMedia {
    #[serde(rename = "type")]
    pub media_type: String,
    pub media: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_spoiler: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_streaming: Option<bool>,
}

#[derive(serde::Deserialize)]
pub struct ChatResponse {
    pub ok: bool,
    pub result: Option<ChatResult>,
    pub description: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct ChatResult {
    pub title: Option<String>,
    #[serde(rename = "first_name")]
    pub first_name: Option<String>,
    #[serde(rename = "last_name")]
    pub last_name: Option<String>,
}

#[derive(Clone)]
pub struct MediaItem {
    pub media_type: String,
    pub file_name: String,
    pub path: PathBuf,
    pub caption: Option<String>,
    pub spoiler: bool,
    pub metadata: Option<utils::MediaMetadata>,
    pub part_name: String,
}
