//! The `hikari` binary: argument parsing and dispatch only.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use hikari_exec::ExecArgs;
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
        /// Task prompt, or `-` to read it from stdin.
        prompt: String,
        /// Emit the event stream as JSON Lines.
        #[arg(long)]
        json: bool,
        /// Use DIR as the workspace root instead of the current directory.
        #[arg(short = 'C', long = "cd", value_name = "DIR")]
        cd: Option<PathBuf>,
        /// Allow writes and shell commands: runs without a sandbox with the user's full permissions.
        #[arg(long)]
        full_auto: bool,
        /// Provider name from config.toml.
        #[arg(long)]
        provider: Option<String>,
        /// Model to use (overrides config).
        #[arg(long)]
        model: Option<String>,
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

fn exec(args: ExecArgs) -> ExitCode {
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to start runtime: {e}");
            return ExitCode::from(1);
        }
    };
    runtime.block_on(hikari_exec::run(args))
}

fn main() -> ExitCode {
    match Cli::parse().command {
        None => not_implemented("tui"),
        Some(Command::Exec {
            prompt,
            json,
            cd,
            full_auto,
            provider,
            model,
        }) => {
            let prompt = if prompt == "-" {
                let mut buf = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
                    eprintln!("error: failed to read prompt from stdin: {e}");
                    return ExitCode::from(1);
                }
                buf
            } else {
                prompt
            };
            let workspace = match cd {
                Some(dir) => dir,
                None => match std::env::current_dir() {
                    Ok(dir) => dir,
                    Err(e) => {
                        eprintln!("error: cannot determine current directory: {e}");
                        return ExitCode::from(1);
                    }
                },
            };
            exec(ExecArgs {
                prompt,
                workspace,
                json,
                full_auto,
                provider,
                model,
            })
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
