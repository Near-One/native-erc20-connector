use aurora_engine_types::types::Address;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use workspaces::{network::Sandbox, Worker};

const ROOT_PATH: &str = "..";
pub const FACTORY_ACCOUNT_ID: &str = "f.test.near";

pub struct TokenFactory {
    pub inner: workspaces::Contract,
}

impl TokenFactory {
    pub async fn deploy(
        worker: &Worker<Sandbox>,
        locker: Address,
        engine: &workspaces::AccountId,
    ) -> anyhow::Result<Self> {
        // Compile and deploy factory contract
        let wasm = Self::compile_factory().await?;
        // We can't use `dev-deploy` here because then the account ID is
        // too long to create sub-accounts
        let (_, sk) = worker.dev_generate().await;
        let contract = worker
            .create_tla_and_deploy(FACTORY_ACCOUNT_ID.parse().unwrap(), sk, &wasm)
            .await?
            .into_result()?;

        // Initialize factory contract
        contract
            .call("new")
            .args_json(serde_json::json!({
                "locker": locker.encode(),
                "aurora": engine,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Compile token contract
        let wasm = Self::compile_token().await?;

        // Set token binary in factory
        let wasm_base64 = base64::encode(wasm);
        contract
            .call("set_token_binary")
            .args_json(serde_json::json!({
                "binary": wasm_base64,
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        Ok(Self { inner: contract })
    }

    pub async fn compile_factory() -> anyhow::Result<Vec<u8>> {
        let root_path = Path::new(ROOT_PATH);
        add_wasm_target(root_path).await?;
        let output = Command::new("cargo")
            .env("RUSTFLAGS", "-C link-arg=-s")
            .current_dir(root_path)
            .args([
                "build",
                "-p",
                "near-token-factory",
                "--target",
                "wasm32-unknown-unknown",
                "--release",
            ])
            .output()
            .await?;
        let binary_path = root_path.join(
            [
                "target",
                "wasm32-unknown-unknown",
                "release",
                "near_token_factory.wasm",
            ]
            .iter()
            .collect::<PathBuf>(),
        );
        crate::process_utils::require_success(output)?;
        let bytes = tokio::fs::read(binary_path).await?;
        Ok(bytes)
    }

    pub async fn compile_token() -> anyhow::Result<Vec<u8>> {
        let root_path = Path::new(ROOT_PATH);
        add_wasm_target(root_path).await?;
        let output = Command::new("cargo")
            .env("RUSTFLAGS", "-C link-arg=-s")
            .current_dir(root_path)
            .args([
                "build",
                "-p",
                "near-token-contract",
                "--target",
                "wasm32-unknown-unknown",
                "--release",
            ])
            .output()
            .await?;
        let binary_path = root_path.join(
            [
                "target",
                "wasm32-unknown-unknown",
                "release",
                "near_token_contract.wasm",
            ]
            .iter()
            .collect::<PathBuf>(),
        );
        crate::process_utils::require_success(output)?;
        let bytes = tokio::fs::read(binary_path).await?;
        Ok(bytes)
    }
}

async fn add_wasm_target(root_path: &Path) -> anyhow::Result<()> {
    let output = tokio::process::Command::new("rustup")
        .current_dir(root_path)
        .args(["target", "add", "wasm32-unknown-unknown"])
        .output()
        .await?;
    crate::process_utils::require_success(output)?;
    Ok(())
}
