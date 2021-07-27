from __future__ import annotations

import asyncio
import json
import os
import sys
import traceback
from configparser import ConfigParser
from functools import partial
from os import listdir
from signal import SIGHUP, SIGINT, SIGTERM
from time import monotonic
from typing import (Coroutine, TYPE_CHECKING, Any, Awaitable, Callable, Dict, List,
                    Optional, cast)

import aiohttp
import aioredis
import asyncgTTS
import asyncpg
import discord
import websockets
from discord.ext import commands

import automatic_update
import utils

print("Starting TTS Bot Premium!")
start_time = monotonic()

# Read config file
config = ConfigParser()
config.read("config.ini")

# Setup activity and intents for logging in
activity = discord.Activity(name=config["Activity"]["name"], type=getattr(discord.ActivityType, config["Activity"]["type"]))
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True, reactions=True)
status = getattr(discord.Status, config["Activity"]["status"])

allowed_mentions = discord.AllowedMentions(everyone=False, roles=False)
cache_flags = discord.MemberCacheFlags(online=False, joined=False)

# Custom prefix support
async def prefix(bot: TTSBotPremium, message: discord.Message) -> str:
    "Gets the prefix for a guild based on the passed message object"
    if message.guild:
        return (await bot.settings.get(message.guild, ["prefix"]))[0]

    return "p-"

async def premium_check(ctx: utils.TypedGuildContext):
    if not ctx.bot.patreon_role:
        return

    if not ctx.guild:
        return True

    if str(ctx.author.id) in ctx.bot.trusted:
        return True

    if str(ctx.command) in ("donate", "add_premium") or str(ctx.command).startswith(("jishaku"),):
        return True

    support_server = ctx.bot.get_support_server()
    if not support_server:
        return False

    if not support_server.chunked:
        await support_server.chunk(cache=True)

    premium_user_for_guild = ctx.bot.patreon_json.get(str(ctx.guild.id))
    if any(premium_user_for_guild == member.id for member in ctx.bot.patreon_role.members):
        return True

    print(f"{ctx.author} | {ctx.author.id} failed premium check in {ctx.guild.name} | {ctx.guild.id}")

    permissions = ctx.channel.permissions_for(ctx.guild.me) # type: ignore
    if permissions.send_messages:
        main_msg = f"Hey! This server isn't premium! Please purchase TTS Bot Premium via Patreon! (`{ctx.prefix}donate`)"
        footer_msg = "If this is an error, please contact Gnome!#6669."

        if permissions.embed_links:
            embed = discord.Embed(
                title="TTS Bot Premium",
                description=main_msg,
            )
            embed.set_footer(text=footer_msg)
            embed.set_thumbnail(url=str(ctx.bot.user.avatar_url))

            await ctx.send(embed=embed)
        else:
            await ctx.send(f"{main_msg}\n*{footer_msg}*")

