// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::borrow::Cow;
use std::collections::HashMap;

use itertools::Itertools;

use poise::serenity_prelude as serenity;

use crate::structs::{Context, Result, Error, TTSMode, Data, CommandResult, PoiseContextExt, ApplicationContext, OptionGettext, PollyVoice, TTSModeChoice};
use crate::constants::{OPTION_SEPERATORS, PREMIUM_NEUTRAL_COLOUR};
use crate::funcs::{random_footer, parse_user_or_guild};
use crate::macros::{require_guild, require};
use crate::database;

fn format_voice<'a>(data: &Data, voice: &'a str, mode: TTSMode) -> Cow<'a, str> {
    if mode == TTSMode::gCloud {
        let (lang, variant) = voice.split_once(' ').unwrap();
        let gender = &data.premium_voices[lang][variant];
        Cow::Owned(format!("{lang} - {variant} ({gender})"))
    } else if mode == TTSMode::Polly {
        let voice = &data.polly_voices[voice];
        Cow::Owned(format!("{} - {} ({})", voice.name, voice.language_name, voice.gender))
    } else {
        Cow::Borrowed(voice)
    }
}

/// Displays the current settings!
#[poise::command(
    category="Settings",
    guild_only, prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS")
]
#[allow(clippy::too_many_lines)]
pub async fn settings(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let author_id = ctx.author().id;

    let data = ctx.data();
    let ctx_discord = ctx.discord();
    let none_str = ctx.gettext("none");

    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let userinfo_row = data.userinfo_db.get(author_id.into()).await?;
    let nickname_row = data.nickname_db.get([guild_id.into(), author_id.into()]).await?;

    let default_channel_name = || Cow::Borrowed("has not been set up yet");
    let channel_name = if guild_row.channel == 0 {
        ctx_discord.cache
            .guild_channel_field(guild_row.channel as u64, |c| c.name.clone())
            .map_or_else(default_channel_name, Cow::Owned)
    } else {
        default_channel_name()
    };

    let prefix = &guild_row.prefix;
    let guild_mode = guild_row.voice_mode;
    let nickname = nickname_row.name.as_deref().unwrap_or(none_str);
    let target_lang = guild_row.target_lang.as_deref().unwrap_or(none_str);

    let user_mode = if guild_mode.is_premium() {
        userinfo_row.premium_voice_mode
    } else {
        userinfo_row.voice_mode
    };

    let guild_voice_row = data.guild_voice_db.get((guild_id.into(), guild_mode)).await?;
    let default_voice = {
        if guild_voice_row.guild_id == 0 {
            Cow::Borrowed(guild_mode.default_voice())
        } else {
            format_voice(data, &guild_voice_row.voice, guild_mode)
        }
    };

    let user_voice_row;
    let user_voice = {
        let currently_set_voice_mode = user_mode.unwrap_or(guild_mode);
        user_voice_row = data.user_voice_db.get((author_id.into(), currently_set_voice_mode)).await?;

        user_voice_row.voice.as_ref().map_or(
            Cow::Borrowed(none_str),
            |voice| format_voice(data, voice, currently_set_voice_mode)
        )
    };

    let (speaking_rate, speaking_rate_kind) =
        if let Some(mode) = user_mode {
            let user_voice_row = data.user_voice_db.get((author_id.into(), mode)).await?;
            let (default, kind) = mode.speaking_rate_info().map_or((1.0, "x"), |(_, d, _, k)| (d, k));

            (
                Cow::Owned(user_voice_row.speaking_rate.unwrap_or(default).to_string()),
                kind,
            )
        } else {
            (Cow::Borrowed("1.0"), "x")
        };

    let neutral_colour = ctx.neutral_colour().await;
    let [sep1, sep2, sep3, sep4] = OPTION_SEPERATORS;

    ctx.send(|b| {b.embed(|e| {e
        .title("Current Settings")
        .url(&data.config.main_server_invite)
        .colour(neutral_colour)
        .footer(|f| f.text(ctx.gettext(
            "Change these settings with `/set {property} {value}`!\nNone = setting has not been set yet!"
        )))

        .field(ctx.gettext("**General Server Settings**"), ctx.gettext("
{sep1} Setup Channel: `#{channel_name}`
{sep1} Command Prefix: `{prefix}`
{sep1} Auto Join: `{autojoin}`
        ")
            .replace("{sep1}", sep1)
            .replace("{prefix}", prefix)
            .replace("{autojoin}", &guild_row.auto_join.to_string())
            .replace("{channel_name}", &channel_name),
        false)
        .field("**TTS Settings**", ctx.gettext("
{sep2} <User> said: message: `{xsaid}`
{sep2} Ignore bot's messages: `{bot_ignore}`
{sep2} Ignore audience messages: `{audience_ignore}`
{sep2} Require users in voice channel: `{require_voice}`

**{sep2} Default Server Voice Mode: `{guild_mode}`**
**{sep2} Default Server Voice: `{default_voice}`**

{sep2} Max Time to Read: `{msg_length} seconds`
{sep2} Max Repeated Characters: `{repeated_chars}`
        ")
            .replace("{sep2}", sep2)
            .replace("{xsaid}", &guild_row.xsaid.to_string())
            .replace("{bot_ignore}", &guild_row.bot_ignore.to_string())
            .replace("{audience_ignore}", &guild_row.audience_ignore.to_string())
            .replace("{require_voice}", &guild_row.require_voice.to_string())
            .replace("{guild_mode}", guild_mode.into())
            .replace("{default_voice}", &default_voice)
            .replace("{msg_length}", &guild_row.msg_length.to_string())
            .replace("{repeated_chars}", &guild_row.repeated_chars.to_string()),
        false)
        .field(ctx.gettext("**Translation Settings (Premium Only)**"), ctx.gettext("
{sep4} Translation: `{to_translate}`
{sep4} Translation Language: `{target_lang}`
        ")
            .replace("{sep4}", sep4)
            .replace("{to_translate}", &guild_row.to_translate.to_string())
            .replace("{target_lang}", target_lang),
        false)
        .field("**User Specific**", #[allow(clippy::redundant_closure_for_method_calls)] ctx.gettext("
{sep3} Voice: `{user_voice}`
{sep3} Voice Mode: `{voice_mode}`
{sep3} Nickname: `{nickname}`
{sep3} Speaking Rate: `{speaking_rate}{speaking_rate_kind}`
        ")
            .replace("{sep3}", sep3)
            .replace("{user_voice}", &user_voice)
            .replace("{voice_mode}", user_mode.map_or(none_str, |m| m.into()))
            .replace("{nickname}", nickname)
            .replace("{speaking_rate}", &speaking_rate)
            .replace("{speaking_rate_kind}", speaking_rate_kind),
        false)
    })}).await.map(drop).map_err(Into::into)
}


struct MenuPaginator<'a> {
    index: usize,
    mode: TTSMode,
    ctx: Context<'a>,
    pages: Vec<String>,
    footer: Cow<'a, str>,
    current_voice: String,
}

impl<'a> MenuPaginator<'a> {
    pub fn new(ctx: Context<'a>, pages: Vec<String>, current_voice: String, mode: TTSMode, footer: Cow<'a, str>) -> Self {
        Self {
            ctx, pages, current_voice, mode, footer,
            index: 0,
        }
    }


    fn create_page<'b>(&self, embed: &'b mut serenity::CreateEmbed, page: &str) -> &'b mut serenity::CreateEmbed {
        let author = self.ctx.author();

        embed
            .title(self.ctx.discord().cache.current_user_field(|u| self.ctx
                .gettext("{bot_user} Voices | Mode: `{mode}`")
                .replace("{mode}", self.mode.into())
                .replace("{bot_user}", &u.name)
            ))
            .author(|a| a
                .name(author.name.clone())
                .icon_url(author.face())
            )
            .description(self.ctx.gettext("**Currently Supported Voice**\n{page}").replace("{page}", page))
            .field(self.ctx.gettext("Current voice used"), &self.current_voice, false)
            .footer(|f| f.text(self.footer.to_string()))
    }

    fn create_action_row<'b>(&self, builder: &'b mut serenity::CreateActionRow, disabled: bool) -> &'b mut serenity::CreateActionRow {
        for emoji in ["⏮️", "◀", "⏹️", "▶️", "⏭️"] {
            builder.create_button(|b| {b
                .custom_id(emoji)
                .style(serenity::ButtonStyle::Primary)
                .emoji(serenity::ReactionType::Unicode(String::from(emoji)))
                .disabled(
                    disabled ||
                    (["⏮️", "◀"].contains(&emoji) && self.index == 0) ||
                    (["▶️", "⏭️"].contains(&emoji) && self.index == (self.pages.len() - 1))
                )
            });
        };
        builder
    }

    async fn create_message(&self) -> serenity::Result<serenity::Message> {
        self.ctx.send(|b| b
            .embed(|e| self.create_page(e, &self.pages[self.index]))
            .components(|c| c.create_action_row(|r| self.create_action_row(r, false)))
        ).await?.message().await
    }

    async fn edit_message(&self, message: &mut serenity::Message, disable: bool) -> serenity::Result<()> {
        message.edit(self.ctx.discord(), |b| b
            .embed(|e| self.create_page(e, &self.pages[self.index]))
            .components(|c| c.create_action_row(|r| self.create_action_row(r, disable)))
        ).await
    }


    pub async fn start(mut self) -> serenity::Result<()> {
        let ctx_discord = self.ctx.discord();
        let mut message = self.create_message().await?;

        loop {
            let collector = message
                .await_component_interaction(&ctx_discord.shard)
                .timeout(std::time::Duration::from_secs(60 * 5))
                .author_id(self.ctx.author().id)
                .collect_limit(1);

            let interaction = require!(collector.await, Ok(()));
            match interaction.data.custom_id.as_str() {
                "⏮️" => {
                    self.index = 0;
                    self.edit_message(&mut message, false).await?;
                },
                "◀" => {
                    self.index -= 1;
                    self.edit_message(&mut message, false).await?;
                },
                "⏹️" => {
                    self.edit_message(&mut message, true).await?;
                    return interaction.defer(&ctx_discord.http).await
                },
                "▶️" => {
                    self.index += 1;
                    self.edit_message(&mut message, false).await?;
                },
                "⏭️" => {
                    self.index = self.pages.len() - 1;
                    self.edit_message(&mut message, false).await?;
                },
                _ => unreachable!()
            };
            interaction.defer(&ctx_discord.http).await?;
        }
    }
}

