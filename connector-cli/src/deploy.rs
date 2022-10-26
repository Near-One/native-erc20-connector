use crate::{
    config::Config,
    log::{EventKind, Log, NearTransactionKind},
    near_rpc_ext::{self, client_like::ClientLike},
};
use near_primitives::{transaction, views::AccessKeyPermissionView};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::process::Command;

/// Path to the factory wasm artifact relative to the repository root.
const FACTORY_WASM_PATH: &str = "target/wasm32-unknown-unknown/release/near_token_factory.wasm";

pub async fn deploy<C: ClientLike>(
    config: &Config,
    near: Arc<C>,
    key: &near_crypto::KeyFile,
    log: &mut Log,
) -> anyhow::Result<()> {
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

    // Ensure no files were modified in the build process (unless config allows it)
    if !config.allow_changed_files {
        let output = Command::new("git").arg("diff").output().await?;
        if !output.status.success() {
            return Err(anyhow::Error::msg(format!(
                "`git diff` failed: {:?} {:?}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            )));
        }
        if !output.stderr.is_empty() || !output.stdout.is_empty() {
            return Err(anyhow::Error::msg(
                "Deploy aborted due to changed files in git repository",
            ));
        }
    }

    // Submit multiple RPC requests for data we need for deployment in parallel
    let maybe_key_info =
        near_rpc_ext::query_access_key(config.factory_account_id.clone(), key.public_key.clone())
            .spawn(near.clone());
    let maybe_preexisting_contract =
        near_rpc_ext::query_code(config.factory_account_id.clone()).spawn(near.clone());

    // Wait for both requests to finish before checking the results
    let maybe_key_info = maybe_key_info.await?;
    let maybe_preexisting_contract = maybe_preexisting_contract.await?;

    // Confirm we have the access key for the factory account (and get the nonce at the same time)
    let key_info = maybe_key_info?;
    let nonce = if let AccessKeyPermissionView::FullAccess = key_info.data.permission {
        key_info.data.nonce
    } else {
        return Err(anyhow::Error::msg("FullAccess key required for deployment"));
    };

    // Confirm there is no code already deployed to the account (unless config allows it)
    let preexisting_contract = maybe_preexisting_contract?.data;
    if !config.allow_deploy_overwrite && !preexisting_contract.code.is_empty() {
        return Err(anyhow::Error::msg(
            "Deploy aborted due to a contract already deployed to factory account ID",
        ));
    }

    // Deploy factory contract to NEAR
    let factory_code = tokio::fs::read(repository_root.join(FACTORY_WASM_PATH)).await?;
    let factory_deploy_tx = transaction::Transaction {
        signer_id: config.factory_account_id.clone(),
        public_key: key.public_key.clone(),
        nonce,
        receiver_id: config.factory_account_id.clone(),
        block_hash: key_info.block_hash,
        actions: vec![transaction::Action::DeployContract(
            transaction::DeployContractAction { code: factory_code },
        )],
    };
    let signer = near_crypto::InMemorySigner::from_secret_key(
        config.factory_account_id.clone(),
        key.secret_key.clone(),
    );
    let tx_hash = near_rpc_ext::broadcast_tx_async(factory_deploy_tx.sign(&signer))
        .spawn(near.clone())
        .await??;
    log.push(EventKind::NearTransactionSubmitted { hash: tx_hash });

    // Confirm deploy success and push events to the log
    let tx_status =
        near_rpc_ext::wait_tx_executed(config.factory_account_id.clone(), tx_hash, near.as_ref())
            .await?;
    match tx_status {
        Ok(_) => {
            let contract_info = near_rpc_ext::query_code(config.factory_account_id.clone())
                .execute(near.as_ref())
                .await?
                .data;
            let previous_code_hash = if preexisting_contract.code.is_empty() {
                None
            } else {
                Some(preexisting_contract.hash)
            };
            log.push(EventKind::NearTransactionSuccessful {
                hash: tx_hash,
                kind: NearTransactionKind::DeployCode {
                    account_id: config.factory_account_id.clone(),
                    new_code_hash: contract_info.hash,
                    previous_code_hash,
                },
            });
        }
        Err(e) => {
            let error_message = format!("Deploy transaction failed: {:?}", e);
            log.push(EventKind::NearTransactionFailed {
                hash: tx_hash,
                error: e,
            });
            return Err(anyhow::Error::msg(error_message));
        }
    }

    // TODO: deploy locker contract to Aurora
    // TODO: initialize the factory contract

    Ok(())
}

/// Spawning a task allows running multiple `make` commands in parallel.
fn make(
    command: &'static str,
    repository_root: &PathBuf,
    log: &mut Log,
) -> anyhow::Result<tokio::task::JoinHandle<anyhow::Result<()>>> {
    let child = Command::new("make")
        .current_dir(repository_root)
        .arg(command)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    log.push(EventKind::Make {
        command: command.into(),
    });
    let task = tokio::task::spawn(async move {
        let output = child.wait_with_output().await?;
        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow::Error::msg(format!(
                "Error: command `make {}` failed. stderr={:?} stdout={:?}",
                command,
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout),
            )))
        }
    });
    Ok(task)
}
