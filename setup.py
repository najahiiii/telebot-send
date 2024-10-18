from setuptools import setup, find_packages

setup(
    name="sendtg",
    version="1.0.0",
    author="Ahmad Thoriq Najahi",
    author_email="najahi@zephyrus.id",
    description="A Telegram bot to send messages and media to a specified chat ID.",
    long_description=open("README.md", "r", encoding="utf-8").read(),
    long_description_content_type="text/markdown",
    url="https://github.com/najahiiii/telebot-send",
    packages=find_packages(),
    install_requires=[
        "python-telegram-bot>=21.6",
    ],
    classifiers=[
        "Programming Language :: Python :: 3",
        "License :: OSI Approved :: WTFPL License",
        "Operating System :: OS Independent",
    ],
    python_requires=">=3.8",
    entry_points={
        "console_scripts": [
            "sendtg=sendtg.bot:run",
        ],
    },
)
