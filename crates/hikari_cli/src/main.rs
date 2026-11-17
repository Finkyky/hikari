//! The `hikari` binary: argument parsing and dispatch only.

use clap::{Parser, Subcommand};
use hikari_features::FeatureFlag;

#[derive(Parser)]
#[command(name = "hikari", version, about = "A resource-frugal coding agent.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run a single task non-interactively.
    Exec {
        prompt: String,
        /// Emit the event stream as JSON Lines.
        #[arg(long)]
        json: bool,
    },
    /// Speak JSON-RPC 2.0 over stdio.
    AppServer,
    /// Resume a previous session.
    Resume,
    /// Inspect and reclaim on-disk state.
    Storage {
        #[command(subcommand)]
        command: StorageCommand,
    },
}

#[derive(Subcommand)]
enum StorageCommand {
    /// Reclaim space on demand.
    Gc,
}

fn not_implemented(name: &str) -> ! {
    eprintln!("{name} is not implemented yet");
    std::process::exit(2);
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        None => not_implemented("tui"),
        Some(Command::Exec { prompt, json }) => {
            let _ = (prompt, json);
            not_implemented("exec");
        }
        Some(Command::AppServer) => {
            if !FeatureFlag::AppServer.is_enabled() {
                eprintln!("app-server is behind the HIKARI_FEATURE_APP_SERVER flag");
            }
            not_implemented("app-server");
        }
        Some(Command::Resume) => not_implemented("resume"),
        Some(Command::Storage { command }) => match command {
            StorageCommand::Gc => not_implemented("storage gc"),
        },
    }
}
