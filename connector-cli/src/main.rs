use aurora_engine_types::types::Address;
use clap::Parser;
use std::sync::Arc;

mod cli;
mod config;
mod create_token;
mod deploy;
mod log;
mod near_rpc_ext;
#[cfg(test)]
mod tests;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();

    let mut log = log::Log::default();
    let config_path = args.config_path.as_deref().unwrap_or("config.json");
    let mut config = if let cli::Command::InitConfig = args.command {
        let config = config::Config::testnet();
        config.write_file(config_path).await?;
        log.push(crate::log::EventKind::InitConfig {
            new_config: config.clone(),
        });
        config
    } else {
        config::Config::from_file(config_path).await?
    };

    let result = handle_command(args.command, &mut config, &mut log).await;

    // Always write the logs, independent of whether the command completed successfully
    log.append_to_file(&config.log_path)
        .await
        .map_err(|e| anyhow::Error::msg(format!("Failed to write logs: {:?}", e)))?;

    // If the command was successful then re-write the config (in case any changes were made)
    if let Ok(()) = result {
        config.write_file(config_path).await?;
    }

    result?;

    Ok(())
}

async fn handle_command(
    command: cli::Command,
    config: &mut crate::config::Config,
    log: &mut log::Log,
) -> anyhow::Result<()> {
    match command {
        cli::Command::Deploy => {
            let near = Arc::new(near_jsonrpc_client::JsonRpcClient::connect(
                &config.near_rpc_url,
            ));
            let key = config.get_near_key()?;
            deploy::deploy(config, near, &key, log).await?
        }
        cli::Command::CreateToken { address } => {
            let parsed_address = Address::decode(address.strip_prefix("0x").unwrap_or(&address))
                .map_err(|e| {
                    anyhow::Error::msg(format!("Invalid address {} provided: {:?}", address, e))
                })?;
            let near = Arc::new(near_jsonrpc_client::JsonRpcClient::connect(
                &config.near_rpc_url,
            ));
            let key = config.get_near_key()?;
            create_token::create_token(parsed_address, config, near, &key, log).await?
        }
        cli::Command::InitConfig => (),
    }

    Ok(())
}
