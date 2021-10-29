import asyncio
import configparser
import logging
from typing import Union

import aiohttp
import discord
from discord.ext import tasks

from .constants import DEFAULT_AVATAR_URL

config = configparser.ConfigParser()
config.read("config.ini")

unknown_avatar_url = DEFAULT_AVATAR_URL.format(5)
avatars = {
    logging.INFO: DEFAULT_AVATAR_URL.format(0),
    logging.DEBUG: DEFAULT_AVATAR_URL.format(1),
    logging.ERROR: DEFAULT_AVATAR_URL.format(4),
    logging.WARNING: DEFAULT_AVATAR_URL.format(3),
}

class CacheFixedLogger(logging.Logger):
    _cache: dict[int, bool]
    def setLevel(self, level: Union[int, str]) -> None:
        self.level = logging._checkLevel(level) # type: ignore
        self._cache.clear()

class WebhookHandler(logging.StreamHandler):
    def __init__(self, prefix: str, session: aiohttp.ClientSession, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.prefix = prefix

        self.loop = asyncio.get_running_loop()
        self.to_be_sent: dict[int, list[str]] = {}

        self.normal_logs = discord.Webhook.from_url(url=config["Webhook URLs"]["logs"], session=session)
        self.error_logs = discord.Webhook.from_url(url=config["Webhook URLs"]["errors"], session=session)


    def webhook_send(self, severity: int, *lines: str):
        severity_name = logging.getLevelName(severity)
        webhook = self.error_logs if severity >= logging.ERROR else self.normal_logs

        message = ""
        for line in lines:
            if severity >= logging.WARNING:
                line = f"**{line}**"

            message += f"{self.prefix}{line}\n"

        return asyncio.gather(*(
            webhook.send(
                username=f"TTS-Webhook [{severity_name}]",
                content="".join(l for l in message if l is not None),
                avatar_url=avatars.get(severity, unknown_avatar_url),
            )
            for message in discord.utils.as_chunks(message, 2000)
        ))

    @tasks.loop(seconds=1)
    async def sender_loop(self) -> None:
        for severity in self.to_be_sent.copy().keys():
            message = self.to_be_sent.pop(severity)
            await self.webhook_send(severity, *message)

    def _emit(self, record: logging.LogRecord) -> None:
        msg = self.format(record)
        if record.levelno not in self.to_be_sent:
            self.to_be_sent[record.levelno] = [msg]
        else:
            self.to_be_sent[record.levelno].append(msg)

        if not self.sender_loop.is_running():
            self.sender_loop.start()

    def emit(self, record: logging.LogRecord) -> None:
        self.loop.call_soon_threadsafe(self._emit, record)

    def close(self):
        self.sender_loop.cancel()


def setup(level: str, prefix: str, session: aiohttp.ClientSession) -> CacheFixedLogger:
    logger = CacheFixedLogger("TTS Bot")
    logger.setLevel(level.upper())
    logger.addHandler(WebhookHandler(prefix, session))
    return logger
