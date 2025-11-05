use crate::{log_debug, log_error, log_info};
use anyhow::{Context, anyhow};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use mime_guess::MimeGuess;
use rand::Rng;
use serde_json::{Value, json};
use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub(crate) fn redact_token(token: &str) -> String {
    if token.len() <= 10 {
        return "REDACTED".to_string();
    }
    format!("{}{}", &token[..10], "*".repeat(30))
}

pub(crate) fn detect_mime_type(path: &Path) -> Option<String> {
    let guess = MimeGuess::from_path(path).first_raw();
    if guess.is_some() {
        return guess.map(ToString::to_string);
    }

    let mut file = File::open(path).ok()?;
    let mut buffer = [0u8; 512];
    let read = file.read(&mut buffer).ok()?;
    Some(
        infer::Infer::new()
            .get(&buffer[..read])?
            .mime_type()
            .to_string(),
    )
}

pub(crate) fn determine_media_type(mime_type: Option<&str>) -> &'static str {
    match mime_type {
        Some(mt) if mt.starts_with("image/") => "photo",
        Some(mt) if mt.starts_with("video/") => "video",
        Some(mt) if mt.starts_with("audio/") => "audio",
        _ => "document",
    }
}

pub(crate) fn create_reply_markup(
    button_text: &Option<String>,
    button_url: &Option<String>,
) -> Option<Value> {
    match (button_text, button_url) {
        (Some(text), Some(url)) => Some(json!({
            "inline_keyboard": [[{"text": text, "url": url}]]
        })),
        (Some(_), None) | (None, Some(_)) => {
            log_error!("Both button_text and button_url must be provided.");
            None
        }
        (None, None) => None,
    }
}

pub(crate) fn validate_defaults(
    provided_api_url: bool,
    provided_bot_token: bool,
    provided_chat_id: bool,
    api_url: &str,
    bot_token: &str,
    chat_id: &str,
) {
    if !provided_bot_token && !provided_chat_id {
        if !provided_api_url {
            log_info!("Using API URL from config: {}", api_url);
        }
        log_info!(
            "Using bot token and chat ID from config: {}, {}",
            redact_token(bot_token),
            chat_id
        );
    }
}

pub(crate) fn is_regular_file(path: &Path) -> bool {
    path.is_file()
}

pub(crate) fn capitalize(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

pub struct ProgressReader<R> {
    inner: R,
    progress: ProgressBar,
    label: String,
    started: bool,
    finished: bool,
}

impl<R> ProgressReader<R> {
    fn new(inner: R, progress: ProgressBar, label: String, started: bool, finished: bool) -> Self {
        Self {
            inner,
            progress,
            label,
            started,
            finished,
        }
    }

    fn start_if_needed(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        self.progress.set_draw_target(ProgressDrawTarget::stdout());
        self.progress.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} {msg:<25} [{bar:25.cyan/blue}] {decimal_bytes}/{decimal_total_bytes} {decimal_bytes_per_sec} ({eta}) {percent}%",
            )
            .unwrap()
            .progress_chars("#>-")
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈"),
        );
        self.progress.set_message(self.label.clone());
        self.progress.enable_steady_tick(Duration::from_millis(100));
    }
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let amount = self.inner.read(buf)?;
        if amount == 0 {
            if !self.finished {
                self.start_if_needed();
                self.progress.finish_and_clear();
                log_info!("Waiting for Telegram to process {}", self.label.clone());
                self.finished = true;
            }
        } else {
            self.start_if_needed();
            self.progress.inc(amount as u64);
        }
        Ok(amount)
    }
}

impl<R> Drop for ProgressReader<R> {
    fn drop(&mut self) {
        if !self.finished {
            self.start_if_needed();
            self.progress.finish_and_clear();
            log_info!("Waiting for Telegram to process {}", self.label.clone());
            self.finished = true;
        }
    }
}

fn truncate_label(label: &str, max_chars: usize) -> String {
    let mut result = String::new();
    let mut count = 0;
    for ch in label.chars() {
        if count + 1 > max_chars {
            result.push('…');
            return result;
        }
        result.push(ch);
        count += 1;
    }
    result
}

pub fn progress_reader_for_path(path: &Path, label: &str) -> anyhow::Result<ProgressReader<File>> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open media {} for upload", path.display()))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
    let total_bytes = metadata.len();
    let truncated = truncate_label(label, 24);

    let progress = ProgressBar::new(total_bytes);
    progress.set_draw_target(ProgressDrawTarget::hidden());

    let (started, finished) = if total_bytes == 0 {
        (false, true)
    } else {
        (false, false)
    };

    Ok(ProgressReader::new(
        file, progress, truncated, started, finished,
    ))
}

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub duration: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub thumbnail: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub enum MediaMetadata {
    Video(VideoMetadata),
    Photo { thumbnail: Option<Vec<u8>> },
}

