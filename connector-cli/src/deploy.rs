use crate::{
    config::Config,
    log::{AuroraTransactionKind, EventKind, Log, NearTransactionKind},
    near_rpc_ext::{self, client_like::ClientLike, MAX_NEAR_GAS},
};
use aurora_engine::parameters::TransactionStatus;
use aurora_engine_types::types::Address;
use near_primitives::{hash::CryptoHash, transaction, views::AccessKeyPermissionView};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::process::Command;

/// Path to the factory wasm artifact relative to the repository root.
const FACTORY_WASM_PATH: &str = "target/wasm32-unknown-unknown/release/near_token_factory.wasm";
const TOKEN_WASM_PATH: &str = "target/wasm32-unknown-unknown/release/near_token_contract.wasm";

pub async fn deploy<C: ClientLike>(
    config: &mut Config,
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
        key_info.data.nonce + 1
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

    // Deploy libraries needed for Locker
    let codec_bytes = read_contract_bytes(
        &repository_root,
        &["aurora-locker", "out", "Codec.sol", "Codec.json"],
    )
    .await?;
    let codec_address = deploy_evm_contract(
        codec_bytes,
        config,
        near.clone(),
        &signer,
        nonce + 1,
        key_info.block_hash,
        log,
    )
    .await?;

    let utils_bytes = read_contract_bytes(
        &repository_root,
        &["aurora-locker", "out", "Utils.sol", "Utils.json"],
    )
    .await?;
    let utils_address = deploy_evm_contract(
        utils_bytes,
        config,
        near.clone(),
        &signer,
        nonce + 2,
        key_info.block_hash,
        log,
    )
    .await?;

    let sdk_make_cmd = format!(
        "CODEC=0x{} UTILS=0x{} aurora-locker-sdk",
        codec_address.encode(),
        utils_address.encode()
    );
    make(sdk_make_cmd, &repository_root, log)?.await??;

    let aurora_sdk_bytes = read_contract_bytes(
        &repository_root,
        &["aurora-locker", "out", "AuroraSdk.sol", "AuroraSdk.json"],
    )
    .await?;
    let aurora_sdk_address = deploy_evm_contract(
        aurora_sdk_bytes,
        config,
        near.clone(),
        &signer,
        nonce + 3,
        key_info.block_hash,
        log,
    )
    .await?;

    // Build and deploy Locker
    let locker_build_command = format!(
        "CODEC=0x{} SDK=0x{} aurora-locker-with-libs",
        codec_address.encode(),
        aurora_sdk_address.encode()
    );
    make(locker_build_command, &repository_root, log)?.await??;

    let locker_bytes = {
        let artifact_path = create_forge_artifact_path(
            &repository_root,
            &["aurora-locker", "out", "Locker.sol", "Locker.json"],
        );
        let artifact = read_forge_artifact(&artifact_path).await?;
        let code = parse_contract_bytes(&artifact, &artifact_path)?;
        let abi = parse_contract_abi(&artifact, &artifact_path)?;
        let constructor = abi
            .constructor()
            .ok_or_else(|| anyhow::Error::msg("Expected constructor for Locker contract"))?;
        constructor
            .encode_input(
                code,
                &[
                    ethabi::Token::String(config.factory_account_id.as_str().into()),
                    ethabi::Token::Address(config.wnear_address.0.into()),
                ],
            )
            .map_err(|e| {
                anyhow::Error::msg(format!(
                    "Failed to encode arguments for Locker constructor: {:?}",
                    e
                ))
            })?
    };
    let locker_address = deploy_evm_contract(
        locker_bytes,
        config,
        near.clone(),
        &signer,
        nonce + 4,
        key_info.block_hash,
        log,
    )
    .await?;

    // Update the config with the new locker address
    let config_locker_address = Some(near_token_common::Address(locker_address.raw().0));
    log.push(EventKind::ModifyConfigLockerAddress {
        old_value: config.locker_address.clone(),
        new_value: config_locker_address.clone(),
    });
    config.locker_address = config_locker_address;

    // Initialize Factory contract now that we know the Locker address
    let args = serde_json::json!({
        "locker": locker_address.encode(),
        "aurora": config.aurora_account_id.as_str(),
    });
    factory_function_call(
        "new",
        args,
        nonce + 5,
        key_info.block_hash,
        config,
        near.clone(),
        &signer,
        log,
    )
    .await?;

    // Set the token binary in the Factory
    let token_code = tokio::fs::read(repository_root.join(TOKEN_WASM_PATH)).await?;
    let args = serde_json::json!({
        "binary": base64::encode(token_code),
    });
    factory_function_call(
        "set_token_binary",
        args,
        nonce + 6,
        key_info.block_hash,
        config,
        near,
        &signer,
        log,
    )
    .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn factory_function_call<C: ClientLike>(
    method: &str,
    args: serde_json::Value,
    nonce: u64,
    block_hash: CryptoHash,
    config: &Config,
    near: Arc<C>,
    signer: &near_crypto::InMemorySigner,
    log: &mut Log,
) -> anyhow::Result<()> {
    let tx = transaction::Transaction {
        signer_id: config.factory_account_id.clone(),
        public_key: signer.public_key.clone(),
        nonce,
        receiver_id: config.factory_account_id.clone(),
        block_hash,
        actions: vec![transaction::Action::FunctionCall(
            transaction::FunctionCallAction {
                method_name: method.into(),
                args: serde_json::to_vec(&args)?,
                gas: MAX_NEAR_GAS,
                deposit: 0,
            },
        )],
    };

    let tx_hash = near_rpc_ext::broadcast_tx_async(tx.sign(signer))
        .spawn(near.clone())
        .await??;
    log.push(EventKind::NearTransactionSubmitted { hash: tx_hash });

    let tx_status =
        near_rpc_ext::wait_tx_executed(config.factory_account_id.clone(), tx_hash, near.as_ref())
            .await?;

    match tx_status {
        Ok(_) => {
            log.push(EventKind::NearTransactionSuccessful {
                hash: tx_hash,
                kind: NearTransactionKind::FunctionCall {
                    account_id: config.factory_account_id.clone(),
                    method: method.into(),
                    args: serde_json::to_string(&args)?,
                },
            });
            Ok(())
        }
        Err(e) => {
            let error_message = format!("Factory `{}` transaction failed: {:?}", method, e);
            log.push(EventKind::NearTransactionFailed {
                hash: tx_hash,
                error: e,
            });
            Err(anyhow::Error::msg(error_message))
        }
    }
}

async fn read_contract_bytes(
    repository_root: &Path,
    contract_output_path: &[&str],
) -> anyhow::Result<Vec<u8>> {
    let artifact_path = create_forge_artifact_path(repository_root, contract_output_path);
    let artifact = read_forge_artifact(&artifact_path).await?;
    parse_contract_bytes(&artifact, &artifact_path)
}

fn parse_contract_bytes(
    artifact: &serde_json::Value,
    artifact_path: &Path,
) -> anyhow::Result<Vec<u8>> {
    let code_hex = artifact
        .as_object()
        .and_then(|x| x.get("bytecode"))
        .and_then(|x| x.as_object())
        .and_then(|x| x.get("object"))
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::Error::msg("Failed to parse forge output"))?;
    let code_hex = code_hex.strip_prefix("0x").unwrap_or(code_hex);
    let code = hex::decode(code_hex).map_err(|e| {
        anyhow::Error::msg(format!(
            "Failed to parse compiled bytecode for {:?}: {:?}",
            artifact_path, e
        ))
    })?;
    Ok(code)
}