async fn voice_autocomplete(ctx: ApplicationContext<'_>, searching: String) -> Vec<poise::AutocompleteChoice<String>> {
    let (_, mode) = match parse_user_or_guild(ctx.data, ctx.discord, ctx.interaction.user().id, ctx.interaction.guild_id()).await {
        Ok(v) => v,
        Err(_) => return Vec::new()
    };

    let voices: Box<dyn Iterator<Item=poise::AutocompleteChoice<String>>> = match mode {
        TTSMode::gTTS => Box::new(ctx.data.gtts_voices.clone().into_iter().map(|(value, name)| poise::AutocompleteChoice {name, value})) as _,
        TTSMode::eSpeak => Box::new(ctx.data.espeak_voices.clone().into_iter().map(poise::AutocompleteChoice::from)) as _,
        TTSMode::Polly => Box::new(
            ctx.data.polly_voices.iter().map(|(_, voice)| poise::AutocompleteChoice{
                name: format!("{} - {} ({})", voice.id, voice.name, voice.gender),
                value: voice.id.clone()
            })
        ),
        TTSMode::gCloud => Box::new(
            ctx.data.premium_voices.iter().flat_map(|(language, variants)| {
                variants.iter().map(move |(variant, gender)| {
                    poise::AutocompleteChoice {
                        name: format!("{language} {variant} ({gender})"),
                        value: format!("{language} {variant}")
                    }
                })
            })
        ) as _
    };

    let mut filtered_voices: Vec<_> = voices
        .filter(|choice| choice.name.starts_with(&searching))
        .collect();

    filtered_voices.sort_by_key(|choice| strsim::levenshtein(&choice.name, &searching));
    filtered_voices
}