pub fn extract_video_metadata(path: &Path) -> anyhow::Result<Option<VideoMetadata>> {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => {
            log_debug!(
                "Skipping metadata extraction for {} because the path is not valid UTF-8.",
                path.display()
            );
            return Ok(None);
        }
    };

    let ffprobe_output = match Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height,duration")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("json")
        .arg(path_str)
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                log_debug!("ffprobe not found; skipping video metadata extraction.");
                return Ok(None);
            }
            return Err(anyhow!(err).context("Failed to spawn ffprobe process"));
        }
    };

    if !ffprobe_output.status.success() {
        log_debug!(
            "ffprobe failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&ffprobe_output.stderr)
        );
        return Ok(None);
    }

    let value: Value = serde_json::from_slice(&ffprobe_output.stdout)
        .context("Failed to parse ffprobe JSON output")?;

    let stream = value
        .get("streams")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .cloned();

    let stream = match stream {
        Some(stream) => stream,
        None => {
            log_debug!("No video stream data found for {}", path.display());
            return Ok(None);
        }
    };

    let parse_duration = |value: Option<&Value>| -> Option<f64> {
        value.and_then(|v| {
            if let Some(n) = v.as_f64() {
                Some(n)
            } else if let Some(s) = v.as_str() {
                s.parse::<f64>().ok()
            } else {
                None
            }
        })
    };

    let mut duration_secs = parse_duration(stream.get("duration"))
        .or_else(|| parse_duration(value.get("format").and_then(|f| f.get("duration"))));

    if let Some(d) = duration_secs.as_mut() {
        if !d.is_finite() || *d < 0.0 {
            *d = 0.0;
        }
    }

    let width = stream
        .get("width")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let height = stream
        .get("height")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let duration = duration_secs.map(|d| d.floor() as u64);

    let mut rng = rand::thread_rng();
    let start_seconds = duration_secs
        .filter(|d| *d > 0.0)
        .map(|d| if d <= 1.0 { 0.0 } else { rng.gen_range(0.0..d) });

    let thumbnail = match start_seconds {
        Some(position) => match generate_thumbnail(path_str, position) {
            Ok(bytes) => bytes,
            Err(err) => {
                log_debug!(
                    "Failed to generate thumbnail for {}: {}",
                    path.display(),
                    err
                );
                None
            }
        },
        None => match generate_thumbnail(path_str, 0.0) {
            Ok(bytes) => bytes,
            Err(err) => {
                log_debug!(
                    "Failed to generate thumbnail for {}: {}",
                    path.display(),
                    err
                );
                None
            }
        },
    };

    Ok(Some(VideoMetadata {
        duration,
        width,
        height,
        thumbnail,
    }))
}

pub fn extract_photo_metadata(path: &Path) -> anyhow::Result<Option<Option<Vec<u8>>>> {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => {
            log_debug!(
                "Skipping photo metadata extraction for {} because the path is not valid UTF-8.",
                path.display()
            );
            return Ok(None);
        }
    };

    let output = match Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(path_str)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg("scale=320:320:force_original_aspect_ratio=decrease")
        .arg("-f")
        .arg("mjpeg")
        .arg("pipe:1")
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                log_debug!("ffmpeg not found; skipping photo thumbnail generation.");
                return Ok(Some(None));
            }
            return Err(anyhow!(err).context("Failed to spawn ffmpeg process for photo"));
        }
    };

    if !output.status.success() {
        log_debug!(
            "ffmpeg failed to generate photo thumbnail: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(Some(None));
    }

    if output.stdout.is_empty() {
        log_debug!("ffmpeg produced an empty photo thumbnail output.");
        return Ok(Some(None));
    }

    if output.stdout.len() > 200_000 {
        log_debug!("Generated photo thumbnail is larger than 200 kB; discarding.");
        return Ok(Some(None));
    }

    Ok(Some(Some(output.stdout)))
}

fn generate_thumbnail(path: &str, timestamp: f64) -> anyhow::Result<Option<Vec<u8>>> {
    let ffmpeg_output = match Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-ss")
        .arg(format!("{:.2}", timestamp.max(0.0)))
        .arg("-i")
        .arg(path)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg("scale=320:320:force_original_aspect_ratio=decrease")
        .arg("-f")
        .arg("mjpeg")
        .arg("pipe:1")
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                log_debug!("ffmpeg not found; skipping thumbnail generation.");
                return Ok(None);
            }
            return Err(anyhow!(err).context("Failed to spawn ffmpeg process"));
        }
    };

    if !ffmpeg_output.status.success() {
        log_debug!(
            "ffmpeg failed to generate thumbnail: {}",
            String::from_utf8_lossy(&ffmpeg_output.stderr)
        );
        return Ok(None);
    }

    if ffmpeg_output.stdout.is_empty() {
        log_debug!("ffmpeg produced an empty thumbnail output.");
        return Ok(None);
    }

    if ffmpeg_output.stdout.len() > 200_000 {
        log_debug!("Generated thumbnail is larger than 200 kB; discarding.");
        return Ok(None);
    }

    Ok(Some(ffmpeg_output.stdout))
}
