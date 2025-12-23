use crate::config::FileConfig;
use anyhow::{Result, anyhow};
use clap::{ArgAction, Parser, builder::ValueHint};
use std::path::PathBuf;

const BUTTON_ROW_BREAK: &str = "__ROW_BREAK__";

#[derive(Debug, Clone)]
pub enum ButtonSpec {
    Link { text: String, url: String },
    RowBreak,
}

#[derive(Parser, Debug)]
#[command(
    name = "sendtg:",
    version = crate::config::VERSION_SUMMARY,
    about = "Send text or media through the Telegram Bot API.",
)]
struct Cli {
    #[arg(long = "setup", help = "Interactive config writer; exit after saving.")]
    setup: bool,
    #[arg(long = "show-config", help = "Print current config contents and exit.")]
    show_config: bool,
    #[arg(
        short = 'a',
        long = "api_url",
        help = "Override the Telegram API base URL."
    )]
    api_url: Option<String>,
    #[arg(short = 't', long = "bot_token", help = "Override the bot token.")]
    bot_token: Option<String>,
    #[arg(
        short = 'c',
        long = "chat_id",
        help = "Override the target chat ID.",
        allow_hyphen_values = true
    )]
    chat_id: Option<String>,
    #[arg(
        short = 'm',
        long = "media",
        value_hint = ValueHint::FilePath,
        action = ArgAction::Append,
        num_args = 1..,
        help = "Attach files to send as media."
    )]
    media: Vec<PathBuf>,
    #[arg(long = "spoiler", help = "Flag media as spoiler.")]
    spoiler: bool,
    #[arg(
        long = "streaming",
        help = "Set supports_streaming on videos so they can be played while downloading."
    )]
    streaming: bool,
    #[arg(
        long = "delay",
        value_name = "SECONDS",
        help = "Delay (seconds) between media requests to reduce rate limiting."
    )]
    delay_secs: Option<u64>,
    #[arg(
        long = "no-group",
        alias = "no_group",
        help = "Send media one by one instead of an album."
    )]
    no_group: bool,
    #[arg(
        short = 'F',
        long = "as-file",
        alias = "as_file",
        help = "Send media as documents."
    )]
    as_file: bool,
    #[arg(short = 'C', long = "caption", help = "Caption to reuse across media.")]
    caption: Option<String>,
    #[arg(
        long = "button",
        alias = "button-row-break",
        value_name = "LABEL|URL",
        num_args = 0..=1,
        default_missing_value = BUTTON_ROW_BREAK,
        action = ArgAction::Append,
        help = "Add inline button as 'Label|URL'. Use --button-row-break between buttons to start a new row."
    )]
    buttons: Vec<String>,
    #[arg(
        long = "button-text",
        alias = "button_text",
        hide = true,
        help = "Deprecated: use --button \"Label|URL\" instead."
    )]
    button_text: Option<String>,
    #[arg(
        long = "button-url",
        alias = "button_url",
        hide = true,
        help = "Deprecated: use --button \"Label|URL\" instead."
    )]
    button_url: Option<String>,
    #[arg(long = "silent", help = "Disable notifications for the message.")]
    silent: bool,
    #[arg(long = "check", help = "Check connectivity and credentials only.")]
    check: bool,
    #[arg(
        long = "thread-id",
        alias = "thread_id",
        help = "Target message thread ID for forum topics.",
        allow_hyphen_values = true
    )]
    thread_id: Option<i64>,
    #[arg(help = "Message text when no media is provided.")]
    message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Args {
    pub api_url: String,
    pub bot_token: String,
    pub chat_id: String,
    pub media_paths: Vec<PathBuf>,
    pub spoiler: bool,
    pub streaming: bool,
    pub delay_secs: Option<u64>,
    pub no_group: bool,
    pub as_file: bool,
    pub caption: Option<String>,
    pub buttons: Vec<ButtonSpec>,
    pub message: Option<String>,
    pub check: bool,
    pub silent: bool,
    pub thread_id: Option<i64>,
    pub provided_api_url: bool,
    pub provided_bot_token: bool,
    pub provided_chat_id: bool,
}

#[derive(Debug, Clone)]
pub struct SetupArgs {
    pub api_url: Option<String>,
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ParsedArgs {
    Run(Args),
    Setup(SetupArgs),
    ShowConfig,
}

impl Args {
    pub fn parse() -> Result<ParsedArgs> {
        let cli = Cli::parse();

        if cli.setup {
            return Ok(ParsedArgs::Setup(SetupArgs {
                api_url: cli.api_url.clone(),
                bot_token: cli.bot_token.clone(),
                chat_id: cli.chat_id.clone(),
            }));
        }

        if cli.show_config {
            return Ok(ParsedArgs::ShowConfig);
        }

        let file_config = crate::config::load_config()?;
        let path = crate::config::config_file_path()?;

        let file_config: FileConfig = match file_config {
            Some(cfg) => cfg,
            None => {
                return Err(anyhow!(
                    "Configuration not found at {}. Run `sendtg --setup` first.",
                    path.display()
                ));
            }
        };

        if !file_config.has_required_fields() {
            return Err(anyhow!(
                "Configuration at {} is missing required fields. Run `sendtg --setup` to populate it.",
                path.display()
            ));
        }

        let api_url = cli
            .api_url
            .clone()
            .or_else(|| file_config.api_url.clone())
            .ok_or_else(|| anyhow!("API URL is missing from configuration"))?;
        let bot_token = cli
            .bot_token
            .clone()
            .or_else(|| file_config.bot_token.clone())
            .ok_or_else(|| anyhow!("Bot token is missing from configuration"))?;
        let chat_id = cli
            .chat_id
            .clone()
            .or_else(|| file_config.chat_id.clone())
            .ok_or_else(|| anyhow!("Chat ID is missing from configuration"))?;

        let mut buttons = parse_button_specs(&cli.buttons)?;

        match (&cli.button_text, &cli.button_url) {
            (Some(text), Some(url)) => buttons.push(ButtonSpec::Link {
                text: text.clone(),
                url: url.clone(),
            }),
            (Some(_), None) | (None, Some(_)) => {
                return Err(anyhow!(
                    "Both --button-text and --button-url must be provided together, or use --button \"Label|URL\"."
                ));
            }
            (None, None) => {}
        }

        Ok(ParsedArgs::Run(Args {
            api_url,
            bot_token,
            chat_id,
            media_paths: cli.media.clone(),
            spoiler: cli.spoiler,
            streaming: cli.streaming,
            delay_secs: cli.delay_secs,
            no_group: cli.no_group,
            as_file: cli.as_file,
            caption: cli.caption.clone(),
            buttons,
            message: cli.message.clone(),
            check: cli.check,
            silent: cli.silent,
            thread_id: cli.thread_id,
            provided_api_url: cli.api_url.is_some(),
            provided_bot_token: cli.bot_token.is_some(),
            provided_chat_id: cli.chat_id.is_some(),
        }))
    }
}

fn parse_button_specs(raw: &[String]) -> Result<Vec<ButtonSpec>> {
    let mut specs = Vec::new();

    for entry in raw {
        if entry == BUTTON_ROW_BREAK {
            specs.push(ButtonSpec::RowBreak);
            continue;
        }

        let mut parts = entry.splitn(2, '|');
        let text = parts
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("Invalid --button value '{}': missing label", entry))?;
        let url = parts
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("Invalid --button value '{}': expected 'Label|URL'", entry))?;

        specs.push(ButtonSpec::Link {
            text: text.to_string(),
            url: url.to_string(),
        });
    }

    Ok(specs)
}