async fn bool_button(ctx: Context<'_>, value: Option<bool>) -> Result<Option<bool>, Error> {
    crate::funcs::bool_button(
        ctx,
        ctx.gettext("What would you like to set this to?"),
        ctx.gettext("True"), ctx.gettext("False"),
        value
    ).await
}


enum Target {
    Guild,
    User(TTSMode)
}

#[allow(clippy::too_many_arguments)]
async fn change_mode<'a, CacheKey, RowT>(
    ctx: &'a Context<'_>,
    general_db: &database::Handler<CacheKey, RowT>,
    guild_id: serenity::GuildId,
    identifier: CacheKey, mode: Option<TTSMode>,
    target: Target
) -> Result<Option<Cow<'a, str>>, Error>
where
    CacheKey: database::CacheKeyTrait + std::hash::Hash + std::cmp::Eq + Send + Sync + Copy,
    RowT: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Sync + Unpin,
{
    let data = ctx.data();
    if mode.map_or(false, TTSMode::is_premium) && crate::premium_check(ctx.discord(), data, Some(guild_id)).await?.is_some() {
        ctx.send(|b| b.embed(|e| {e
            .title("TTS Bot Premium")
            .colour(PREMIUM_NEUTRAL_COLOUR)
            .thumbnail(data.premium_avatar_url.clone())
            .url("https://www.patreon.com/Gnome_the_Bot_Maker")
            .footer(|f| f.text(ctx.gettext("If this is an error, please contact Gnome!#6669.")))
            .description(ctx.gettext("
                The `Premium` TTS Mode is only for TTS Bot Premium subscribers, please check out the `/premium` command!
                If this server has purchased premium, please run the `/activate` command to link yourself to this server!
            "))
        })).await?;
        Ok(None)
    } else {
        let get_key = |mode: TTSMode| if mode.is_premium() {
            "premium_voice_mode"
        } else {
            "voice_mode"
        };

        let key = if let Some(mode) = mode {
            get_key(mode)
        } else if let Target::User(user_mode) = target {
            // If the mode is being deleted, we can't just rely on the mode check on None...
            // so we check the currently set mode for premium instead.
            get_key(user_mode)
        } else {
            "voice_mode"
        };

        general_db.set_one(identifier, key, &mode).await?;

        Ok(Some(match mode {
            Some(mode) => Cow::Owned(match target {
                Target::Guild => ctx.gettext("Changed the server TTS Mode to: {mode}"),
                Target::User(_) => ctx.gettext("Changed your TTS Mode to: {mode}")
            }.replace("{mode}", mode.into())),
            None => Cow::Borrowed(match target {
                Target::Guild => ctx.gettext("Reset the server mode"),
                Target::User(_) => ctx.gettext("Reset your mode")
            })
        }))
    }
}

#[allow(clippy::too_many_arguments)]
async fn change_voice<'a, T, RowT1, RowT2>(
    ctx: &'a Context<'a>,
    general_db: &database::Handler<T, RowT1>,
    voice_db: &database::Handler<(T, TTSMode), RowT2>,
    author_id: serenity::UserId, guild_id: serenity::GuildId,
    key: T, voice: Option<String>,
    target: Target,
) -> Result<Cow<'a, str>, Error>
where
    RowT1: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Sync + Unpin,
    RowT2: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Sync + Unpin,

    T: database::CacheKeyTrait + std::hash::Hash + std::cmp::Eq + Send + Sync + Copy,
    (T, TTSMode): database::CacheKeyTrait,
{
    let (_, mode) = parse_user_or_guild(ctx.data(), ctx.discord(), author_id, Some(guild_id)).await?;
    Ok(if let Some(voice) = voice {
        if check_valid_voice(ctx.data(), &voice, mode) {
            general_db.create_row(key).await?;
            voice_db.set_one((key, mode), "voice", &voice).await?;
            Cow::Owned(match target {
                Target::Guild => ctx.gettext("Changed the server voice to: {voice}"),
                Target::User(_) => ctx.gettext("Changed your voice to {voice}")
            }.replace("{voice}", &voice))
        } else {
            Cow::Borrowed(ctx.gettext("Invalid voice, do `/voices`"))
        }
    } else {
        voice_db.delete((key, mode)).await?;
        Cow::Borrowed(match target {
            Target::Guild => ctx.gettext("Reset the server voice"),
            Target::User(_) => ctx.gettext("Reset your voice")
        })
    })
}

fn check_valid_voice(data: &Data, voice: &String, mode: TTSMode) -> bool {
    match mode {
        TTSMode::gTTS => data.gtts_voices.contains_key(voice),
        TTSMode::eSpeak => data.espeak_voices.contains(voice),
        TTSMode::Polly => data.polly_voices.contains_key(voice),
        TTSMode::gCloud => {
            voice.split_once(' ')
                .and_then(|(language, variant)| data.premium_voices.get(language).map(|l| (l, variant)))
                .map_or(false, |(ls, v)| ls.contains_key(v))
        }
    }
}

async fn get_translation_langs(reqwest: &reqwest::Client, token: &str) -> Result<Vec<String>, Error> {
    Ok(
        reqwest
            .get(format!("{}/languages", crate::constants::TRANSLATION_URL))
            .query(&serenity::json::prelude::json!({
                "type": "target",
                "auth_key": token
            }))
            .send().await?
            .error_for_status()?
            .json::<Vec<crate::structs::DeeplVoice>>().await?
            .iter().map(|v| v.language.to_lowercase()).collect()
    )
}



fn to_enabled(catalog: Option<&gettext::Catalog>, value: bool) -> &str {
    if value {
        catalog.gettext("Enabled")
    } else {
        catalog.gettext("Disabled")
    }
}

/// Changes a setting!
#[poise::command(category="Settings", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES | EMBED_LINKS")]
pub async fn set(ctx: Context<'_>, ) -> CommandResult {
    crate::commands::help::_help(ctx, Some("set")).await
}

/// Owner only: used to block a user from dms
#[poise::command(
    prefix_command,
    category="Settings",
    owners_only, hide_in_help,
    required_bot_permissions="SEND_MESSAGES"
)]
pub async fn block(
    ctx: Context<'_>,
    user: serenity::UserId,
    value: bool
) -> CommandResult {
    ctx.data().userinfo_db.set_one(user.into(), "dm_blocked", &value).await?;
    ctx.say(ctx.gettext("Done!")).await?;
    Ok(())
}

