use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use audio_drama_editor::settings::EnvDefaults;
use audio_drama_editor::{AppState, Config, TtsConfig};

const DEFAULT_ADDR: &str = "127.0.0.1:3000";

const USAGE: &str = "usage: audio-drama-editor <command>

commands:
  serve [--addr HOST:PORT]   start the server (default 127.0.0.1:3000)

environment:
  ADE_HOME                 data directory (default ~/.audio-drama-editor)
  ADE_ADDR                 listen address when --addr is not given
  ADE_TTS                  set to `off` to start without loading the TTS model
  ADE_TTS_CHECKPOINT       Irodori-TTS checkpoint on Hugging Face
  ADE_TTS_CODEC            DACVAE codec on Hugging Face
  ADE_LLAMA_SERVER_URL     initial llama-server URL (the settings page wins)
  ADE_OPENCODE_BASE_URL    initial OpenCode Go API base URL (the settings page wins)
  ADE_OPENCODE_API_KEY     OpenCode Go API key used when none is saved on the settings page";

fn non_empty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|s| !s.is_empty())
}

fn config() -> anyhow::Result<Config> {
    let home = match env::var_os("ADE_HOME").filter(|h| !h.is_empty()) {
        Some(h) => PathBuf::from(h),
        None => {
            PathBuf::from(env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME is not set"))?)
                .join(".audio-drama-editor")
        }
    };
    let defaults = TtsConfig::default();
    Ok(Config {
        home,
        tts: TtsConfig {
            enabled: non_empty("ADE_TTS").is_none_or(|v| v != "off"),
            checkpoint: non_empty("ADE_TTS_CHECKPOINT").unwrap_or(defaults.checkpoint),
            codec: non_empty("ADE_TTS_CODEC").unwrap_or(defaults.codec),
        },
        env: EnvDefaults {
            llama_server_url: non_empty("ADE_LLAMA_SERVER_URL"),
            opencode_base_url: non_empty("ADE_OPENCODE_BASE_URL"),
            opencode_api_key: non_empty("ADE_OPENCODE_API_KEY"),
        },
    })
}

fn listen_addr(args: &[String], env_addr: Option<String>) -> Result<String, String> {
    match args {
        [] => Ok(env_addr
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_ADDR.into())),
        [flag, addr] if flag == "--addr" => Ok(addr.clone()),
        [flag] if flag.starts_with("--addr=") => Ok(flag["--addr=".len()..].to_string()),
        _ => Err(format!("unexpected arguments: {}", args.join(" "))),
    }
}

async fn serve(args: &[String]) -> anyhow::Result<()> {
    let addr = listen_addr(args, env::var("ADE_ADDR").ok()).map_err(anyhow::Error::msg)?;
    let config = config()?;
    tracing::info!("data directory: {}", config.home.display());
    let state = AppState::open(config).await?;
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("audio-drama-editor listening on http://{addr}");
    axum::serve(listener, audio_drama_editor::app(state)).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("serve") => serve(&args[1..]).await,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_addr_option_wins_over_the_environment_which_wins_over_the_default() {
        let env = || Some("0.0.0.0:4000".to_string());
        assert_eq!(
            listen_addr(&args(&["--addr", "0.0.0.0:3000"]), env()).unwrap(),
            "0.0.0.0:3000"
        );
        assert_eq!(
            listen_addr(&args(&["--addr=100.64.0.1:3000"]), env()).unwrap(),
            "100.64.0.1:3000"
        );
        assert_eq!(listen_addr(&[], env()).unwrap(), "0.0.0.0:4000");
        assert_eq!(listen_addr(&[], None).unwrap(), DEFAULT_ADDR);
    }

    #[test]
    fn unknown_serve_arguments_are_rejected() {
        assert!(listen_addr(&args(&["--port", "3000"]), None).is_err());
        assert!(listen_addr(&args(&["--addr"]), None).is_err());
    }
}
