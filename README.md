
# Telegram Bot Sender - sendtg.py

`sendtg.py` designed to send messages and media to a specified Telegram chat using a bot. The script can handle plain messages, media files (such as photos and videos), as well as inline buttons with URLs.

## Features

- Send text messages to any Telegram chat.
- Upload media files (images and videos) directly to chats.
- Send media files either as compressed or uncompressed.
- Include captions for media being sent.
- Create inline buttons with custom text and URLs.

## Prerequisites

- Python 3.8+
- `config.py` file containing your bot token

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

3. Create a `config.py` file using the `example.config.py` template:

   ```python
    # pylint: disable=line-too-long
    """Bot configuration"""

    DEFAULT_CHAT_ID = "YOUR CHAT ID"
    DEFAULT_BOT_TOKEN = "YOUR BOT TOKEN"
   ```

## Command-Line Options

- `-h` : Show help.
- `-t`, `--bot_token`: Token for the Telegram bot.
- `-c`, `--chat_id`: Chat ID to send the message or media to.
- `-m`, `--media`: Path to one or more media files to be sent.
- `-C`, `--caption`: Caption for the media being sent (optional).
- `-F`, `--as_file`: Sends media as uncompressed (optional).
- `--button_text`: Text displayed on the inline button (optional).
- `--button_url`: URL that the inline button links to (optional).
- `message`: The message text to send (parse mode: HTML).

## Examples

1. **Send a simple message:**

   ```
   python sendtg.py -t YOUR_BOT_TOKEN -c YOUR_CHAT_ID "Hello, world!"
   ```

2. **Send a photo with a caption:**

   ```
   python sendtg.py -t YOUR_BOT_TOKEN -c YOUR_CHAT_ID -m /path/to/photo.jpg -C "Check out this photo!"
   ```

3. **Send multiple media files as a media group:**

   ```
   python sendtg.py -t YOUR_BOT_TOKEN -c YOUR_CHAT_ID -m /path/to/photo1.jpg /path/to/photo2.jpg ...
   ```

4. **Send a message with an inline button:**

   ```
   python sendtg.py -t YOUR_BOT_TOKEN -c YOUR_CHAT_ID "Click the button below:" --button_text "Google" --button_url "https://www.google.com"
   ```

## License

This project is licensed under the WTFPL License. See the [LICENSE](LICENSE) file for details.
