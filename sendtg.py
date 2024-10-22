#!/usr/bin/env python3
"""This script is a Telegram bot that sends messages and media to a specified chat ID.
"""
import argparse
import asyncio
import os
import sys

from telegram import (
    Bot,
    InlineKeyboardButton,
    InlineKeyboardMarkup,
    InputMediaPhoto,
    InputMediaVideo,
)
from telegram.constants import ParseMode
from telegram.error import TelegramError

from config import DEFAULT_CHAT_ID, DEFAULT_BOT_TOKEN, URL, VERSION
from utils.logger import logger as log


class SendTg:
    """
    SendTg is a class that provides methods to send messages and media to
    a specified Telegram chat using a bot.

    Methods:
        __init__(self, bot_token=None, chat_id=None):
            Initializes the SendTg instance with a bot token and a default chat ID.

        send_chat_action(self, chat_id, media_path, as_file=False):
            Sends a chat action based on the type of media being uploaded.

        send_message(self, chat_id, message, reply_markup=None):

        send_media(self, chat_id, media_paths, caption=None, as_file=False):
            Sends media (photos or videos) to a specified chat ID using a bot
            and indicates the upload action.

        run(self, run_args):
            Main function to parse command-line arguments and send messages or media
            to a specified Telegram chat.
    """

    def __init__(self, bot_token=None, chat_id=None):
        """
        Initializes the Telegram bot with the provided bot token and chat ID.

        Args:
            bot_token (str, optional): The token for the Telegram bot
                Defaults to DEFAULT_BOT_TOKEN.
            chat_id (str, optional): The chat ID to send messages to.
                Defaults to DEFAULT_CHAT_ID.
        """
        self.bot_token = bot_token or DEFAULT_BOT_TOKEN
        self.chat_id = chat_id or DEFAULT_CHAT_ID

        if not self.bot_token:
            log.error("Bot token is required!")
            raise ValueError("Bot token is missing!")

        try:
            self.bot = Bot(token=self.bot_token)
        except TelegramError as e:
            log.error("Failed to initialize bot: %s", e)
            sys.exit("Error initializing bot with the provided token.")

        if not self.chat_id:
            log.error("Chat ID is required!")
            raise ValueError("Chat ID is missing!")

    async def send_chat_action(self, chat_id, media_path, as_file=False):
        """
        Sends a chat action to indicate the type of media being uploaded.

        Parameters:
            chat_id (int): Unique identifier for the target chat.
            media_path (str): Path to the media file being uploaded.
            as_file (bool, optional): If True, the media is sent as a file. Defaults to False.

        Returns:
            None

        Sends:
            - "upload_document" if `as_file` is True or the media type is not recognized.
            - "upload_photo" if the media is an image (png, jpg, jpeg, gif).
            - "upload_video" if the media is a video (mp4, mov, avi).
        """
        try:
            if as_file:
                await self.bot.send_chat_action(
                    chat_id=chat_id, action="upload_document"
                )
            elif media_path.lower().endswith((".png", ".jpg", ".jpeg", ".gif")):
                await self.bot.send_chat_action(chat_id=chat_id, action="upload_photo")
            elif media_path.lower().endswith((".mp4", ".mov", ".avi")):
                await self.bot.send_chat_action(chat_id=chat_id, action="upload_video")
            else:
                await self.bot.send_chat_action(
                    chat_id=chat_id, action="upload_document"
                )
        except TelegramError as e:
            log.error("Error sending chat action: %s", e)

    async def send_message(self, chat_id, message, reply_markup=None):
        """
        Sends a message to a specified chat ID with optional reply markup.

        Args:
            chat_id (int): The ID of the chat to send the message to.
            message (str): The message text to send.
            reply_markup (Optional[ReplyMarkup], optional):
                Additional interface options for the message. Defaults to None.

        Returns:
            None

        Logs:
            Logs the chat ID and message content after sending the message.
        """
        try:
            await self.bot.send_chat_action(chat_id=chat_id, action="typing")
            await self.bot.send_message(
                chat_id=chat_id,
                text=message,
                parse_mode=ParseMode.HTML,
                reply_markup=reply_markup,
            )
            log.info("Message sent to chat ID %s: %s", chat_id, message)
        except TelegramError as e:
            log.error("Failed to send message: %s", e)
            sys.exit("Error sending message.")

    async def send_media(self, chat_id, media_paths, caption=None, as_file=False):
        """
        Sends media files to a specified chat.

        Args:
            chat_id (int): The ID of the chat to send the media to.
            media_paths (list): A list of file paths to the media files to be sent.
            caption (str, optional): A caption to include with the media. Defaults to None.
            as_file (bool, optional): If True, sends the media as files/documents.
                Defaults to False.

        Raises:
            FileNotFoundError: If any of the media files do not exist.

        Notes:
            - Supports sending images (PNG, JPG, JPEG, GIF) and videos (MP4, MOV, AVI).
            - If `as_file` is True, all media files are sent as documents.
            - If multiple media files are provided, they are sent as an album.
            - Logs information about the sending process.
        """
        media_group = []

        for media_path in media_paths:
            if os.path.isfile(media_path):
                await self.send_chat_action(chat_id, media_path, as_file=as_file)

                try:
                    if as_file:
                        with open(media_path, "rb") as file:
                            await self.bot.send_document(
                                chat_id=chat_id,
                                document=file,
                                caption=caption,
                                filename=os.path.basename(media_path),
                                disable_content_type_detection=True,
                            )
                        log.info(
                            "Album sent as file to chat ID %s: %s", chat_id, media_path
                        )
                    elif media_path.lower().endswith((".png", ".jpg", ".jpeg", ".gif")):
                        with open(media_path, "rb") as file:
                            media_item = InputMediaPhoto(media=file, caption=caption)
                            media_group.append(
                                (media_item, os.path.basename(media_path))
                            )
                    elif media_path.lower().endswith((".mp4", ".mov", ".avi")):
                        with open(media_path, "rb") as file:
                            media_item = InputMediaVideo(media=file, caption=caption)
                            media_group.append(
                                (media_item, os.path.basename(media_path))
                            )
                    else:
                        with open(media_path, "rb") as file:
                            await self.bot.send_document(
                                chat_id=chat_id,
                                document=file,
                                caption=caption,
                                filename=os.path.basename(media_path),
                                disable_content_type_detection=True,
                            )
                        log.info("File sent to chat ID %s: %s", chat_id, media_path)
                except TelegramError as e:
                    log.error("Failed to send media: %s", e)
            else:
                log.info("Media not found: %s", media_path)

        if len(media_group) > 1:
            try:
                media_list = [
                    f"{media[1]} "
                    f"(Caption: {media[0].caption if media[0].caption else 'No caption'})"
                    for media in media_group
                ]
                log.info("Album prepared: %s", media_list)
                await self.bot.send_media_group(
                    chat_id=chat_id, media=[m[0] for m in media_group]
                )
                log.info(
                    "Album sent to chat ID %s: %d media items",
                    chat_id,
                    len(media_group),
                )
            except TelegramError as e:
                log.error("Failed to send media group: %s", e)
        elif len(media_group) == 1:
            try:
                media_item = media_group[0][0]
                if isinstance(media_item, InputMediaPhoto):
                    await self.bot.send_photo(
                        chat_id=chat_id,
                        photo=media_item.media,  # type: ignore
                        caption=media_item.caption,
                    )
                    log.info("Photo sent to chat ID %s", chat_id)
                elif isinstance(media_item, InputMediaVideo):
                    await self.bot.send_video(
                        chat_id=chat_id,
                        video=media_item.media,  # type: ignore
                        caption=media_item.caption,
                    )
                    log.info("Video sent to chat ID %s", chat_id)
            except TelegramError as e:
                log.error("Failed to send media item: %s", e)

    async def run(self, run_args):
        """
        Args:
            run_args (Namespace): Command-line arguments containing the following attributes:
                - media (str): Path to the media file to be sent.
                - caption (str): Caption for the media file.
                - as_file (bool): Flag indicating whether to send the media as a file.
                - button_text (str): Text for the inline button.
                - button_url (str): URL for the inline button.
                - message (str): Text message to be sent.

        Returns:
            None
        """
        chat_id = self.chat_id

        if run_args.media:
            await self.send_media(
                chat_id,
                run_args.media,
                caption=run_args.caption,
                as_file=run_args.as_file,
            )
        else:
            reply_markup = None
            if run_args.button_text and run_args.button_url:
                keyboard = [
                    [
                        InlineKeyboardButton(
                            run_args.button_text, url=run_args.button_url
                        )
                    ]
                ]
                reply_markup = InlineKeyboardMarkup(keyboard)
            if not run_args.message:
                return
            await self.send_message(chat_id, run_args.message, reply_markup)


