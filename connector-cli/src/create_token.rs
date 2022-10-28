use crate::{
    config::Config,
    log::{AuroraTransactionKind, EventKind, Log},
    near_rpc_ext::{self, client_like::ClientLike, MAX_NEAR_GAS},
};
use aurora_engine::parameters::{CallArgs, FunctionCallArgsV2};
use aurora_engine_types::types::Address;
use borsh::BorshSerialize;
use near_primitives::transaction;
use std::sync::Arc;

const CREATE_TOKEN_SELECTOR: [u8; 4] = [0, 0, 0, 0];

pub async fn create_token<C: ClientLike>(
    address: Address,
    config: &Config,
    near: Arc<C>,
    key: &near_crypto::KeyFile,
    log: &mut Log,
) -> anyhow::Result<()> {
    if config.use_aurora_rpc {
        return Err(anyhow::Error::msg("Aurora RPC not yet supported"));
    }

    let locker_address = config
        .locker_address
        .as_ref()
        .map(|a| Address::from_array(a.0))
        .ok_or_else(|| anyhow::Error::msg("locker_address must be set in config"))?;
    let key_info =
        near_rpc_ext::query_access_key(config.factory_account_id.clone(), key.public_key.clone())
            .spawn(near.clone())
            .await??;
    let nonce = key_info.data.nonce + 1;
    let block_hash = key_info.block_hash;

    let input = {
        let mut buf = CREATE_TOKEN_SELECTOR.to_vec();
        buf.extend_from_slice(&ethabi::encode(&[ethabi::Token::Address(address.raw())]));
        buf
    };
    let args = CallArgs::V2(FunctionCallArgsV2 {
        contract: locker_address,
        value: [0u8; 32],
        input,
    });

    let tx = transaction::Transaction {
        signer_id: key.account_id.clone(),
        public_key: key.public_key.clone(),
        nonce,
        receiver_id: config.aurora_account_id.clone(),
        block_hash,
        actions: vec![transaction::Action::FunctionCall(
            transaction::FunctionCallAction {
                method_name: "call".into(),
                args: args.try_to_vec()?,
                gas: MAX_NEAR_GAS,
                deposit: 0,
            },
        )],
    };

    let signer = near_crypto::InMemorySigner::from_secret_key(
        key.account_id.clone(),
        key.secret_key.clone(),
    );
    let tx_hash = near_rpc_ext::broadcast_tx_async(tx.sign(&signer))
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
            log.push(EventKind::AuroraTransactionSuccessful {
                near_hash: Some(tx_hash),
                aurora_hash: None,
                kind: AuroraTransactionKind::ContractCall {
                    address: locker_address,
                    result,
                },
            });
        }
        Err(e) => {
            let error_message = format!("create_token transaction failed: {:?}", e);
            log.push(EventKind::NearTransactionFailed {
                hash: tx_hash,
                error: e,
            });
            return Err(anyhow::Error::msg(error_message));
        }
    }

    Ok(())
}
