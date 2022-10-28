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
    /// Deploy the Locker and Factory contracts on-chain
    Deploy,
    /// Add a new Aurora-native ERC-20 token to the connector. This will create a NEP-141
    /// counterpart of the token on NEAR. This command will fail if the given address is
    /// already registered with the native token connector.
    CreateToken { address: String },
}
