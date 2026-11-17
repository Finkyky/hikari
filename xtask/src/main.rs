// xtask is developer tooling, not shipped code: spawning cargo directly is correct here.
#![allow(clippy::disallowed_types)]
//! Developer automation tasks invoked as `cargo xtask <task>`.

use std::process::Command as ProcCommand;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "Hikari developer automation")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Install developer tools (cargo-nextest, cargo-deny).
    Bootstrap,
    /// Format all code.
    Format,
    /// Full local presubmit: fmt check, clippy, tests, deny.
    Presubmit,
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

fn cargo_cmd(args: &[&str]) -> anyhow::Result<()> {
    let status = ProcCommand::new(cargo())
        .args(args)
        .status()
        .with_context(|| format!("failed to spawn cargo {}", args.join(" ")))?;
    if !status.success() {
        bail!("cargo {} failed", args.join(" "));
    }
    Ok(())
}

fn cargo_tool_installed(subcommand: &str) -> bool {
    ProcCommand::new(cargo())
        .args([subcommand, "--version"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn presubmit() -> anyhow::Result<()> {
    cargo_cmd(&["fmt", "--all", "--check"])?;
    cargo_cmd(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ])?;
    if cargo_tool_installed("nextest") {
        cargo_cmd(&["nextest", "run", "--workspace", "--no-fail-fast"])?;
    } else {
        eprintln!("warning: cargo-nextest not installed; falling back to `cargo test`");
        cargo_cmd(&["test", "--workspace"])?;
    }
    cargo_cmd(&["test", "--doc", "--workspace"])?;
    if cargo_tool_installed("deny") {
        cargo_cmd(&["deny", "check"])?;
    } else {
        eprintln!("warning: cargo-deny not installed; skipping `cargo deny check`");
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Task::Bootstrap => cargo_cmd(&["install", "--locked", "cargo-nextest", "cargo-deny"]),
        Task::Format => cargo_cmd(&["fmt", "--all"]),
        Task::Presubmit => presubmit(),
    }
}
