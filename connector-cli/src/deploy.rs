use crate::{
    config::Config,
    log::{EventKind, Log},
};
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub async fn deploy(config: &Config, log: &mut Log) -> anyhow::Result<()> {
    let repository_root =
        Path::new(config.repository_root.as_deref().unwrap_or(".")).canonicalize()?;

    // `cargo` and `forge` compilations can be run in parallel.
    let compile_factory_task = make("near-token-factory", &repository_root, log)?;
    let compile_locker_task = make("aurora-locker", &repository_root, log)?;
    // `near-token-factory` and `near-token-contract` are both `cargo` builds, so they must
    // run sequentially. Therefore we wait until the factory is done before starting the token.
    let compile_factory_result = compile_factory_task.await?;
    let compile_token_task = make("near-token-contract", &repository_root, log)?;
    let compile_token_result = compile_token_task.await?;
    let compile_locker_result = compile_locker_task.await?;
    // Ensure all compilation was successful
    compile_factory_result?;
    compile_token_result?;
    compile_locker_result?;

    // TODO: deploy compiled artifacts to chain

    Ok(())
}

/// Spawning a task allows running multiple `make` commands in parallel.
fn make(
    command: &'static str,
    repository_root: &PathBuf,
    log: &mut Log,
) -> anyhow::Result<tokio::task::JoinHandle<anyhow::Result<()>>> {
    let mut child = Command::new("make")
        .current_dir(repository_root)
        .arg(command)
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()?;
    log.push(EventKind::Make {
        command: command.into(),
    });
    let task = tokio::task::spawn(async move {
        let output = child.wait().await?;
        if output.success() {
            Ok(())
        } else {
            Err(anyhow::Error::msg(format!(
                "Error: command `make {}` failed.",
                command
            )))
        }
    });
    Ok(task)
}
