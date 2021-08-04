from __future__ import annotations

from typing import Any, Coroutine, Type, cast

import discord

from .classes import TypedGuildContext, TypedMessage


class GenericView(discord.ui.View):
    message: TypedMessage
    def __init__(self, ctx: TypedGuildContext, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.ctx = ctx

    @classmethod
    def from_item(cls,
        item: Type[discord.ui.Item[GenericView]],
        ctx: TypedGuildContext, *args: Any, **kwargs: Any
    ):
        self = cls(ctx)
        self.add_item(item(ctx, *args, **kwargs)) # type: ignore
        return self


    def _clean_args(self, *args: Any):
        return [arg for arg in args if arg is not None][1:]

    def recall_command(self, *args: Any) -> Coroutine[Any, Any, Any]:
        self.stop()
        return self.ctx.command(*self._clean_args(*self.ctx.args), *args)


    async def on_error(self, *args: Any) -> None:
        self.ctx.bot.dispatch("interaction_error", *args)

    async def interaction_check(self, interaction: discord.Interaction) -> bool:
        assert interaction.user is not None
        assert interaction.channel is not None
        assert isinstance(interaction.user, discord.Member)

        if interaction.user != self.ctx.author:
            await interaction.response.send_message("You don't own this interaction!", ephemeral=True)
            return False

        permissions = interaction.channel.permissions_for(interaction.user)
        if not permissions.administrator:
            await interaction.response.send_message("You don't have permission use this interaction!", ephemeral=True)
            return False

        return True


class BoolView(GenericView):
    @discord.ui.button(label="True", style=discord.ButtonStyle.success)
    async def yes(self, *_):
        await self.recall_command(True)

    @discord.ui.button(label="False", style=discord.ButtonStyle.danger)
    async def no(self, *_):
        await self.recall_command(False)

    def stop(self) -> None:
        super().stop()
        for button in self.children:
            button = cast(discord.ui.Button, button)
            button.disabled = True

        self.ctx.bot.create_task(self.message.edit(view=self))

class GenericItemMixin:
    view: GenericView

class ChannelSelector(GenericItemMixin, discord.ui.Select):
    def __init__(self, ctx: TypedGuildContext, *args: Any, **kwargs: Any):
        self.ctx = ctx
        self.channels = ctx.guild.text_channels

        discord.ui.Select.__init__(self, *args, **kwargs, options=[
            discord.SelectOption(
                label=f"""#{
                    (channel.name[:20] + '...')
                    if len(channel.name) >= 25
                    else channel.name
                }""",
                value=str(channel.id)
            )
            for channel in self.channels
        ])

    async def callback(self, interaction: discord.Interaction):
        channel = self.ctx.guild.get_channel(int(self.values[0]))

        if channel is None:
            self.ctx.bot.logger.error(f"Couldn't find channel: {channel}")

            err = f"Sorry, but that channel has been deleted!"
            await interaction.response.send_message(err, ephemeral=True)

            new_view = GenericView.from_item(self.__class__, self.ctx)
            await self.view.message.edit(view=new_view)
        else:
            await self.view.recall_command(channel)
            await self.view.message.delete()
