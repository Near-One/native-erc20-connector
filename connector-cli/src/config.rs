use near_account_id::AccountId;
use near_crypto::KeyFile;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// URL for the RPC to interact with the NEAR network.
    pub near_rpc_url: String,
    /// URL for the RPC to interact with the Aurora Engine. Must be given if `use_aurora_rpc` is true.
    pub aurora_rpc_url: Option<String>,
    /// AccountId where `near-token-factory` is deployed (or will be deployed).
    pub factory_account_id: AccountId,
    /// Aurora Address where `aurora-locker` is deployed,
    /// or `null` if the locker is not yet deployed.
    pub locker_address: Option<near_token_common::types::Address>,
    /// Path to file where logs are written
    pub log_path: String,
    /// Credentials used for signing transactions. Must be given if executing transactions
    /// that change the state (e.g. `deploy`).
    pub signing: Option<Signing>,
    /// Path to the `native-erc20-connector` repository.
    /// If `null`, then it is assumed to be the current directory.
    pub repository_root: Option<String>,
    /// If `true` then use the Aurora RPC to interact with the Aurora Engine, otherwise the
    /// Engine's `call` method will be used from the signer for `factory_account_id`.
    pub use_aurora_rpc: bool,
    /// If `true` then allow the `deploy` command to continue even if there are modified
    /// files in the repository.
    pub allow_changed_files: bool,
    /// If `true` then allow the `deploy` command to overwrite code already deployed
    /// to the factory account. Otherwise, `deploy` requires the factory account
    /// to not have any code already (but it must already exist).
    pub allow_deploy_overwrite: bool,
}

impl Config {
    pub async fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        let result = serde_json::from_slice(&bytes)?;
        Ok(result)
    }

    pub async fn write_file<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let serialized = serde_json::to_string_pretty(&self)?;
        tokio::fs::write(path, serialized.into_bytes()).await?;
        Ok(())
    }

    pub fn get_near_key(&self) -> anyhow::Result<KeyFile> {
        let key_path = self
            .signing
            .as_ref()
            .ok_or_else(|| anyhow::Error::msg("Missing signing config"))?
            .near_key_path
            .as_str();
        let key = KeyFile::from_file(Path::new(key_path))?;
        Ok(key)
    }

    pub fn testnet() -> Self {
        Self {
            near_rpc_url: "https://archival-rpc.testnet.near.org/".into(),
            aurora_rpc_url: None,
            factory_account_id: "factory.testnet".parse().unwrap(),
            locker_address: None,
            log_path: "connector_cli.log".into(),
            signing: None,
            repository_root: None,
            use_aurora_rpc: false,
            allow_changed_files: false,
            allow_deploy_overwrite: false,
        }
    }
}

// TODO: ledger support?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signing {
    /// Path to a file containing a NEAR signing key, in the usual JSON format
    /// (see https://docs.rs/near-crypto/0.15.0/near_crypto/struct.KeyFile.html).
    pub near_key_path: String,
    /// Path to a file containing a hex-encoded secp256k1 private key (32-bytes).
    /// Must be provided if using Aurora RPC as opposed to NEAR RPC to interact
    /// with the Aurora Engine.
    pub aurora_key_path: Option<String>,
}
