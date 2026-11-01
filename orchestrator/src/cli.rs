//! CLI argument parsing. Override config file values.

use std::path::PathBuf;

use clap::Parser;

use crate::config::LogFormat;

#[derive(Debug, Parser)]
#[command(name = "orchestrator", version, about = "Unified core engine")]
pub struct Cli {
    /// Path to TOML config file. Uses defaults if missing.
    #[arg(short, long, default_value = "orchestrator.toml")]
    pub config: PathBuf,

    /// Override modules directory.
    #[arg(long)]
    pub modules_dir: Option<PathBuf>,

    /// Override log level (e.g. info, debug, warn).
    #[arg(long)]
    pub log_level: Option<String>,

    /// Override log format.
    #[arg(long, value_enum)]
    pub log_format: Option<LogFormat>,

    /// Validate config and exit without starting.
    #[arg(long)]
    pub dry_run: bool,
}
