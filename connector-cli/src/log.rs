use aurora_engine::parameters::{SubmitResult, TransactionStatus};
use aurora_engine_types::types::Address;
use near_account_id::AccountId;
use near_primitives::{errors::TxExecutionError, hash::CryptoHash};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub type DateTime = chrono::DateTime<chrono::Utc>;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Log {
    pub events: Vec<Event>,
}

impl Log {
    pub fn push(&mut self, kind: EventKind) {
        let timestamp = chrono::Utc::now();
        self.events.push(Event { timestamp, kind })
    }

    pub async fn append_to_file<P: AsRef<Path>>(self, path: P) -> anyhow::Result<()> {
        let mut writer = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        for event in self.events {
            let serialized = serde_json::to_string(&event)?;
            writer.write_all(serialized.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: DateTime,
    pub kind: EventKind,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventKind {
    Make {
        command: String,
    },
    InitConfig {
        new_config: crate::config::Config,
    },
    ModifyConfigLockerAddress {
        old_value: Option<near_token_common::Address>,
        new_value: Option<near_token_common::Address>,
    },
    NearTransactionSubmitted {
        hash: CryptoHash,
    },
    NearTransactionSuccessful {
        hash: CryptoHash,
        kind: NearTransactionKind,
    },
    NearTransactionFailed {
        hash: CryptoHash,
        error: TxExecutionError,
    },
    AuroraTransactionSuccessful {
        near_hash: Option<CryptoHash>,
        aurora_hash: Option<CryptoHash>,
        kind: AuroraTransactionKind,
    },
    AuroraTransactionFailed {
        near_hash: Option<CryptoHash>,
        aurora_hash: Option<CryptoHash>,
        error: AuroraTransactionError,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuroraTransactionError {
    Revert {
        #[serde(with = "serde_hex")]
        bytes: Vec<u8>,
    },
    OutOfGas,
    OutOfFund,
    OutOfOffset,
    CallTooDeep,
}

impl AuroraTransactionError {
    pub fn from_status(status: TransactionStatus) -> Option<Self> {
        match status {
            TransactionStatus::Succeed(_) => None,
            TransactionStatus::Revert(bytes) => Some(Self::Revert { bytes }),
            TransactionStatus::OutOfGas => Some(Self::OutOfGas),
            TransactionStatus::OutOfFund => Some(Self::OutOfFund),
            TransactionStatus::OutOfOffset => Some(Self::OutOfOffset),
            TransactionStatus::CallTooDeep => Some(Self::CallTooDeep),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum NearTransactionKind {
    DeployCode {
        account_id: AccountId,
        new_code_hash: CryptoHash,
        previous_code_hash: Option<CryptoHash>,
    },
    FunctionCall {
        account_id: AccountId,
        method: String,
        args: String,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuroraTransactionKind {
    DeployContract {
        address: Address,
    },
    ContractCall {
        address: Address,
        result: SubmitResult,
    },
}

mod serde_hex {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(input: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(input))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let no_prefix = s.strip_prefix("0x").unwrap_or(&s);
        let bytes = hex::decode(no_prefix).map_err(Error::custom)?;
        Ok(bytes)
    }
}
