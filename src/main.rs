mod args;
mod config;
mod logger;
mod telegram;
mod utils;

use crate::args::{Args, ParsedArgs, SetupArgs};
use crate::config::FileConfig;
use crate::telegram::SendTg;
use anyhow::{Context, Result, anyhow};
use std::io::{self, Write};
use std::process;

fn run() -> Result<()> {
    match Args::parse()? {
        ParsedArgs::Setup(setup_args) => handle_setup(setup_args),
        ParsedArgs::ShowConfig => handle_show_config(),
        ParsedArgs::Run(args) => {
            let mut client = SendTg::new(
                args.api_url.clone(),
                args.bot_token.clone(),
                args.chat_id.clone(),
            )?;
            client.run(&args)?;
            Ok(())
        }
    }
}

fn prompt_input(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush().context("Failed to flush stdout")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input from stdin")?;
    Ok(input.trim().to_string())
}

fn normalize_owned(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_option(value: Option<String>) -> Option<String> {
    value.and_then(normalize_owned)
}

fn ensure_value(target: &mut Option<String>, provided: Option<String>, label: &str) -> Result<()> {
    if let Some(value) = provided.and_then(normalize_owned) {
        *target = Some(value);
        return Ok(());
    }

    loop {
        let prompt = if target.is_some() {
            format!("{label} (leave blank to keep current): ")
        } else {
            format!("{label}: ")
        };

        let input = prompt_input(&prompt)?;
        if input.is_empty() {
            if target.is_some() {
                return Ok(());
            }
            println!("{label} is required.");
            continue;
        }

        if let Some(value) = normalize_owned(input) {
            *target = Some(value);
            return Ok(());
        }
    }
}

fn handle_setup(setup_args: SetupArgs) -> Result<()> {
    let mut existing: FileConfig = crate::config::load_config()?.unwrap_or_default();

    existing.api_url = normalize_option(existing.api_url);
    existing.bot_token = normalize_option(existing.bot_token);
    existing.chat_id = normalize_option(existing.chat_id);

    ensure_value(&mut existing.api_url, setup_args.api_url.clone(), "API URL")?;
    ensure_value(
        &mut existing.bot_token,
        setup_args.bot_token.clone(),
        "Bot token",
    )?;
    ensure_value(&mut existing.chat_id, setup_args.chat_id.clone(), "Chat ID")?;

    if existing.api_url.is_none() {
        return Err(anyhow!("API URL is required for setup"));
    }
    if existing.bot_token.is_none() {
        return Err(anyhow!("Bot token is required for setup"));
    }
    if existing.chat_id.is_none() {
        return Err(anyhow!("Chat ID is required for setup"));
    }

    let path = crate::config::write_config(&existing)?;
    log_info!("Configuration saved to {}", path.display());
    Ok(())
}

fn handle_show_config() -> Result<()> {
    let path = crate::config::config_file_path()?;
    println!("Configuration file: {}", path.display());

    match crate::config::load_config()? {
        Some(cfg) => {
            let api_url = cfg.api_url.as_deref().unwrap_or("<not set>");
            let bot_token = cfg
                .bot_token
                .as_ref()
                .map(|token| crate::utils::redact_token(token))
                .unwrap_or_else(|| "<not set>".to_string());
            let chat_id = cfg.chat_id.as_deref().unwrap_or("<not set>");

            println!("API URL   : {}", api_url);
            println!("Bot Token : {}", bot_token);
            println!("Chat ID   : {}", chat_id);
        }
        None => {
            println!("No configuration found. Run `sendtg --setup` to create one.");
        }
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        log_error!("{}", err);
        process::exit(1);
    }
}