fn parse_contract_abi(
    artifact: &serde_json::Value,
    artifact_path: &Path,
) -> anyhow::Result<ethabi::Contract> {
    let abi = artifact
        .as_object()
        .and_then(|x| x.get("abi"))
        .ok_or_else(|| anyhow::Error::msg("Failed to parse forge output"))?;
    let abi = serde_json::from_value(abi.clone()).map_err(|e| {
        anyhow::Error::msg(format!(
            "Failed to parse ABI for {:?}: {:?}",
            artifact_path, e
        ))
    })?;
    Ok(abi)
}

fn create_forge_artifact_path(repository_root: &Path, contract_output_path: &[&str]) -> PathBuf {
    repository_root.join(contract_output_path.iter().collect::<PathBuf>())
}

async fn read_forge_artifact(artifact_path: &Path) -> anyhow::Result<serde_json::Value> {
    let s = tokio::fs::read_to_string(&artifact_path).await?;
    let artifact: serde_json::Value = serde_json::from_str(&s).map_err(|e| {
        anyhow::Error::msg(format!(
            "Failed to read forge artifact {:?}: {:?}",
            artifact_path, e
        ))
    })?;
    Ok(artifact)
}

async fn deploy_evm_contract<C: ClientLike>(
    contract_bytes: Vec<u8>,
    config: &Config,
    near: Arc<C>,
    signer: &near_crypto::InMemorySigner,
    nonce: u64,
    block_hash: CryptoHash,
    log: &mut Log,
) -> anyhow::Result<Address> {
    if config.use_aurora_rpc {
        return Err(anyhow::Error::msg("Aurora RPC not yet supported"));
    }

    let deploy_tx = transaction::Transaction {
        signer_id: config.factory_account_id.clone(),
        public_key: signer.public_key.clone(),
        nonce,
        receiver_id: config.aurora_account_id.clone(),
        block_hash,
        actions: vec![transaction::Action::FunctionCall(
            transaction::FunctionCallAction {
                method_name: "deploy_code".into(),
                args: contract_bytes,
                gas: MAX_NEAR_GAS,
                deposit: 0,
            },
        )],
    };
    let tx_hash = near_rpc_ext::broadcast_tx_async(deploy_tx.sign(signer))
        .spawn(near.clone())
        .await??;
    log.push(EventKind::NearTransactionSubmitted { hash: tx_hash });

    let tx_status =
        near_rpc_ext::wait_tx_executed(config.factory_account_id.clone(), tx_hash, near.as_ref())
            .await?;
    match tx_status {
        Ok(return_bytes) => {
            let result = near_rpc_ext::aurora_engine_utils::assume_successful_submit_result(
                tx_hash,
                &return_bytes,
                log,
            )?;
            match result.status {
                TransactionStatus::Succeed(address_bytes) => {
                    let address = Address::try_from_slice(&address_bytes).map_err(|e| {
                        anyhow::Error::msg(format!(
                            "Failed to get Address from Engine result: {:?}",
                            e
                        ))
                    })?;
                    log.push(EventKind::AuroraTransactionSuccessful {
                        near_hash: Some(tx_hash),
                        aurora_hash: None,
                        kind: AuroraTransactionKind::DeployContract { address },
                    });
                    Ok(address)
                }
                _ => unreachable!(), // `assume_successful_submit_result` would error out if not `TransactionStatus::Succeed`
            }
        }
        Err(e) => {
            let error_message = format!("Deploy Aurora EVM contract failed: {:?}", e);
            log.push(EventKind::NearTransactionFailed {
                hash: tx_hash,
                error: e,
            });
            Err(anyhow::Error::msg(error_message))
        }
    }
}

/// Spawning a task allows running multiple `make` commands in parallel.
fn make<T>(
    command: T,
    repository_root: &Path,
    log: &mut Log,
) -> anyhow::Result<tokio::task::JoinHandle<anyhow::Result<()>>>
where
    T: std::fmt::Display + AsRef<str> + Send + Sync + 'static,
{
    let args = command.as_ref().split(' ');
    let child = Command::new("make")
        .current_dir(repository_root)
        .args(args)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    log.push(EventKind::Make {
        command: command.as_ref().into(),
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