/// Makes the bot say "<user> said" before each message
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES"
)]
pub async fn xsaid(
    ctx: Context<'_>,
    #[description="Whether to say \"<user> said\" before each message"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "xsaid", &value).await?;
    ctx.say(ctx.gettext("xsaid is now: {}").replace("{}", to_enabled(ctx.current_catalog(), value))).await?;

    Ok(())
}

/// Makes the bot join the voice channel automatically when a message is sent in the setup channel
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("auto_join")
)]
pub async fn autojoin(
    ctx: Context<'_>,
    #[description="Whether to autojoin voice channels"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "auto_join", &value).await?;
    ctx.say(ctx.gettext("Auto Join is now: {}").replace("{}", to_enabled(ctx.current_catalog(), value))).await?;

    Ok(())
}

/// Makes the bot ignore messages sent by bots and webhooks 
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("bot_ignore", "ignore_bots", "ignorebots")
)]
pub async fn botignore(
    ctx: Context<'_>,
    #[description="Whether to ignore messages sent by bots and webhooks"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "bot_ignore", &value).await?;
    ctx.say(ctx.gettext("Ignoring bots is now: {}").replace("{}", to_enabled(ctx.current_catalog(), value))).await?;

    Ok(())
}

/// Makes the bot require people to be in the voice channel to TTS
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("voice_require", "require_in_vc")
)]
pub async fn require_voice(
    ctx: Context<'_>,
    #[description="Whether to require people to be in the voice channel to TTS"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "require_voice", &value).await?;
    ctx.say(ctx.gettext("Requiring users to be in voice channel for TTS is now: {}").replace("{}", to_enabled(ctx.current_catalog(), value))).await?;

    Ok(())
}

