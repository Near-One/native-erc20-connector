use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
pub struct Args {
    /// Path to the config file. By default, assumes `config.json` in the current directory.
    #[arg(short, long)]
    pub config_path: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Write a default config to `config_path`
    InitConfig,
    Deploy,
}
