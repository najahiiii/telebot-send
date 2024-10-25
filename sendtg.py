#!/usr/bin/env python3
# pylint: disable=too-many-arguments, too-many-locals, too-many-branches, too-many-statements
"""A Telegram bot CLI that sends messages and media to a specified chat ID using requests."""

import argparse
import json
import mimetypes
import os
import random
import sys
import threading
import time

import requests

from config import DEFAULT_API_URL, DEFAULT_BOT_TOKEN, DEFAULT_CHAT_ID, URL, VERSION
from utils.logger import logger as log


class SendTg:
    """
    A class to interact with the Telegram Bot API for sending messages, media, and chat actions.

    Attributes:
        api_url (str): The base URL for the Telegram Bot API.
        bot_token (str): The token for authenticating the bot.
        chat_id (str or int): The unique identifier for the target chat.


    Methods:
        __init__(api_url=None, bot_token=None, chat_id=None):
            Initializes the SendTg instance with the provided API URL, bot token, and chat ID.

        log_except(msgs, e):
            Logs an exception with a redacted error message and additional HTTP
                response details if available.

        send_chat_action(chat_id, action="typing"):
            Sends a chat action (e.g., typing, upload_photo) to the specified chat ID.

        send_message(chat_id, message, reply_markup=None):
            Sends a text message to the specified chat ID with optional reply markup.

        send_media(chat_id, media_paths, caption="", as_file=False, no_group=False,
                    button_text="", button_url="", spoiler=False):
            Sends media files (photos, videos, documents) to the specified chat ID.
                Supports sending media as files, grouping media, and adding buttons.

        _media_type(mime_type):
            Determines the media type based on the MIME type.

        _send_document(chat_id, media_path, caption, reply_markup=None):
            Sends a document to a specified chat.

        _send_media_group(chat_id, media_group, files_data):
            Sends a group of media files to the specified chat ID.

        _send_single_media(chat_id, media, files_data, reply_markup=None, spoiler=False):
            Sends a single media file to the specified chat ID
                with optional reply markup and spoiler flag.

        _create_reply_markup(button_text, button_url):
            Creates a reply markup with an inline keyboard button.

        run(run_args):
            Executes the sending of messages or media based on the provided arguments.
    """

    def __init__(self, api_url=None, bot_token=None, chat_id=None):
        self.api_url = api_url or DEFAULT_API_URL
        self.bot_token = bot_token or DEFAULT_BOT_TOKEN
        self.chat_id = chat_id or DEFAULT_CHAT_ID

        if not self.bot_token:
            log.error("Bot token is required!")
            raise ValueError("Bot token is missing!")

        if not self.chat_id:
            log.error("Chat ID is required!")
            raise ValueError("Chat ID is missing!")

    def log_except(self, msgs, e):
        """
        Logs an exception with a redacted error message and additional HTTP response details
            if available.

        Args:
            msgs (str): The message to log alongside the exception.
            e (Exception): The exception to log. If the exception has a 'response' attribute,
                            additional HTTP status code and response text will be logged.

        Notes:
            The bot token within the exception message will be redacted for security purposes.
        """
        err_msg = str(e)
        redacted_msg = err_msg.replace(self.bot_token, "REDACTED")
        log.error("%s %s", msgs, redacted_msg)

        if hasattr(e, "response") and e.response is not None:
            log.debug(
                "HTTP Status Code: %s, Response: %s",
                e.response.status_code,
                e.response.text,
            )

    def send_chat_action(self, chat_id, action="typing"):
        """
        Sends a chat action to the specified chat.

        This method sends a chat action (e.g., 'typing', 'upload_photo') to the specified chat ID.
        The action is sent asynchronously in a separate thread.

        Args:
            chat_id (int or str): Unique identifier for the target Chat ID.
            action (str, optional): Type of action to broadcast. Defaults to "typing".

        Raises:
            requests.exceptions.RequestException: If there is an issue with the request.
        """

        def send_action():
            try:
                url = f"{self.api_url}{self.bot_token}/sendChatAction"
                data = {"chat_id": chat_id, "action": action}

                response = requests.post(url, data=data, timeout=None)
                response.raise_for_status()
            except requests.exceptions.RequestException as e:
                self.log_except("Failed to send chat action:", e)

        thread = threading.Thread(target=send_action)
        thread.start()

    def send_message(self, chat_id, message, reply_markup=None):
        """
        Sends a message to a specified chat.

        Args:
            chat_id (int or str): Unique identifier for the target Chat ID.
            message (str): Text of the message to be sent.
            reply_markup (dict, optional): Additional interface options such as inline keyboards,
                custom reply keyboards, etc. Defaults to None.

        Raises:
            requests.exceptions.RequestException: If there is an issue with the HTTP request.

        Logs:
            Info: When the message is successfully sent.
            Exception: If there is a failure in sending the message.
        """
        url = f"{self.api_url}{self.bot_token}/sendMessage"
        payload = {
            "chat_id": chat_id,
            "text": message.replace("\\n", "\n"),
            "parse_mode": "HTML",
        }

        if reply_markup:
            payload["reply_markup"] = reply_markup

        try:
            self.send_chat_action(chat_id, action="typing")
            response = requests.post(url, json=payload, timeout=None)
            response.raise_for_status()
            log.info("Message sent to chat ID %s: %s", chat_id, message)
        except requests.exceptions.RequestException as e:
            self.log_except("Failed to send message:", e)

    def send_media(
        self,
        chat_id,
        media_paths,
        caption="",
        as_file=False,
        no_group=False,
        button_text="",
        button_url="",
        spoiler=False,
    ):
        """
        Sends media files to a specified chat.

        Parameters:
        - chat_id (int or str): Unique identifier for the target Chat ID.
        - media_paths (list of str): List of file paths to the media files to be sent.
        - caption (str, optional): Caption for the media. Defaults to an empty string.
        - as_file (bool, optional): If True, sends the media as a document. Defaults to False.
        - no_group (bool, optional): If True, sends each media file individually. Defaults to False.
        - button_text (str, optional): Text for the inline button. Defaults to an empty string.
        - button_url (str, optional): URL for the inline button. Defaults to an empty string.
        - spoiler (bool, optional): If True, marks the media as a spoiler. Defaults to False.

        Returns:
        None

        Raises:
        - requests.exceptions.RequestException: If there is an error while sending the media.

        Note:
        - Media files are sent in chunks of 10 if sending as a media group.
        - If a file is not found, it is skipped and an error is logged.
        - Media files are closed after sending.
        """
        media_group = []
        files_data = {}

        reply_markup = self._create_reply_markup(button_text, button_url)

        for media_path in media_paths:
            if not os.path.isfile(media_path):
                log.error("File not found: %s", media_path)
                continue

            mime_type, _ = mimetypes.guess_type(media_path)
            media_type = "document" if as_file else self._media_type(mime_type)

            if media_type != "document":
                media_file = open(media_path, "rb")
                file_name = os.path.basename(media_path)
                files_data[file_name] = media_file
                media_group.append(
                    {
                        "type": media_type,
                        "media": f"attach://{file_name}",
                        "caption": caption if len(media_group) == 0 else "",
                        "has_spoiler": spoiler,
                    }
                )
            else:
                self.send_chat_action(chat_id, action="upload_document")
                self._send_document(chat_id, media_path, caption, reply_markup)
                return

        try:
            if no_group:
                for media_item in media_group:
                    self.send_chat_action(
                        chat_id, action=f"upload_{media_item['type'].lower()}"
                    )
                    file_name = media_item["media"].replace("attach://", "")
                    if file_name in files_data:
                        self._send_single_media(
                            chat_id,
                            media_item,
                            {file_name: files_data[file_name]},
                            caption,
                            reply_markup,
                            spoiler,
                        )
            else:
                log.info("Sending media in groups of 10.")
                for i in range(0, len(media_group), 10):
                    media_chunk = media_group[i : i + 10]
                    log.info(
                        "Sending media group #%d with %d media files",
                        (i // 10) + 1,
                        len(media_chunk),
                    )

                    current_files_data = {
                        os.path.basename(
                            media["media"].replace("attach://", "")
                        ): files_data[
                            os.path.basename(media["media"].replace("attach://", ""))
                        ]
                        for media in media_chunk
                        if os.path.basename(media["media"].replace("attach://", ""))
                        in files_data
                    }

                    self.send_chat_action(
                        chat_id, action="upload_" + media_type.lower()
                    )
                    self._send_media_group(chat_id, media_chunk, current_files_data)

        except requests.exceptions.RequestException as e:
            self.log_except("Failed to send media:", e)
        finally:
            for media_file in files_data.values():
                media_file.close()

    def _media_type(self, mime_type):
        if mime_type and mime_type.startswith("image/"):
            return "photo"
        if mime_type and mime_type.startswith("video/"):
            return "video"
        if mime_type and mime_type.startswith("audio/"):
            return "audio"
        return "document"

    def _send_document(self, chat_id, media_path, caption, reply_markup=None):
        try:
            with open(media_path, "rb") as media_file:
                files = {"document": media_file}
                data = {
                    "chat_id": chat_id,
                    "caption": caption if isinstance(caption, str) else None,
                    "reply_markup": reply_markup,
                }
                url = f"{self.api_url}{self.bot_token}/sendDocument"
                response = requests.post(url, files=files, data=data, timeout=None)
                response.raise_for_status()
                log.info("Document sent to chat ID %s: %s", chat_id, media_path)
        except requests.exceptions.RequestException as e:
            self.log_except("Failed to send document:", e)

    def _send_media_group(self, chat_id, media_group, files_data):
        try:
            url = f"{self.api_url}{self.bot_token}/sendMediaGroup"
            for media in media_group:
                if media.get("caption") is None:
                    del media["caption"]
            data = {
                "chat_id": chat_id,
                "media": json.dumps(media_group),
            }
            response = requests.post(url, files=files_data, data=data, timeout=None)
            response.raise_for_status()
            log.info(
                "Media group sent to chat ID %s with %d items",
                chat_id,
                len(media_group),
            )

        except requests.exceptions.RequestException as e:
            self.log_except("Failed to send media group:", e)

    def _send_single_media(
        self, chat_id, media, files_data, caption="", reply_markup=None, spoiler=False
    ):
        file_name = media["media"].replace("attach://", "")
        files = {media["type"]: files_data[file_name]}

        try:
            url = f"{self.api_url}{self.bot_token}/send{media['type'].capitalize()}"
            data = {
                "chat_id": chat_id,
                "caption": caption,
                "reply_markup": reply_markup,
                "has_spoiler": spoiler,
            }
            response = requests.post(url, files=files, data=data, timeout=None)
            response.raise_for_status()
            log.info("Single media file sent to chat ID %s: %s", chat_id, file_name)
        except requests.exceptions.RequestException as e:
            self.log_except("Failed to send media file:", e)

    def _create_reply_markup(self, button_text, button_url):
        if button_text and button_url:
            reply_markup = {
                "inline_keyboard": [[{"text": button_text, "url": button_url}]]
            }
            return json.dumps(reply_markup)
        if button_text or button_url:
            log.error("Both button_text and button_url must be provided.")
        return None

    def check(self, chat_id):
        """
        Sends a random chat action to the specified chat ID and logs the response time.

        Parameters:
        chat_id (int): The ID of the chat to which the action will be sent.

        Raises:
        requests.exceptions.RequestException: If there is an issue with the HTTP request.

        Logs:
        Logs the API response time in milliseconds.
        """
        actions = [
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
        ]
        url = f"{self.api_url}{self.bot_token}/sendChatAction"
        data = {"chat_id": chat_id, "action": random.choice(actions)}

        try:
            start = int(time.time())
            response = requests.post(url, json=data, timeout=None)
            response.raise_for_status()
            end = int(time.time())

            time_ms = (end - start) * 1000

            log.info("%s API Response time: %s ms", self.api_url, time_ms)
        except requests.exceptions.RequestException as e:
            self.log_except("Failed to send chat action:", e)

    def run(self, run_args):
        """
        Executes the main logic for sending messages or media via the Telegram bot.

        Parameters:
        run_args (Namespace): A namespace object containing the following attributes:
            - message (str): The message to be sent.
            - media (str): The media file to be sent.
            - bot_token (str): The bot token for authentication.
            - chat_id (str): The chat ID to which the message or media will be sent.
            - api_url (str): The API URL for the Telegram bot.
            - check (bool): Flag to check the API response time for the bot.
            - caption (str): Caption for the media file.
            - as_file (bool): Flag to send media as a file.
            - no_group (bool): Flag to avoid grouping media.
            - button_text (str): Text for the inline button.
            - button_url (str): URL for the inline button.
            - spoiler (bool): Flag to mark media as a spoiler.

        Returns:
        None

        Raises:
        ValueError: If neither message nor media is provided.
        SystemExit: If no message or media is provided and the check flag is not set.
        """
        chat_id = self.chat_id

        if not run_args.message and not run_args.media:
            if getattr(run_args, "check", False):
                self.check(chat_id)
                return
            sys.exit("No message or media provided, use -h/--help for help.")

        if not run_args.bot_token and not run_args.chat_id:
            if not run_args.api_url:
                log.info("Using default bot api URL. %s", self.api_url)
            log.info(
                "Using default bot token and chat ID. %s, %s",
                self.bot_token.replace(self.bot_token[10:], "*" * 30),
                self.chat_id,
            )

        if run_args.media:
            self.send_media(
                chat_id,
                run_args.media,
                caption=run_args.caption,
                as_file=run_args.as_file,
                no_group=run_args.no_group,
                button_text=run_args.button_text,
                button_url=run_args.button_url,
                spoiler=run_args.spoiler,
            )
        else:
            reply_markup = (
                {
                    "inline_keyboard": [
                        [{"text": run_args.button_text, "url": run_args.button_url}]
                    ]
                }
                if run_args.button_text and run_args.button_url
                else None
            )

            if run_args.message:
                self.send_message(chat_id, run_args.message, reply_markup)
            else:
                raise ValueError("No message or media provided.")


def cli():
    """
    Parses command-line arguments for sending messages or media to a specified chat ID.

    Arguments:
        -a, --api_url (str): API URL for the Telegram bot. (Default: {DEFAULT_API_URL})
        -t, --bot_token (str): Token for the Telegram bot.
        -c, --chat_id: Chat ID to send the message or media to.
        -m, --media: Path of one or more media files to send.
        --spoiler: Send media with spoiler, applies to photos and videos only.
        --no-group: Send media as individual files.
        -F, --as-file: Send the media as a file (Uncompressed).
        -C, --caption (str): Caption for the media being sent.
        --button-text (str): Text displayed on the inline button.
        --button-url (str): URL that the button links to.
        message (str): Message to send (only used if -m is not specified).
        --check: Check the API response time for the bot.
        -v, --version: Show program's version number and exit.

    Help:
    Visit {URL} for more information.

    Note:
    If no bot token or chat ID is provided, the default values will be used.

    Returns:
    argparse.Namespace: Parsed command-line arguments.
        If no bot token or chat ID is provided, the default values will be used.

    Returns:
    argparse.Namespace: Parsed command-line arguments.
    """
    parser = argparse.ArgumentParser(
        description="Send messages or media to a specified chat ID."
    )
    parser.add_argument(
        "-a",
        "--api_url",
        type=str,
        help=f"API URL for the Telegram bot. (Default: {DEFAULT_API_URL})",
    )
    parser.add_argument(
        "-t", "--bot_token", type=str, help="Token for the Telegram bot."
    )
    parser.add_argument(
        "-c", "--chat_id", help="Chat ID to send the message or media to."
    )
    parser.add_argument(
        "-m", "--media", nargs="+", help="Path of one or more media files to send."
    )
    parser.add_argument(
        "--spoiler",
        action="store_true",
        help="Send media with spoiler, applies to photos and videos only.",
    )
    parser.add_argument(
        "--no-group",
        action="store_true",
        default=False,
        help="Send media as individual files.",
    )
    parser.add_argument(
        "-F",
        "--as-file",
        action="store_true",
        help="Send the media as a file (Uncompressed).",
    )
    parser.add_argument(
        "-C", "--caption", type=str, help="Caption for the media being sent."
    )
    parser.add_argument(
        "--button-text", type=str, help="Text displayed on the inline button."
    )
    parser.add_argument("--button-url", type=str, help="URL that the button links to.")
    parser.add_argument(
        "message",
        nargs="?",
        type=str,
        help="Message to send (only used if -m is not specified).",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Check the API response time for the bot.",
    )
    parser.add_argument(
        "-v",
        "--version",
        action="version",
        version=f"%(prog)s v{VERSION}",
        help="Show program's version number and exit.",
    )
    parser.add_argument_group(
        "Help",
        description=f"Visit {URL} for more information.",
    )
    parser.epilog = (
        "Note: If no bot token or chat ID is provided, the default values will be used."
    )

    return parser.parse_args()


if __name__ == "__main__":
    args = cli()
    try:
        main = SendTg(
            api_url=args.api_url, bot_token=args.bot_token, chat_id=args.chat_id
        )
        main.run(args)
    except ValueError as e:
        log.error(e)
