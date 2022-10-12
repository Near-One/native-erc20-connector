use crate::aurora_engine_utils::{AuroraEngine, ContractInput};
use aurora_engine_types::types::Address;
use std::path::{Path, PathBuf};
use tokio::process::Command;

const AURORA_LOCKER_PATH: &str = "../aurora-locker";

pub async fn deploy_codec_lib(engine: &AuroraEngine) -> anyhow::Result<Address> {
    let aurora_locker_path = Path::new(AURORA_LOCKER_PATH);
    let output = Command::new("forge")
        .current_dir(aurora_locker_path)
        .arg("build")
        .output()
        .await?;
    crate::process_utils::require_success(output)?;
    let codec_data: serde_json::Value = {
        let s = tokio::fs::read_to_string(
            aurora_locker_path.join(
                ["out", "Codec.sol", "Codec.json"]
                    .iter()
                    .collect::<PathBuf>(),
            ),
        )
        .await?;
        serde_json::from_str(&s)?
    };
    let code_hex = json_lens(&codec_data, &["bytecode", "object"], |x| {
        serde_json::Value::as_str(x)
    })
    .ok_or_else(forge_parse_err)?;
    let code_hex = code_hex.strip_prefix("0x").unwrap_or(code_hex);
    let code = hex::decode(code_hex)?;

    let address = engine.deploy_evm_contract(code).await?;
    Ok(address)
}

pub async fn deploy_utils_lib(engine: &AuroraEngine) -> anyhow::Result<Address> {
    let aurora_locker_path = Path::new(AURORA_LOCKER_PATH);
    let output = Command::new("forge")
        .current_dir(aurora_locker_path)
        .arg("build")
        .output()
        .await?;
    crate::process_utils::require_success(output)?;
    let utils_data: serde_json::Value = {
        let s = tokio::fs::read_to_string(
            aurora_locker_path.join(
                ["out", "Utils.sol", "Utils.json"]
                    .iter()
                    .collect::<PathBuf>(),
            ),
        )
        .await?;
        serde_json::from_str(&s)?
    };
    let code_hex = json_lens(&utils_data, &["bytecode", "object"], |x| {
        serde_json::Value::as_str(x)
    })
    .ok_or_else(forge_parse_err)?;
    let code_hex = code_hex.strip_prefix("0x").unwrap_or(code_hex);
    let code = hex::decode(code_hex)?;

    let address = engine.deploy_evm_contract(code).await?;
    Ok(address)
}

pub async fn deploy_aurora_sdk_lib(
    engine: &AuroraEngine,
    codec_lib: Address,
    utils_lib: Address,
) -> anyhow::Result<Address> {
    let aurora_locker_path = Path::new(AURORA_LOCKER_PATH);
    let output = Command::new("forge")
        .current_dir(aurora_locker_path)
        .args([
            "build",
            "--libraries",
            format!("src/Codec.sol:Codec:0x{}", codec_lib.encode()).as_str(),
            "--libraries",
            format!("src/Utils.sol:Utils:0x{}", utils_lib.encode()).as_str(),
        ])
        .output()
        .await?;
    crate::process_utils::require_success(output)?;
    let aurora_sdk_data: serde_json::Value = {
        let s = tokio::fs::read_to_string(
            aurora_locker_path.join(
                ["out", "AuroraSdk.sol", "AuroraSdk.json"]
                    .iter()
                    .collect::<PathBuf>(),
            ),
        )
        .await?;
        serde_json::from_str(&s)?
    };
    let code_hex = json_lens(&aurora_sdk_data, &["bytecode", "object"], |x| {
        serde_json::Value::as_str(x)
    })
    .ok_or_else(forge_parse_err)?;
    let code_hex = code_hex.strip_prefix("0x").unwrap_or(code_hex);
    let code = hex::decode(code_hex)?;

    let address = engine.deploy_evm_contract(code).await?;
    Ok(address)
}