Pool = asyncpg.Pool[asyncpg.Record] if TYPE_CHECKING else asyncpg.Pool
class TTSBotPremium(commands.AutoShardedBot):
    if TYPE_CHECKING:
        from extensions import cache_handler, database_handler
        from player import TTSVoicePlayer

        settings: database_handler.GeneralSettings
        userinfo: database_handler.UserInfoHandler
        nicknames: database_handler.NicknameHandler
        cache: cache_handler.CacheHandler

        command_prefix: Callable[[TTSBotPremium, discord.Message], Coroutine[Any, Any, str]]
        voice_clients: List[TTSVoicePlayer]
        patreon_json: Dict[str, int]
        analytics_buffer: utils.SafeDict
        cache_db: aioredis.Redis
        gtts: asyncgTTS.asyncgTTS
        pool: Pool

        conn: asyncpg.pool.PoolConnectionProxy # temporary conn for updates

        conn: asyncpg.pool.PoolConnectionProxy
        del cache_handler, database_handler, TTSVoicePlayer

    def __init__(self,
        config: ConfigParser,
        session: aiohttp.ClientSession,
        cluster_id: int = None,
    *args, **kwargs):
        self.config = config
        self.websocket = None
        self.session = session
        self.cluster_id = cluster_id
        self.channels: Dict[str, discord.Webhook] = {}

        self.status_code = utils.RESTART_CLUSTER
        self.trusted = config["Main"]["trusted_ids"].strip("[]'").split(", ")

        with open("patreon_users.json") as f:
            self.patreon_json = json.load(f)

        super().__init__(*args, **kwargs)


    @property
    def avatar_url(self) -> str:
        return str(self.user.avatar_url) if self.user else ""

    @property
    def invite_channel(self) -> Optional[discord.TextChannel]:
        support_server = self.get_support_server()
        return support_server.get_channel(835224660458864670) if support_server else None # type: ignore

    @property
    def patreon_role(self) -> Optional[discord.Role]:
        support_server = self.get_support_server()
        if not support_server:
            return

        return discord.utils.get(support_server.roles, name="Patreon!")


    def log(self, event: str) -> None:
        self.analytics_buffer.add(event)

    def get_support_server(self) -> Optional[discord.Guild]:
        return self.get_guild(int(self.config["Main"]["main_server"]))

    def load_extensions(self, folder: str):
        filered_exts = filter(lambda e: e.endswith(".py"), listdir(folder))
        for ext in filered_exts:
            self.load_extension(f"{folder}.{ext[:-3]}")

    def create_websocket(self) -> Awaitable[websockets.WebSocketClientProtocol]:
        host = self.config["Clustering"].get("websocket_host", "localhost")
        port = self.config["Clustering"].get("websocket_port", "8765")

        uri = f"ws://{host}:{port}/{self.cluster_id}"
        return websockets.connect(uri)


    async def get_invite_channel(self) -> Optional[discord.TextChannel]:
        channel_id = 694127922801410119
        support_server = self.get_support_server()
        if support_server is None:
            return await self.fetch_channel(channel_id) # type: ignore
        else:
            return support_server.get_channel(channel_id) # type: ignore

    async def user_from_dm(self, dm_name: str) -> Optional[discord.User]:
        match = utils.ID_IN_BRACKETS_REGEX.search(dm_name)
        if not match:
            return

        real_user_id = int(match.group(1))
        try:
            return await self.fetch_user(real_user_id)
        except commands.UserNotFound:
            return


    def add_check(self, *args, **kwargs):
        super().add_check(*args, **kwargs)
        return self

    def close(self, status_code: Optional[int] = None) -> Awaitable[None]:
        if status_code is not None:
            self.status_code = status_code
            self.logger.debug(f"Shutting down with status code {status_code}")

        return super().close()

    async def wait_until_ready(self, *_: Any, **__: Any) -> None:
        return await super().wait_until_ready()

    async def process_commands(self, message: discord.Message) -> None:
        if message.author.bot:
            return

        ctx_class = utils.TypedGuildContext if message.guild else utils.TypedContext
        ctx = await self.get_context(message=message, cls=ctx_class)

        await self.invoke(ctx)

    async def start(self, token: str, **kwargs):
        "Get everything ready in async env"
        cache_info = self.config["Redis Info"]
        db_info = self.config["PostgreSQL Info"]

        gcp_creds = os.getenv("GOOGLE_APPLICATION_CREDENTIALS")
        if not gcp_creds:
            raise asyncgTTS.AuthorizationException("GOOGLE_APPLICATION_CREDENTIALS is not set or empty!")

        self.cache_db = aioredis.from_url(**cache_info)
        self.pool, self.gtts = await asyncio.gather(
            cast(Awaitable[Pool], asyncpg.create_pool(**db_info)),
            asyncgTTS.setup(
                premium=True,
                session=self.session,
                service_account_json_location=gcp_creds
            ),
        )

        # Fill up bot.channels, as a load of webhooks
        for channel_name, webhook_url in self.config["Webhook URLs"].items():
            adapter = discord.AsyncWebhookAdapter(session=self.session)
            self.channels[channel_name] = discord.Webhook.from_url(
                url=webhook_url, adapter=adapter
            )

        # Load all of /cogs
        self.load_extensions("cogs")
        self.load_extensions("extensions")

        # Send starting message and actually start the bot
        if self.shard_ids is not None:
            prefix = f"`[Cluster] [ID {self.cluster_id}] [Shards {len(self.shard_ids)}]`: "
            kwargs["reconnect"] = False # allow cluster launcher to handle restarting
            self.websocket = await self.create_websocket()
        else:
            prefix = ""
            self.websocket = None

        self.logger = utils.setup_logging(
            aio=True, level=config["Main"]["log_level"],
            session=self.session, prefix=prefix
        )
        self.logger.info("Starting TTS Bot Premium!")

        await automatic_update.do_normal_updates(self)
        await super().start(token, **kwargs)


def get_error_string(e: BaseException) -> str:
    return f"{type(e).__name__}: {e}"

async def only_avaliable(ctx: utils.TypedContext):
    return not ctx.guild.unavailable if ctx.guild else True


async def on_ready(bot: TTSBotPremium):
    await bot.wait_until_ready()
    bot.logger.info(f"Started and ready! Took `{monotonic() - start_time:.2f} seconds`")

async def main(*args, **kwargs) -> int:
    async with aiohttp.ClientSession() as session:
        return await _real_main(session, *args, **kwargs)

async def _real_main(
    session: aiohttp.ClientSession,
    cluster_id: Optional[int] = None,
    total_shard_count: Optional[int] = None,
    shards_to_handle: Optional[List[int]] = None,
) -> int:
    bot = TTSBotPremium(
        config=config,
        status=status,
        intents=intents,
        session=session,
        max_messages=None,
        help_command=None, # Replaced by FancyHelpCommand by FancyHelpCommandCog
        activity=activity,
        command_prefix=prefix,
        cluster_id=cluster_id,
        case_insensitive=True,
        shard_ids=shards_to_handle,
        shard_count=total_shard_count,
        chunk_guilds_at_startup=False,
        member_cache_flags=cache_flags,
        allowed_mentions=allowed_mentions,
    ).add_check(only_avaliable)

    def stop_bot_sync(sig: int, *args, **kwargs):
        bot.status_code = -sig
        bot.logger.warning(f"Recieved signal {sig} and shutting down.")

        bot.loop.create_task(bot.close())

    for sig in (SIGINT, SIGTERM, SIGHUP):
        bot.loop.add_signal_handler(sig, partial(stop_bot_sync, sig))

    await automatic_update.do_early_updates(bot)
    try:
        bot.loop.create_task(on_ready(bot))
        await bot.start(token=config["Main"]["Token"])
        return bot.status_code

    except Exception:
        traceback.print_exception(*sys.exc_info())
        return utils.DO_NOT_RESTART_CLUSTER

    finally:
        if not bot.user:
            return utils.DO_NOT_RESTART_CLUSTER

        closing_coros = [bot.pool.close(), bot.cache_db.close(), bot.close()]
        if bot.websocket is not None:
            closing_coros.append(bot.websocket.close())

        bot.logger.info(f"{bot.user.mention} is shutting down.")
        await asyncio.wait_for(asyncio.gather(*closing_coros), timeout=5)

try:
    import uvloop
    uvloop.install()
except ModuleNotFoundError:
    print("Failed to import uvloop, performance may be reduced")

if __name__ == "__main__":
    asyncio.run(main())
