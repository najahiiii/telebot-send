# sendtg · Telegram Bot CLI

`sendtg` is a Rust CLI for sending messages and media to Telegram chats through the Bot API.  
It supports both the official API endpoint and self-hosted [`telegram-bot-api`](https://github.com/tdlib/telegram-bot-api) servers, and focuses on convenient automation features such as persistent config files, automatic metadata extraction, and live upload progress bars.

## Features

- Send text messages or upload photos, videos, audio, and documents (Up To 2000MB if using self-hosted [`telegram-bot-api`](https://github.com/tdlib/telegram-bot-api) servers).
- Automatically group media (up to 10 items) or send individually with `--no-group`.
- Auto-convert large photos (>10 MB) to documents while keeping thumbnails.
- Generate video/photo thumbnails and metadata via `ffprobe`/`ffmpeg` (if available).
- Show upload progress bars for every multipart transfer.
- Handle spoilers, inline buttons, silent messages, and latency checks.
- Interactive `--setup` wizard that stores credentials in `$HOME/.config/sendtg/config.toml`.
- `--show-config` to inspect stored values without editing files.
- Compatible with the official API or custom Bot API server URLs.

## Prerequisites

- Rust 1.75+ (Edition 2024).
- A Telegram bot token and target chat ID.
- Optional: `ffprobe` and `ffmpeg` on `PATH` for richer metadata/thumbnails.

## Build & Run

1. Clone the repository:

   ```bash
   git clone https://github.com/najahiiii/telebot-send.git
   cd telebot-send
   ```

2. Build the CLI:

   ```bash
    cargo build --release
   ```

3. Run the binary with your desired options:

   ```bash
    ./target/release/sendtg "Hello, world!"
   ```

> Tip · Use `./target/release/sendtg --setup` once to store your bot token/chat ID and avoid repeating flags.

### Interactive setup

```bash
./target/release/sendtg --setup
```

The wizard prompts for API URL, bot token, and chat ID (with current values pre-filled if they exist).  
Credentials are persisted at `$HOME/.config/sendtg/config.toml`, and every run reads that file unless a flag overrides it.

Use `./target/release/sendtg --show-config` to print the stored values.

## Command-line reference

| Flag                        | Description                                                             |
| --------------------------- | ----------------------------------------------------------------------- |
| `--setup`                   | Store credentials in the config file and exit.                          |
| `--show-config`             | Print current configuration values and exit.                            |
| `-a`, `--api_url <URL>`     | Override the Bot API base URL (default `https://api.telegram.org/bot`). |
| `-t`, `--bot_token <TOKEN>` | Override the bot token.                                                 |
| `-c`, `--chat_id <ID>`      | Override the target chat ID/channel username.                           |
| `--thread-id <ID>`          | Target a specific forum topic (message thread ID) inside a group.       |
| `-m`, `--media <PATH>...`   | Attach one or more media files.                                         |
| `--spoiler`                 | Mark supported media with Telegram’s spoiler animation.                 |
| `--no-group`                | Send each media item individually (disables media albums).              |
| `-F`, `--as-file`           | Force media to be sent as documents.                                    |
| `-C`, `--caption <TEXT>`    | Caption applied to the first media item.                                |
| `--button "LABEL\|URL"`     | Add an inline button; repeat for multiple buttons.                      |
| `--button-row-break`        | Start a new inline keyboard row (use between `--button` flags).         |
| `--silent`                  | Send the message without notifications.                                 |
| `--check`                   | Measure Bot API latency by sending a random chat action.                |
| `message`                   | Positional message when no media is provided.                           |

### Notes

- The tool converts photos larger than 10 MB to documents automatically (Telegram limit), while still generating thumbnails for previews.
- Video and image thumbnails are produced with `ffmpeg`/`ffprobe` when available; uploads still succeed without them.
- Every multipart upload displays a progress bar. After the bar completes, the CLI informs you that it is waiting for Telegram (useful when a self-hosted API server forwards the request asynchronously).
- Albums are chunked to 10 media items, matching Telegram’s API limit.

## Usage Examples

Send a message using stored credentials:

```bash
./target/release/sendtg "Hello, world!"
```

Send multiple photos as a media group with a caption:

```bash
./target/release/sendtg -m photo1.jpg photo2.jpg --caption "Weekend recap"
```

Send a video silently with inline buttons (overriding stored config):

```bash
./target/release/sendtg \
  -a https://api.yourproxy.example/bot \
  -t 12345:ABC... \
  -c @yourchannel \
  -m clip.mp4 \
  --as-file \
  --silent \
  --button "Watch more|https://example.com" \
  --button "Status|https://status.example.com" \
  --button-row-break \
  --button "Support|https://example.com/help"
```

Send a message to a specific forum topic inside a group:

```bash
./target/release/sendtg -c -1001234567890 --thread-id 42 "Halo topik!"
```

Check API latency:

```bash
./target/release/sendtg --check
```

## License

This project is licensed under the [MIT License](LICENSE).
