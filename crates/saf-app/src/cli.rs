use clap::{Parser, Subcommand};
use saf_app::live_runtime::{DEFAULT_RUN_LIVE_POLL_INTERVAL_MS, MarketActionMode};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "saf")]
#[command(about = "SAF Rust runtime")]
pub(crate) struct Cli {
    #[arg(long, default_value = "config.json5")]
    pub(crate) config: PathBuf,
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    CheckConfig,
    ParseInbox {
        file: PathBuf,
    },
    ProcessInbox {
        file: PathBuf,
        #[arg(long, value_delimiter = ',')]
        running: Vec<String>,
        #[arg(long)]
        state_base_dir: Option<PathBuf>,
        #[arg(long)]
        cofl_auction_api: bool,
    },
    EnqueueCommand {
        #[arg(
            long,
            default_value = ".saf-commands.jsonl",
            env = "SAF_COMMAND_INBOX_FILE"
        )]
        file: PathBuf,
        #[command(subcommand)]
        command: EnqueueCommands,
    },
    RunLive {
        #[arg(
            long,
            default_value = ".saf-commands.jsonl",
            env = "SAF_COMMAND_INBOX_FILE"
        )]
        command_inbox: PathBuf,
        #[arg(long, default_value = ".")]
        state_base_dir: PathBuf,
        #[arg(long, value_enum, default_value_t = MarketActionMode::DryRun)]
        market_actions: MarketActionMode,
        #[arg(long, default_value_t = DEFAULT_RUN_LIVE_POLL_INTERVAL_MS)]
        poll_interval_ms: u64,
        #[arg(long)]
        once: bool,
        #[arg(long)]
        disable_cofl: bool,
    },
    LivePreflight {
        #[arg(
            long,
            default_value = ".saf-commands.jsonl",
            env = "SAF_COMMAND_INBOX_FILE"
        )]
        command_inbox: PathBuf,
        #[arg(long, default_value = ".")]
        state_base_dir: PathBuf,
        #[arg(long)]
        require_discord: bool,
        #[arg(long)]
        require_cofl: bool,
        #[arg(long)]
        require_minecraft: bool,
        #[arg(long)]
        full: bool,
    },
    #[cfg(feature = "live-minecraft")]
    MinecraftAuth {
        account: Option<String>,
        #[arg(long)]
        cache_key: Option<String>,
    },
    State {
        #[command(subcommand)]
        command: StateCommands,
    },
    RouteCommand {
        line: String,
        #[arg(long, value_delimiter = ',')]
        configured: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        running: Vec<String>,
        #[arg(long)]
        default_ign: Option<String>,
    },
    SimulateFlip {
        payload: String,
    },
    PlanCommand {
        line: String,
        #[arg(long, value_delimiter = ',')]
        configured: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        running: Vec<String>,
        #[arg(long)]
        default_ign: Option<String>,
    },
    PlanDiscord {
        payload: String,
    },
    ProcessEnvelope {
        account: String,
        payload: String,
    },
    #[cfg(feature = "discord-webhook")]
    Notify {
        title: String,
        body: String,
        #[arg(long)]
        account: Option<String>,
    },
    #[cfg(feature = "live-cofl")]
    CoflAuthLink {
        account: Option<String>,
        #[arg(long)]
        socket_link: Option<String>,
        #[arg(long, default_value_t = 30_000)]
        timeout_ms: u64,
    },
    #[cfg(feature = "live-cofl")]
    CoflOnce {
        account: String,
        socket_link: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum EnqueueCommands {
    Terminal {
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        line: Vec<String>,
    },
    Transfer {
        from: String,
        to: String,
        #[arg(default_value = "all")]
        amount: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum StateCommands {
    Snapshot {
        account_uuid: String,
        #[arg(long, default_value = ".")]
        base_dir: PathBuf,
    },
    Add {
        account_uuid: String,
        state: String,
        priority: u8,
        action: String,
        #[arg(long, default_value = ".")]
        base_dir: PathBuf,
        #[arg(long)]
        save: bool,
    },
    RemoveNext {
        account_uuid: String,
        #[arg(long, default_value = ".")]
        base_dir: PathBuf,
        #[arg(long)]
        save: bool,
    },
    Clear {
        account_uuid: String,
        #[arg(long, default_value = ".")]
        base_dir: PathBuf,
        #[arg(long)]
        save: bool,
    },
    ClearData {
        account_uuid: String,
        #[arg(long, default_value = ".")]
        base_dir: PathBuf,
        #[arg(long)]
        save: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn enqueue_command_cli_preserves_terminal_line() {
        let cli = Cli::try_parse_from([
            "saf",
            "enqueue-command",
            "--file",
            "commands.jsonl",
            "terminal",
            "/cofl",
            "switchregion",
            "US",
        ])
        .unwrap();

        let Some(Commands::EnqueueCommand { file, command }) = cli.command else {
            panic!("expected enqueue-command");
        };
        let EnqueueCommands::Terminal { line } = command else {
            panic!("expected terminal command");
        };

        assert_eq!(file, PathBuf::from("commands.jsonl"));
        assert_eq!(line.join(" "), "/cofl switchregion US");
    }

    #[test]
    fn enqueue_command_cli_accepts_transfer_defaults() {
        let cli =
            Cli::try_parse_from(["saf", "enqueue-command", "transfer", "Main", "Alt"]).unwrap();

        let Some(Commands::EnqueueCommand { command, .. }) = cli.command else {
            panic!("expected enqueue-command");
        };
        let EnqueueCommands::Transfer { from, to, amount } = command else {
            panic!("expected transfer command");
        };

        assert_eq!(from, "Main");
        assert_eq!(to, "Alt");
        assert_eq!(amount, "all");
    }
}