/// Changes the default mode for TTS that messages are read in
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("server_voice_mode", "server_tts_mode", "server_ttsmode")
)]
pub async fn server_mode(
    ctx: Context<'_>,
    #[description="The TTS Mode to change to"] mode: TTSModeChoice
) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();

    let data = ctx.data();
    let to_send = change_mode(
        &ctx, &data.guilds_db,
        guild_id, guild_id.into(),
        Some(TTSMode::from(mode)), Target::Guild
    ).await?;

    if let Some(to_send) = to_send {
        ctx.say(to_send).await?;
    };
    Ok(())
}

/// Changes the default language messages are read in
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("defaultlang", "default_lang", "defaultlang", "slang", "serverlanguage")
)]
pub async fn server_voice(
    ctx: Context<'_>,
    #[description="The default voice to read messages in"] #[autocomplete="voice_autocomplete"] #[rest] voice: String
) -> CommandResult {
    let data = ctx.data();
    let guild_id = ctx.guild_id().unwrap();

    let to_send = change_voice(
        &ctx, &data.guilds_db, &data.guild_voice_db,
        ctx.author().id, guild_id, guild_id.into(), Some(voice),
        Target::Guild
    ).await?;

    ctx.say(to_send).await?;
    Ok(())
}

/// Whether to use DeepL translate to translate all TTS messages to the same language 
#[poise::command(
    guild_only,
    category="Settings",
    check="crate::premium_command_check",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("translate", "to_translate", "should_translate")
)]
pub async fn translation(ctx: Context<'_>, #[description="Whether to translate all messages to the same language"] value: Option<bool>) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "to_translate", &value).await?;
    ctx.say(ctx.gettext("Translation is now: {}").replace("{}", to_enabled(ctx.current_catalog(), value))).await?;

    Ok(())
}

/// Changes the target language for translation
#[poise::command(
    guild_only,
    category="Settings",
    check="crate::premium_command_check",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("tlang", "tvoice", "target_lang", "target_voice", "target_language")
)]
pub async fn translation_lang(
    ctx: Context<'_>,
    #[description="The language to translate all TTS messages to"] lang: Option<String>
) -> CommandResult {
    let data = ctx.data();
    let guild_id = ctx.guild_id().unwrap().into();

    let translation_langs = get_translation_langs(
        &data.reqwest,
        data.config.translation_token.as_ref().expect("Tried to do translation without token set in config!")
    ).await?;

    match lang {
        Some(lang) if translation_langs.contains(&lang) => {
            data.guilds_db.set_one(guild_id, "target_lang", &lang).await?;

            let mut to_say = ctx.gettext("The target translation language is now: {}").replace("{}", &lang);
            if data.guilds_db.get(guild_id).await?.to_translate {
                to_say.push_str(ctx.gettext("You may want to enable translation with `/set translation on`"));
            };

            ctx.say(to_say).await?;
        },
        _ => {
            ctx.send(|b| b.embed(|e| {e
                .title(ctx.gettext("DeepL Translation - Supported languages"))
                .description(format!("```{}```", translation_langs.iter().join(", ")))
            })).await?;
        }
    }

    Ok(())
}