pub async fn create_locker_constructor(engine: &AuroraEngine) -> anyhow::Result<Constructor> {
    let codec_lib = deploy_codec_lib(engine).await?;
    let utils_lib = deploy_utils_lib(engine).await?;
    let aurora_sdk_lib = deploy_aurora_sdk_lib(engine, codec_lib, utils_lib).await?;
    let constructor = Constructor::load(aurora_sdk_lib, codec_lib).await?;
    Ok(constructor)
}

pub struct Constructor {
    pub code: Vec<u8>,
    pub abi: ethabi::Contract,
}

impl Constructor {
    pub async fn load(aurora_sdk_lib: Address, codec_lib: Address) -> anyhow::Result<Self> {
        let aurora_locker_path = Path::new(AURORA_LOCKER_PATH);
        let output = Command::new("forge")
            .current_dir(aurora_locker_path)
            .args([
                "build",
                "--libraries",
                format!("src/Codec.sol:Codec:0x{}", codec_lib.encode()).as_str(),
                "--libraries",
                format!("src/AuroraSdk.sol:AuroraSdk:0x{}", aurora_sdk_lib.encode()).as_str(),
            ])
            .output()
            .await?;
        crate::process_utils::require_success(output)?;
        let locker_data: serde_json::Value = {
            let s = tokio::fs::read_to_string(
                aurora_locker_path.join(
                    ["out", "Locker.sol", "Locker.json"]
                        .iter()
                        .collect::<PathBuf>(),
                ),
            )
            .await?;
            serde_json::from_str(&s)?
        };
        let code_hex = json_lens(&locker_data, &["bytecode", "object"], |x| {
            serde_json::Value::as_str(x)
        })
        .ok_or_else(forge_parse_err)?;
        let code_hex = code_hex.strip_prefix("0x").unwrap_or(code_hex);
        let code = hex::decode(code_hex)?;
        let abi_data = json_lens(&locker_data, &["abi"], Some).ok_or_else(forge_parse_err)?;
        let abi = serde_json::from_value(abi_data.clone())?;
        Ok(Self { code, abi })
    }

    pub fn deploy_code(&self, factory: &workspaces::AccountId, wnear: Address) -> Vec<u8> {
        // Unwraps are safe because we statically know there is a constructor and it
        // takes two strings as input.
        self.abi
            .constructor()
            .unwrap()
            .encode_input(
                self.code.clone(),
                &[
                    ethabi::Token::String(factory.as_str().into()),
                    ethabi::Token::Address(wnear.raw()),
                ],
            )
            .unwrap()
    }
}

fn json_lens<'a, T, F>(value: &'a serde_json::Value, keys: &[&str], interp: F) -> Option<T>
where
    F: FnOnce(&'a serde_json::Value) -> Option<T>,
{
    let mut value = value;
    for k in keys {
        value = value.as_object()?.get(*k)?;
    }
    interp(value)
}

fn forge_parse_err() -> anyhow::Error {
    anyhow::Error::msg("Failed to parse Forge output")
}

pub struct AuroraLocker {
    pub address: Address,
    pub abi: ethabi::Contract,
}

impl AuroraLocker {
    pub fn deposit(
        &self,
        token: Address,
        recipient: &workspaces::AccountId,
        amount: u128,
    ) -> ContractInput {
        let data = self
            .abi
            .function("deposit")
            .unwrap()
            .encode_input(&[
                ethabi::Token::Address(token.raw()),
                ethabi::Token::String(recipient.as_str().into()),
                ethabi::Token::Uint(amount.into()),
            ])
            .unwrap();
        ContractInput(data)
    }

    pub fn init_near_account(&self) -> ContractInput {
        let data = self
            .abi
            .function("initNearAccount")
            .unwrap()
            .encode_input(&[])
            .unwrap();
        ContractInput(data)
    }
}

pub trait LockerDeployedAt {
    fn deployed_at(self, address: Address) -> AuroraLocker;
}

impl LockerDeployedAt for Constructor {
    fn deployed_at(self, address: Address) -> AuroraLocker {
        AuroraLocker {
            abi: self.abi,
            address,
        }
    }
}
