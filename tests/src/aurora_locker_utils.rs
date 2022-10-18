use crate::aurora_engine_utils::{AuroraEngine, ContractInput};
use aurora_engine_types::types::Address;
use std::path::{Path, PathBuf};
use tokio::{process::Command, sync::Mutex};

/// A lock to prevent multiple tests from compiling the Solidity contracts with different
/// library addresses at the same time.
static FORGE_LOCK: Mutex<()> = Mutex::const_new(());
const AURORA_LOCKER_PATH: &str = "../aurora-locker";

pub async fn deploy_codec_lib(engine: &AuroraEngine) -> anyhow::Result<Address> {
    let codec_data = forge_build(&[], &["out", "Codec.sol", "Codec.json"]).await?;
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
    let utils_data = forge_build(&[], &["out", "Utils.sol", "Utils.json"]).await?;
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
    let aurora_sdk_data = forge_build(
        &[
            format!("src/Codec.sol:Codec:0x{}", codec_lib.encode()),
            format!("src/Utils.sol:Utils:0x{}", utils_lib.encode()),
        ],
        &["out", "AuroraSdk.sol", "AuroraSdk.json"],
    )
    .await?;
    let code_hex = json_lens(&aurora_sdk_data, &["bytecode", "object"], |x| {
        serde_json::Value::as_str(x)
    })
    .ok_or_else(forge_parse_err)?;
    let code_hex = code_hex.strip_prefix("0x").unwrap_or(code_hex);
    let code = hex::decode(code_hex)?;

    let address = engine.deploy_evm_contract(code).await?;
    Ok(address)
}

pub async fn deploy_aurora_sdk_test_contract(
    engine: &AuroraEngine,
    aurora_sdk_lib: Address,
) -> anyhow::Result<Address> {
    let aurora_sdk_test_data = forge_build(
        &[format!(
            "src/AuroraSdk.sol:AuroraSdk:0x{}",
            aurora_sdk_lib.encode()
        )],
        &["out", "AuroraSdk.t.sol", "AuroraSdkTest.json"],
    )
    .await?;
    let code_hex = json_lens(&aurora_sdk_test_data, &["bytecode", "object"], |x| {
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
        let locker_data = forge_build(
            &[
                format!("src/Codec.sol:Codec:0x{}", codec_lib.encode()),
                format!("src/AuroraSdk.sol:AuroraSdk:0x{}", aurora_sdk_lib.encode()),
            ],
            &["out", "Locker.sol", "Locker.json"],
        )
        .await?;
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

async fn forge_build(
    libraries: &[String],
    contract_output_path: &[&str],
) -> anyhow::Result<serde_json::Value> {
    let mutex_ref = &FORGE_LOCK;
    let _guard = mutex_ref.lock().await;
    let aurora_locker_path = Path::new(AURORA_LOCKER_PATH);
    let args = std::iter::once("build").chain(libraries.iter().flat_map(|x| ["--libraries", x]));
    let output = Command::new("forge")
        .current_dir(aurora_locker_path)
        .args(args)
        .output()
        .await?;
    crate::process_utils::require_success(output)?;

    let s = tokio::fs::read_to_string(
        aurora_locker_path.join(contract_output_path.iter().collect::<PathBuf>()),
    )
    .await?;
    let result = serde_json::from_str(&s)?;
    Ok(result)
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

    pub fn create_token(&self, token: Address) -> ContractInput {
        let data = self
            .abi
            .function("createToken")
            .unwrap()
            .encode_input(&[ethabi::Token::Address(token.raw())])
            .unwrap();
        ContractInput(data)
    }

    pub fn storage_deposit(
        &self,
        token: Address,
        account_id: &workspaces::AccountId,
    ) -> ContractInput {
        let data = self
            .abi
            .function("storageDeposit")
            .unwrap()
            .encode_input(&[
                ethabi::Token::Address(token.raw()),
                ethabi::Token::String(account_id.as_str().into()),
            ])
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