/// Changes the prefix used before commands
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
)]
pub async fn prefix(
    ctx: Context<'_>,
    #[description="The prefix to be used before commands"] #[rest] prefix: String
) -> CommandResult {
    let to_send = if prefix.len() <= 5 && prefix.matches(' ').count() <= 1 {
        ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "prefix", &prefix).await?;
        Cow::Owned(ctx.gettext("Command prefix for this server is now: {prefix}").replace("{prefix}", &prefix))
    } else {
        Cow::Borrowed(ctx.gettext("**Error**: Invalid Prefix, please use 5 or less characters with maximum 1 space"))
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the max repetion of a character (0 = off)
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("repeated_chars", "repeated_letters", "chars")
)]
pub async fn repeated_characters(ctx: Context<'_>, #[description="The max repeated characters"] chars: u8) -> CommandResult {
    let to_send = {
        if chars > 100 {
            Cow::Borrowed(ctx.gettext("**Error**: Cannot set the max repeated characters above 100"))
        } else if chars < 5 && chars != 0 {
            Cow::Borrowed(ctx.gettext("**Error**: Cannot set the max repeated characters below 5"))
        } else {
            ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "repeated_chars", &(chars as i16)).await?;
            Cow::Owned(ctx.gettext("Max repeated characters is now: {}").replace("{}", &chars.to_string()))
        }
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the max length of a TTS message in seconds
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("max_length", "message_length")
)]
pub async fn msg_length(ctx: Context<'_>, #[description="Max length of TTS message in seconds"] seconds: u8) -> CommandResult {
    let to_send = {
        if seconds > 60 {
            Cow::Borrowed(ctx.gettext("**Error**: Cannot set the max length of messages above 60 seconds"))
        } else if seconds < 10 {
            Cow::Borrowed(ctx.gettext("**Error**: Cannot set the max length of messages below 10 seconds"))
        } else {
            ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "msg_length", &(seconds as i16)).await?;
            Cow::Owned(ctx.gettext("Max message length is now: {} seconds").replace("{}", &seconds.to_string()))
        }
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Makes the bot ignore messages sent by members of the audience in stage channels
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("audience_ignore", "ignore_audience", "ignoreaudience")
)]
pub async fn audienceignore(
    ctx: Context<'_>,
    #[description="Whether to ignore messages sent by the audience"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "audience_ignore", &value).await?;
    ctx.say(ctx.gettext("Ignoring audience is now: {}").replace("{}", to_enabled(ctx.current_catalog(), value))).await?;
    Ok(())
}

/// Changes the multiplier for how fast to speak
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
    aliases("speed", "speed_multiplier", "speaking_rate_multiplier", "speaking_speed", "tts_speed")
)]
pub async fn speaking_rate(
    ctx: Context<'_>,
    #[description="The speed to speak at"] #[min=0] #[max=400.0] speaking_rate: f32
) -> CommandResult {
    let data = ctx.data();
    let author = ctx.author();

    let (_, mode) = parse_user_or_guild(data, ctx.discord(), author.id, ctx.guild_id()).await?;
    let (min, _, max, kind) = require!(mode.speaking_rate_info(), {
        ctx.say(ctx.gettext("**Error**: Cannot set speaking rate for the {mode} mode").replace("{mode}", mode.into())).await?;
        Ok(())
    });

    let to_send = {
        if speaking_rate > max {
            ctx.gettext("**Error**: Cannot set the speaking rate multiplier above {max}{kind}").replace("{max}", &max.to_string())
        } else if speaking_rate < min {
            ctx.gettext("**Error**: Cannot set the speaking rate multiplier below {min}{kind}").replace("{min}", &min.to_string())
        } else {
            data.userinfo_db.create_row(author.id.0 as i64).await?;
            data.user_voice_db.set_one((author.id.0 as i64, mode), "speaking_rate", &speaking_rate).await?;
            ctx.gettext("Your speaking rate is now: {speaking_rate}{kind}").replace("{speaking_rate}", &speaking_rate.to_string())
        }
    }.replace("{kind}", kind);

    ctx.say(to_send).await?;
    Ok(())
}

/// Replaces your username in "<user> said" with a given name
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
    aliases("nick_name", "nickname", "name"),
)]
pub async fn nick(
    ctx: Context<'_>,
    #[description="The user to set the nick for, defaults to you"] user: Option<serenity::User>,
    #[description="The nickname to set, leave blank to reset"] #[rest] nickname: Option<String>
) -> CommandResult {
    let ctx_discord = ctx.discord();
    let guild = require_guild!(ctx);

    let author = ctx.author();
    let user = user.map_or(Cow::Borrowed(author), Cow::Owned);

    if author.id != user.id && !guild.member(ctx_discord, author).await?.permissions(ctx_discord)?.administrator() {
        ctx.say(ctx.gettext("**Error**: You need admin to set other people's nicknames!")).await?;
        return Ok(())
    }

    let data = ctx.data();

    let to_send =
        if let Some(nick) = nickname {
            if nick.contains('<') && nick.contains('>') {
                Cow::Borrowed(ctx.gettext("**Error**: You can't have mentions/emotes in your nickname!"))
            } else {
                tokio::try_join!(
                    data.guilds_db.create_row(guild.id.into()),
                    data.userinfo_db.create_row(user.id.into())
                )?;

                data.nickname_db.set_one([guild.id.into(), user.id.into()], "name", &nick).await?;
                Cow::Owned(ctx.gettext("Changed {user}'s nickname to {new_nick}").replace("{user}", &user.name).replace("{new_nick}", &nick))
            }
        } else {
            data.nickname_db.delete([guild.id.into(), user.id.into()]).await?;
            Cow::Owned(ctx.gettext("Reset {user}'s nickname").replace("{user}", &user.name))
        };

    ctx.say(to_send).await?;
    Ok(())
}


fn can_send(guild: &serenity::Guild, channel: &serenity::GuildChannel, member: &serenity::Member) -> bool {
    const REQUIRED_PERMISSIONS: serenity::Permissions = serenity::Permissions::from_bits_truncate(
        serenity::Permissions::SEND_MESSAGES.bits() | serenity::Permissions::VIEW_CHANNEL.bits()
    );

    guild.user_permissions_in(channel, member)
        .map(|p| (REQUIRED_PERMISSIONS - p).is_empty())
        .unwrap_or(false)
}


