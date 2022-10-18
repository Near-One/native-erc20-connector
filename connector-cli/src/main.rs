use clap::Parser;

mod cli;
mod config;
mod deploy;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();

    let config_path = args.config_path.as_deref().unwrap_or("config.json");
    let config = if let cli::Command::InitConfig = args.command {
        let config = config::Config::testnet();
        config.write_file(config_path).await?;
        config
    } else {
        config::Config::from_file(config_path).await?
    };

    match args.command {
        cli::Command::Deploy => deploy::deploy(&config).await?,
        cli::Command::InitConfig => (),
    }

    Ok(())
}
