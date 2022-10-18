use clap::Parser;

mod cli;
mod config;
mod deploy;
mod log;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();

    let mut log = log::Log::default();
    let config_path = args.config_path.as_deref().unwrap_or("config.json");
    let config = if let cli::Command::InitConfig = args.command {
        let config = config::Config::testnet();
        config.write_file(config_path).await?;
        log.push(crate::log::EventKind::InitConfig {
            new_config: config.clone(),
        });
        config
    } else {
        config::Config::from_file(config_path).await?
    };

    let result = handle_command(args.command, &config, &mut log).await;

    // Always write the logs, independent of whether the command completed successfully
    log.append_to_file(&config.log_path)
        .await
        .map_err(|e| anyhow::Error::msg(format!("Failed to write logs: {:?}", e)))?;

    result?;

    Ok(())
}

async fn handle_command(
    command: cli::Command,
    config: &crate::config::Config,
    log: &mut log::Log,
) -> anyhow::Result<()> {
    match command {
        cli::Command::Deploy => deploy::deploy(config, log).await?,
        cli::Command::InitConfig => (),
    }

    Ok(())
}