/// Setup the bot to read messages from the given channel
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
)]
pub async fn setup(
    ctx: Context<'_>,
    #[description="The channel for the bot to read messages from"] #[channel_types("Text")]
    channel: Option<serenity::GuildChannel>
) -> CommandResult {
    let guild = require_guild!(ctx);

    let ctx_discord = ctx.discord();
    let cache = &ctx_discord.cache;

    let author = ctx.author();
    let (bot_user_id, bot_user_name, bot_user_face) =
        cache.current_user_field(|u| (u.id, u.name.clone(), u.face()));

    let channel: u64 =
        if let Some(channel) = channel {
            channel.id.into()
        } else {
            let author_member = guild.member(ctx_discord, author).await?;
            let bot_member = guild.member(ctx_discord, bot_user_id).await?;

            let mut text_channels: Vec<&serenity::GuildChannel> = guild.channels.values()
                .filter_map(|c| {match c {
                    serenity::Channel::Guild(channel) => Some(channel),
                    _ => None
                }})
                .filter(|c| {
                    c.kind == serenity::ChannelType::Text &&
                    can_send(&guild, c, &author_member) &&
                    can_send(&guild, c, &bot_member)
                })
                .collect();

            if text_channels.is_empty() {
                ctx.say(ctx.gettext("**Error**: This server doesn't have any text channels that we both have Read/Send Messages in!")).await?;
                return Ok(())
            } else if text_channels.len() >= (25 * 5) {
                ctx.say(ctx.gettext("**Error**: This server has too many text channels to show in a menu! Please run `/setup #channel`")).await?;
                return Ok(())
            };

            text_channels.sort_by(|f, s| Ord::cmp(&f.position, &s.position));

            let message = ctx.send(|b| {b
                .content(ctx.gettext("Select a channel!"))
                .components(|c| {
                    for (i, chunked_channels) in text_channels.chunks(25).enumerate() {
                        c.create_action_row(|r| {
                            r.create_select_menu(|s| {s
                                .custom_id(format!("select::channels::{i}"))
                                .options(|os| {
                                    for channel in chunked_channels {
                                        os.create_option(|o| {o
                                            .label(&channel.name)
                                            .value(channel.id)
                                        });
                                    };
                                    os
                                })
                            })
                        });
                    };
                    c
                })
            }).await?.message().await?;

            let interaction = message
                .await_component_interaction(&ctx_discord.shard)
                .timeout(std::time::Duration::from_secs(60 * 5))
                .author_id(ctx.author().id)
                .collect_limit(1)
                .await;

            if let Some(interaction) = interaction {
                interaction.defer(&ctx_discord.http).await?;
                interaction.data.values[0].parse().unwrap()
            } else {
                // The timeout was hit
                return Ok(())
            }
        };

    let data = ctx.data();
    data.guilds_db.set_one(guild.id.into(), "channel", &(channel as i64)).await?;
    ctx.send(|b| b.embed(|e| {e
        .title(ctx.gettext("{bot_name} has been setup!").replace("{bot_name}", &bot_user_name))
        .thumbnail(bot_user_face)
        .description(ctx.gettext("
TTS Bot will now accept commands and read from <#{channel}>.
Just do `/join` and start talking!
").replace("{channel}", &channel.to_string()))

        .footer(|f| f.text(random_footer(
            &data.config.main_server_invite, cache.current_user_id().0, ctx.current_catalog()
        )))
        .author(|a| {
            a.name(&author.name);
            a.icon_url(author.face())
        })
    })).await?;

    Ok(())
}

/// Changes the voice mode that messages are read in for you
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("voice_mode", "tts_mode", "ttsmode")
)]
pub async fn mode(
    ctx: Context<'_>,
    #[description="The TTS Mode to change to, leave blank for server default"] mode: Option<TTSModeChoice>
) -> CommandResult {
    let userinfo_db = &ctx.data().userinfo_db;
    let author_id = ctx.author().id.into();

    let to_send = change_mode(
        &ctx, userinfo_db,
        ctx.guild_id().unwrap(), author_id,
        mode.map(TTSMode::from),
        Target::User(userinfo_db.get(author_id).await?.voice_mode.unwrap_or_default())
    ).await?;

    if let Some(to_send) = to_send {
        ctx.say(to_send).await?;
    };
    Ok(())
}

/// Changes the voice your messages are read in, full list in `-voices`
#[poise::command(
    guild_only,
    category="Settings",
    aliases("language", "voice"),
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
)]
pub async fn voice(
    ctx: Context<'_>,
    #[description="The voice to read messages in, leave blank to reset"] #[autocomplete="voice_autocomplete"] #[rest] voice: Option<String>
) -> CommandResult {
    let data = ctx.data();
    let author_id = ctx.author().id;
    let guild_id = ctx.guild_id().unwrap();

    let to_send = change_voice(
        &ctx, &data.userinfo_db, &data.user_voice_db,
        author_id, guild_id, author_id.into(), voice,
        Target::User(data.userinfo_db.get(author_id.into()).await?.voice_mode.unwrap_or_default())
    ).await?;

    ctx.say(to_send).await?;
    Ok(())
}

