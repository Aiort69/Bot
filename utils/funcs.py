"Useful functions used throughout the bot"

from __future__ import annotations

import asyncio
import os
import sys
from inspect import cleandoc
from typing import TYPE_CHECKING, Awaitable, Optional, Sequence, TypeVar

from utils.constants import (ANIMATED_EMOJI_REGEX, EMOJI_REGEX,
                             OPTION_SEPERATORS, READABLE_TYPE)


if TYPE_CHECKING:
    import aioredis
    import discord

    _R = TypeVar("_R")


_sep = OPTION_SEPERATORS[2]

def get_size(start_path: str = ".") -> int:
    "Gets the recursive size of a directory"
    total_size = 0
    for dirpath, _, filenames in os.walk(start_path):
        for file in filenames:
            file_path = os.path.join(dirpath, file)
            total_size += os.path.getsize(file_path)

    return total_size

def emojitoword(text: str) -> str:
    "Replaces discord emojis with an alternates that can be spoken"
    output = []
    words = text.split(" ")

    for word in words:
        if EMOJI_REGEX.match(word):
            output.append(f"emoji {word.split(':')[1]}")
        elif ANIMATED_EMOJI_REGEX.match(word):
            output.append(f"animated emoji {word.split(':')[1]}")
        else:
            output.append(word)

    return " ".join(output)

def exts_to_format(attachments: Sequence[discord.Attachment]) -> Optional[str]:
    "Returns a description of the given attachment(s)"
    if not attachments:
        return None

    if len(attachments) >= 2:
        return "multiple files"

    ext = attachments[0].filename.split(".")[-1]
    returned_format_gen = (file_type for exts, file_type in READABLE_TYPE.items() if ext in exts)

    return next(returned_format_gen, "a file")

def to_async(coro: Awaitable[_R], loop: asyncio.AbstractEventLoop = None) -> _R:
    """Gets to an async env and returns the coro's result
    Notes: Can be used for swapping threads, if loop is passed."""

    if not loop:
        loop = asyncio.get_event_loop()
        if loop.is_running():
            raise RuntimeError

        return loop.run_until_complete(coro)

    return asyncio.run_coroutine_threadsafe(coro, loop).result()

async def get_redis_info(cache_db: aioredis.Redis) -> str:
    rstats = await cache_db.info("stats")
    hits: int = rstats["keyspace_hits"]
    misses: int = rstats["keyspace_misses"]

    # Redis is actually stupid, so stats reset on server restart... :(
    if not (hits and misses):
        return ""

    total_queries = hits + misses
    hit_rate = (hits / (total_queries)) * 100
    return cleandoc(f"""
        Redis Info:
        {_sep} `Total Queries: {total_queries}`
        {_sep} `Hit Rate:      {hit_rate:.2f}%`

        {_sep} `Key Hits:      {hits}`
        {_sep} `Key Misses:    {misses}`
    """)


if sys.version_info >= (3, 9):
    removeprefix = str.removeprefix
else:
    def removeprefix(self: str, __prefix: str) -> str:
        "str.removeprefix but for older python versions"
        return self[len(__prefix):] if self.startswith(__prefix) else self
