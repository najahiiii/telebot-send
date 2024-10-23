
# Telegram Bot Sender - sendtg.py

`sendtg.py` designed to send messages and media to a specified Telegram chat using a bot. The script can handle plain messages, media files (such as photos and videos), as well as inline buttons with URLs.

## Features

- Send text messages to any Telegram chat.
- Upload media files (images and videos) directly to chats.
- Send media files either as compressed or uncompressed.
- Send media files as group or individually.
- Send media files with spoiler.
- Send chat action with asynchronously in a separate thread.
- Include captions for media being sent.
- Create inline buttons with custom text and URLs.

## Prerequisites

- Python 3.8+
- `config.py` file containing your bot token and chat id

## Installation

1. Clone this repository:

    ```bash
    git clone https://github.com/najahiiii/telebot-send.git
    ```

2. Navigate to the project directory:

    ```bash
    cd telebot-send
    ```

3. Create a virtual environment (optional, but recommended):

    ```bash
    python3 -m venv telebot
    source telebot/bin/activate
    ```

4. Install dependencies:

    ```bash
    pip install -r requirements.txt
    ```

5. Create a `config.py` file using the `example.config.py` template:

   ```python
   # pylint: disable=line-too-long
   """Bot configuration"""

   DEFAULT_API_URL = "https://api.telegram.org/bot"
   DEFAULT_BOT_TOKEN = "YOUR_BOT_TOKEN"
   DEFAULT_CHAT_ID = "YOUT_CHAT_ID"
   URL = "https://github.com/najahiiii/telebot-send"
   VERSION = "1.0.0"
   ```

## Command-Line Options

- `-a`, `--api_url`: API URL for the Telegram bot. (Default: <https://api.telegram.org/bot>)
- `-t`, `--bot_token`: Token for the Telegram bot.
- `-c`, `--chat_id`: Chat ID to send the message or media to.
- `-m`, `--media`: Path of one or more media files to send.
- `--spoiler`: Send media with spoiler.
- `--no-group`: Send media as individual files. (Default: False)
- `-F`, `--as_file`: Send the media as a file (Uncompressed).
- `-C`, `--caption`: Caption for the media being sent.
- `--button-text`: Text displayed on the inline button.
- `--button-url`: URL that the button links to.
- `message`: Message to send (only used if -m is not specified).
- `-v`, `--version`: Show program's version number and exit.

## Examples

1. **Send a simple message:**

   ```bash
   python sendtg.py -t YOUR_BOT_TOKEN -c YOUR_CHAT_ID "Hello, world!"
   ```

2. **Send a photo with a caption:**

   ```bash
   python sendtg.py -t YOUR_BOT_TOKEN -c YOUR_CHAT_ID -m /path/to/photo.jpg --caption "Check out this photo!"
   ```

3. **Send multiple media files as a media group:**

   ```bash
   python sendtg.py -t YOUR_BOT_TOKEN -c YOUR_CHAT_ID -m /path/to/photo1.jpg /path/to/photo2.jpg ...
   ```

4. **Send a message with an inline button:**

   ```bash
   python sendtg.py -t YOUR_BOT_TOKEN -c YOUR_CHAT_ID "Click the button below:" --button-text "PornHub" --button-url "https://pornhub.com"
   ```

## License

This project is licensed under the WTFPL License. See the [LICENSE](LICENSE) file for details.
