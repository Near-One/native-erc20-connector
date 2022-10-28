use crate::log::{AuroraTransactionError, EventKind, Log};
use aurora_engine::parameters::{SubmitResult, TransactionStatus};
use borsh::BorshDeserialize;
use near_primitives::hash::CryptoHash;

pub fn assume_successful_submit_result(
    tx_hash: CryptoHash,
    near_tx_output: &[u8],
    log: &mut Log,
) -> anyhow::Result<SubmitResult> {
    let result = SubmitResult::try_from_slice(near_tx_output).map_err(|e| {
        anyhow::Error::msg(format!(
            "Failed to parse SubmitResult from Engine return: {:?}",
            e
        ))
    })?;
    match result.status {
        TransactionStatus::Succeed(_) => Ok(result),
        TransactionStatus::Revert(revert_bytes) => {
            let error_message = format!(
                "Deploy Aurora EVM contract reverted with bytes 0x{:?}",
                hex::encode(&revert_bytes)
            );
            log.push(EventKind::AuroraTransactionFailed {
                near_hash: Some(tx_hash),
                aurora_hash: None,
                error: AuroraTransactionError::Revert {
                    bytes: revert_bytes,
                },
            });
            Err(anyhow::Error::msg(error_message))
        }
        other => {
            let error_message = format!("Deploy Aurora EVM contract error: {:?}", other);
            log.push(EventKind::AuroraTransactionFailed {
                near_hash: Some(tx_hash),
                aurora_hash: None,
                error: AuroraTransactionError::from_status(other).unwrap(),
            });
            Err(anyhow::Error::msg(error_message))
        }
    }
}