def cli():
    """
    Parse command-line arguments for sending messages or media to a specified chat ID.

    Arguments:
    -t, --bot_token (str): Token for the Telegram bot.
    -c, --chat_id: Chat ID to send the message or media to.
    -m, --media (list of str, optional): Path of one or more media files to send.
    -F, --as_file (bool): Send the media as a file (Uncompressed).
    -C, --caption (str, optional): Caption for the media being sent.
    --button_text (str, optional): Text displayed on the inline button.
    --button_url (str, optional): URL that the button links to.
    message (str, optional): Message to send (only used if -m is not specified).

    Returns:
    argparse.Namespace: Parsed command-line arguments.
    """
    parser = argparse.ArgumentParser(
        description="Send messages or media to a specified chat ID."
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
        "-F",
        "--as_file",
        action="store_true",
        help="Send the media as a file (Uncompressed).",
    )
    parser.add_argument(
        "-C", "--caption", type=str, help="Caption for the media being sent."
    )
    parser.add_argument(
        "--button_text", type=str, help="Text displayed on the inline button."
    )
    parser.add_argument("--button_url", type=str, help="URL that the button links to.")
    parser.add_argument(
        "message",
        nargs="?",
        type=str,
        help="Message to send (only used if -m is not specified).",
    )
    parser.add_argument(
        "-v",
        "--version",
        action="version",
        version=f"%(prog)s v{VERSION}",
        help="Show program's version number and exit.",
    )
    parser.epilog = f"Visit {URL} for more information."

    return parser.parse_args()


if __name__ == "__main__":
    args = cli()
    try:
        main = SendTg(bot_token=args.bot_token, chat_id=args.chat_id)
        if not args.message and not args.media:
            sys.exit("No message or media provided.")
        if not args.bot_token and not args.chat_id:
            log.info(
                "Using default bot token and chat ID. %s, %s",
                main.bot_token.replace(main.bot_token[6:], "*" * 30),
                main.chat_id,
            )
        asyncio.run(main.run(args))
    except ValueError as e:
        log.error(e)
        sys.exit(f"Input validation error: {e}")
