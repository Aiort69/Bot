from __future__ import annotations

import asyncio
import dataclasses
from collections import defaultdict
from typing import (TYPE_CHECKING, Any, Generic, Iterable, Literal, Optional, Tuple,
                    TypeVar, Union, cast)

from discord.ext import tasks
from sql_athame import sql

import utils

if TYPE_CHECKING:
    from main import TTSBotPremium
    from sql_athame.base import SQLFormatter

    sql: SQLFormatter
    del SQLFormatter


_T = TypeVar("_T")
_DK = TypeVar("_DK")
_CACHE_ITEM = dict[Literal[
    "channel", "premium_user", "xsaid", "bot_ignore", "auto_join",
    "to_translate", "formal", "msg_length", "repeated_chars", "prefix",
    "default_lang", "target_lang", "blocked", "lang", "variant", "name",
], Any]

@dataclasses.dataclass
class WriteTask:
    waiter: asyncio.Future[None] = dataclasses.field(default_factory=asyncio.Future)
    changes: _CACHE_ITEM = dataclasses.field(default_factory=dict)


def _unpack_id(identifer: Union[Iterable[_T], _T]) -> tuple[_T, ...]:
    return tuple(identifer) if isinstance(identifer, Iterable) else (identifer,)

def setup(bot: TTSBotPremium):
    bot.settings, bot.userinfo, bot.nicknames = (
        TableHandler[int](bot,
            table_name="guilds", broadcast=True, pkey_columns=("guild_id",),
            select="SELECT * FROM guilds WHERE guild_id = $1",
            delete="DELETE FROM guilds WHERE guild_id = $1",
    ),  TableHandler[int](bot,
            table_name="userinfo", broadcast=False, pkey_columns=("user_id",),
            select="SELECT * FROM userinfo WHERE user_id = $1",
            delete="DELETE FROM userinfo WHERE user_id = $1",
    ),  TableHandler[tuple[int, int]](bot,
            table_name="nicknames", broadcast=False, pkey_columns=("guild_id", "user_id"),
            select="SELECT * from nicknames WHERE guild_id = $1 and user_id = $2",
            delete="DELETE FROM nicknames WHERE guild_id = $1 and user_id = $2",
    ))


class TableHandler(Generic[_DK]):
    def __init__(self, bot: TTSBotPremium,
        broadcast: bool, select: str, delete: str,
        table_name: str, pkey_columns: tuple[str, ...]
    ):
        self.bot = bot
        self.pool = bot.pool

        self.broadcast = broadcast
        self.select_query = select
        self.delete_query = delete
        self.pkey_columns = tuple(sql(pkey) for pkey in pkey_columns)
        self.default_id = 0 if len(pkey_columns) == 1 else (0,) * len(pkey_columns)

        generic_insert_query = """
            INSERT INTO {}({{}})
            VALUES({{}})

            ON CONFLICT ({})
            {}
        """

        args = table_name, ", ".join(pkey_columns)
        self.multi_insert_query = generic_insert_query.format(*args, "DO UPDATE SET ({}) = ({})")
        self.single_insert_query = generic_insert_query.format(*args, "DO UPDATE SET {} = {}")
        self.ensure_row_query = generic_insert_query.format(*args, "DO NOTHING{}{}")

        self._not_fully_fetched: list[_DK] = []
        self._cache: dict[_DK, _CACHE_ITEM] = {}
        self.defaults: Optional[_CACHE_ITEM] = None
        self._write_tasks: defaultdict[_DK, WriteTask] = defaultdict(WriteTask)

        bot.add_listener(self.on_invalidate_cache)


    async def on_invalidate_cache(self, identifier: _DK):
        if isinstance(identifier, list):
            identifier = tuple(identifier) # type: ignore

        self._cache.pop(identifier, None) # type: ignore

    async def _fetch_defaults(self) -> _CACHE_ITEM:
        row = await self.bot.pool.fetchrow(self.select_query, *_unpack_id(self.default_id))
        assert row is not None

        row = dict(row)
        for column in self.pkey_columns:
            del row[list(column)[0]]

        self.defaults = cast(_CACHE_ITEM, row)
        return self.defaults


    def __getitem__(self, identifer: _DK) -> _CACHE_ITEM:
        if identifer not in self._not_fully_fetched:
            return self._cache[identifer]

        raise KeyError

    def __setitem__(self, identifier: _DK, new_settings: _CACHE_ITEM):
        if identifier not in self._cache:
            self._cache[identifier] = {}
            self._not_fully_fetched.append(identifier)

        self._cache[identifier].update(new_settings)
        self._write_tasks[identifier].changes.update(new_settings)

        if not self.insert_writes.is_running():
            self.insert_writes.start()

    def __delitem__(self, identifier: _DK):
        del self._cache[identifier]
        self.bot.create_task(self.bot.pool.execute(
            self.delete_query, identifier
        ))

    def __contains__(self, identifier: _DK):
        return identifier not in self._not_fully_fetched and identifier in self._cache


    async def get(self, identifer: _DK) -> _CACHE_ITEM:
        try:
            return self[identifer]
        except KeyError:
            return await self._fill_cache(identifer)

    async def set(self, identifer: _DK, new_settings: _CACHE_ITEM):
        self[identifer] = new_settings
        await self._write_tasks[identifer].waiter


    @tasks.loop(seconds=1)
    async def insert_writes(self):
        if not self._write_tasks:
            return self.insert_writes.cancel()

        return await asyncio.gather(*(
            self._insert_write(pending_id)
            for pending_id in self._write_tasks.keys()
        ))


    def _get_query(self, raw_id: _DK, task: WriteTask) -> Tuple[Any, ...]:
        no_id_settings = [sql(setting) for setting in task.changes.keys()]
        no_id_values = [sql("{}", value) for value in task.changes.values()]
        identifer = [sql("{}", identifer) for identifer in _unpack_id(raw_id)]

        settings = [*self.pkey_columns, *no_id_settings]
        values   = [*identifer, *no_id_values]

        amount_of_settings = len(no_id_settings)
        if amount_of_settings == 1:
            unformatted_query = self.single_insert_query
        elif amount_of_settings == 0:
            unformatted_query = self.ensure_row_query
        else:
            unformatted_query = self.multi_insert_query

        query = tuple(sql(
            unformatted_query,
            sql.list(settings), sql.list(values),
            sql.list(no_id_settings), sql.list(no_id_values)
        ))
        self.bot.logger.debug(f"query: {query}")
        return query

    async def _insert_write(self, raw_id: _DK):
        task = self._write_tasks.pop(raw_id)

        try:
            query = self._get_query(raw_id, task)
            await self.pool.execute(*query)
        except Exception as error:
            task.waiter.set_exception(error)
        else:
            task.waiter.set_result(None)

        if self.bot.websocket is not None and self.broadcast:
            await self.bot.websocket.send(
                utils.data_to_ws_json("SEND", target="*", **{
                    "c": "invalidate_cache",
                    "a": {"identifer": raw_id},
                })
            )


    async def _fill_cache(self, identifier: _DK) -> _CACHE_ITEM:
        record = await self.pool.fetchrow(self.select_query, *_unpack_id(identifier))
        if record is None:
            item = (self.defaults or await self._fetch_defaults()).copy()
        else:
            item = cast(_CACHE_ITEM, dict(record))
            for column in self.pkey_columns:
                del item[list(column)[0]]

        if identifier in self._not_fully_fetched:
            self._not_fully_fetched.remove(identifier)

        self._cache[identifier] = item
        return item