/// Lists all the voices that TTS bot accepts for the current mode
#[poise::command(
    category="Settings",
    aliases("langs", "languages"),
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
)]
pub async fn voices(
    ctx: Context<'_>,
    #[description="The mode to see the voices for, leave blank for current"] mode: Option<TTSModeChoice>
) -> CommandResult {
    let author = ctx.author();
    let data = ctx.data();

    let mode = match mode {
        Some(mode) => TTSMode::from(mode),
        None => parse_user_or_guild(data, ctx.discord(), author.id, ctx.guild_id()).await?.1
    };

    let voices = {
        fn format<'a>(mut iter: impl Iterator<Item=&'a String>) -> String {
            let mut buf = String::with_capacity(iter.size_hint().0 * 2);
            if let Some(first_elt) = iter.next() {
                buf.push('`');
                buf.push_str(first_elt);
                buf.push('`');
                for elt in iter {
                    buf.push_str(", `");
                    buf.push_str(elt);
                    buf.push('`');
                }
            };

            buf
        }

        let random_footer = || random_footer(
            &data.config.main_server_invite,
            ctx.discord().cache.current_user_id().into(),
            ctx.current_catalog(),
        );

        match mode {
            TTSMode::gTTS => format(data.gtts_voices.keys()),
            TTSMode::eSpeak => format(data.espeak_voices.iter()),
            TTSMode::Polly => return {
                let (current_voice, pages) = list_polly_voices(&ctx).await?;
                MenuPaginator::new(ctx, pages, current_voice, mode, random_footer()).start().await.map_err(Into::into)
            },
            TTSMode::gCloud => return {
                let (current_voice, pages) = list_gcloud_voices(&ctx).await?;
                MenuPaginator::new(ctx, pages, current_voice, mode, random_footer()).start().await.map_err(Into::into)
            }
        }
    };

    let cache = &ctx.discord().cache;
    let user_voice_row = data.user_voice_db.get((author.id.into(), mode)).await?;
    ctx.send(|b| b.embed(|e| e
        .title(cache.current_user_field(|u| ctx
            .gettext("{bot_user} Voices | Mode: `{mode}`")
            .replace("{bot_user}", &u.name)
            .replace("{mode}", mode.into())
        ))
        .footer(|f| f.text(random_footer(
            &data.config.main_server_invite, cache.current_user_id().0, ctx.current_catalog()
        )))
        .author(|a| a
            .name(author.name.clone())
            .icon_url(author.face())
        )
        .field(ctx.gettext("Currently supported voices"), voices, true)
        .field(
            ctx.gettext("Current voice used"),
            user_voice_row.voice.as_ref().map_or_else(|| ctx.gettext("None"), std::ops::Deref::deref),
            false
        )
    )).await?;

    Ok(())
}


pub async fn list_polly_voices(ctx: &Context<'_>) -> Result<(String, Vec<String>)> {
    let data = ctx.data();

    let (voice_id, mode) = parse_user_or_guild(data, ctx.discord(), ctx.author().id, ctx.guild_id()).await?;
    let voice = match mode {
        TTSMode::Polly => {
            let voice_id: &str = &voice_id;
            &data.polly_voices[voice_id]
        },
        _ => &data.polly_voices[TTSMode::Polly.default_voice()]
    };

    let mut lang_to_voices: HashMap<&String, Vec<&PollyVoice>> = HashMap::new();
    for voice in data.polly_voices.values() {
        lang_to_voices.entry(&voice.language_name).or_insert_with(Vec::new).push(voice);
    }

    let format_voice = |voice: &PollyVoice| format!("{} - {} ({})\n", voice.id, voice.language_name, voice.gender);
    let pages = lang_to_voices.into_iter().map(|(_, voices)| voices
        .into_iter()
        .map(format_voice)
        .collect()
    ).collect();

    Ok((format_voice(voice).trim_end().to_string(), pages))
}

pub async fn list_gcloud_voices(ctx: &Context<'_>) -> Result<(String, Vec<String>)> {
    let data = ctx.data();

    let (lang_variant, mode) = parse_user_or_guild(data, ctx.discord(), ctx.author().id, ctx.guild_id()).await?;
    let (lang, variant) = match mode {
        TTSMode::gCloud => &lang_variant,
        _ => TTSMode::gCloud.default_voice()
    }.split_once(' ').unwrap();

    let pages = data.premium_voices.iter().map(|(language, variants)| {
        variants.iter().map(|(variant, gender)| {
            format!("{language} {variant} ({gender})\n")
        }).collect()
    }).collect();

    let gender = data.premium_voices[lang][variant];
    Ok((format!("{lang} {variant} ({gender})"), pages))
}
