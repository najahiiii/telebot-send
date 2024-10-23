"""Logging configuration for the bot."""

import logging

logging.basicConfig(
    format="[%(asctime)s] - %(levelname)s - %(message)s",
    level=logging.INFO,
    datefmt="%Y-%m-%d %H:%M:%S",
)
logging.getLogger("httpx").setLevel(logging.WARNING)

logger = logging.getLogger(__name__)
