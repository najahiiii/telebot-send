use crate::args::ButtonSpec;
use crate::{log_debug, log_error, log_info};
use anyhow::{Context, anyhow};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use mime_guess::MimeGuess;
use rand::Rng;
use serde_json::{Value, json};
use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub fn redact_token(token: &str) -> String {
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
        Some(mt) if mt == "image/gif" => "animation",
        Some(mt) if mt.starts_with("image/") => "photo",
        Some(mt) if mt.starts_with("video/") => "video",
        Some(mt) if mt == "audio/ogg" || mt == "audio/ogg; codecs=opus" => "voice",
        Some(mt) if mt.starts_with("audio/") => "audio",
        _ => "document",
    }
}

pub(crate) fn is_animation(mime_type: Option<&str>) -> bool {
    matches!(mime_type, Some(mt) if mt == "image/gif")
}

pub(crate) fn is_ogg(mime_type: Option<&str>) -> bool {
    matches!(mime_type, Some(mt) if mt == "audio/ogg" || mt == "audio/ogg; codecs=opus")
}

pub(crate) fn convert_to_ogg(input_path: &Path) -> anyhow::Result<Option<PathBuf>> {
    let input_str = match input_path.to_str() {
        Some(s) => s,
        None => return Ok(None),
    };

    let output_path = input_path.with_extension("ogg");
    let output_str = match output_path.to_str() {
        Some(s) => s,
        None => return Ok(None),
    };

    log_info!("Converting {} to OGG/OPUS...", input_path.display());

    let output = match Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input_str)
        .arg("-c:a")
        .arg("libopus")
        .arg("-b:a")
        .arg("48k")
        .arg("-ar")
        .arg("48000")
        .arg("-ac")
        .arg("1")
        .arg("-application")
        .arg("voip")
        .arg(output_str)
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                log_debug!("ffmpeg not found; cannot convert to OGG.");
                return Ok(None);
            }
            return Err(anyhow!(err).context("Failed to spawn ffmpeg for OGG conversion"));
        }
    };

    if !output.status.success() {
        log_error!(
            "ffmpeg OGG conversion failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(None);
    }

    if !output_path.exists() {
        log_error!("ffmpeg completed but output file not found: {}", output_str);
        return Ok(None);
    }

    let file_size = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    log_info!(
        "OGG conversion successful: {} ({} bytes)",
        output_path.display(),
        file_size
    );

    Ok(Some(output_path))
}

pub(crate) fn create_reply_markup(buttons: &[ButtonSpec]) -> Option<Value> {
    if buttons.is_empty() {
        return None;
    }

    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut current_row: Vec<Value> = Vec::new();

    for button in buttons {
        match button {
            ButtonSpec::Link { text, url } => {
                current_row.push(json!({ "text": text, "url": url }));
            }
            ButtonSpec::RowBreak => {
                if !current_row.is_empty() {
                    rows.push(current_row);
                    current_row = Vec::new();
                }
            }
        }
    }

    if !current_row.is_empty() {
        rows.push(current_row);
    }

    if rows.is_empty() {
        None
    } else {
        Some(json!({ "inline_keyboard": rows }))
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

pub fn extract_audio_duration(path: &Path) -> anyhow::Result<Option<u64>> {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => return Ok(None),
    };

    let ffprobe_output = match Command::new("ffprobe")
        .arg("-v")
        .arg("error")
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
                log_debug!("ffprobe not found; cannot extract audio duration.");
                return Ok(None);
            }
            return Err(anyhow!(err).context("Failed to spawn ffprobe for audio duration"));
        }
    };

    if !ffprobe_output.status.success() {
        log_debug!(
            "ffprobe failed for audio duration {}: {}",
            path.display(),
            String::from_utf8_lossy(&ffprobe_output.stderr)
        );
        return Ok(None);
    }

    let value: Value = serde_json::from_slice(&ffprobe_output.stdout)
        .context("Failed to parse ffprobe JSON output")?;

    let duration_secs = value
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|v| {
            if let Some(n) = v.as_f64() {
                Some(n)
            } else if let Some(s) = v.as_str() {
                s.parse::<f64>().ok()
            } else {
                None
            }
        });

    match duration_secs {
        Some(d) if d.is_finite() && d >= 0.0 => Ok(Some(d.floor() as u64)),
        _ => Ok(None),
    }
}

pub fn compress_video_for_note(
    input_path: &Path,
    max_bytes: u64,
    total_duration: Option<u64>,
) -> anyhow::Result<Option<PathBuf>> {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::Duration;

    let input_str = match input_path.to_str() {
        Some(s) => s,
        None => return Ok(None),
    };

    let output_path = input_path.with_extension("note.mp4");
    let output_str = match output_path.to_str() {
        Some(s) => s,
        None => return Ok(None),
    };

    let file_size = std::fs::metadata(input_path).map(|m| m.len()).unwrap_or(0);
    if file_size <= max_bytes {
        return Ok(None);
    }

    let max_mb = max_bytes / (1024 * 1024);
    let input_mb = file_size / (1024 * 1024);
    log_info!(
        "Video {}MB exceeds {}MB limit, compressing for video note...",
        input_mb,
        max_mb
    );

    let mut child = match Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input_str)
        .arg("-t")
        .arg("60")
        .arg("-vf")
        .arg("scale=360:360:force_original_aspect_ratio=decrease,pad=360:360:(ow-iw)/2:(oh-ih)/2")
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("fast")
        .arg("-crf")
        .arg("28")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-an")
        .arg("-movflags")
        .arg("+faststart")
        .arg("-progress")
        .arg("pipe:1")
        .arg(output_str)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                log_debug!("ffmpeg not found; cannot compress video for note.");
                return Ok(None);
            }
            return Err(anyhow!(err).context("Failed to spawn ffmpeg for video note compression"));
        }
    };

    let max_secs = total_duration.unwrap_or(60).min(60);
    let progress = ProgressBar::new(max_secs);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} Compressing video note [{bar:30.cyan/blue}] {pos}/{len}s ({eta})",
        )
        .unwrap()
        .progress_chars("#>-")
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈"),
    );
    progress.enable_steady_tick(Duration::from_millis(200));

    if let Some(ref mut stdout) = child.stdout {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Some(val) = line.strip_prefix("out_time_us=") {
                    if let Ok(us) = val.trim().parse::<u64>() {
                        let secs = us / 1_000_000;
                        progress.set_position(secs.min(max_secs));
                    }
                }
            }
        }
    }

    let status = child.wait();
    progress.finish_and_clear();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            log_error!("ffmpeg exited with status: {}", s);
            return Ok(None);
        }
        Err(err) => {
            log_error!("ffmpeg wait failed: {}", err);
            return Ok(None);
        }
    }

    if !output_path.exists() {
        log_error!("ffmpeg completed but output file not found: {}", output_str);
        return Ok(None);
    }

    let compressed_size = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let compressed_mb = compressed_size / (1024 * 1024);

    log_info!(
        "Video note compressed: {}MB → {}MB ({})",
        input_mb,
        compressed_mb,
        output_path.display()
    );

    Ok(Some(output_path))
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
