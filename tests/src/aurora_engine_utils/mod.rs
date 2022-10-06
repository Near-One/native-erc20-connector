use aurora_engine_types::{
    types::{Address, Wei},
    U256,
};
use workspaces::Contract;

pub mod repo;

const TESTNET_CHAIN_ID: u64 = 1313161555;
const MAX_GAS: u64 = 300_000_000_000_000;

pub struct AuroraEngine {
    inner: Contract,
}

pub async fn deploy_latest() -> anyhow::Result<AuroraEngine> {
    let worker = workspaces::sandbox().await?;
    let wasm = repo::AuroraEngineRepo::download_and_compile_latest().await?;
    let contract = worker.dev_deploy(&wasm).await?;
    let new_args = aurora_engine::parameters::NewCallArgs {
        chain_id: aurora_engine_types::types::u256_to_arr(&TESTNET_CHAIN_ID.into()),
        owner_id: contract.id().as_ref().parse().unwrap(),
        bridge_prover_id: contract.id().as_ref().parse().unwrap(),
        upgrade_delay_blocks: 0,
    };
    contract
        .call("new")
        .args_borsh(new_args)
        .transact()
        .await?
        .into_result()?;
    let init_args = aurora_engine::parameters::InitCallArgs {
        prover_account: contract.id().as_ref().parse().unwrap(),
        eth_custodian_address: "0000000000000000000000000000000000000000".into(),
        metadata: Default::default(),
    };
    contract
        .call("new_eth_connector")
        .args_borsh(init_args)
        .transact()
        .await?
        .into_result()?;
    Ok(AuroraEngine { inner: contract })
}

impl AuroraEngine {
    pub async fn mint_account(
        &self,
        address: Address,
        init_nonce: u64,
        init_balance: Wei,
    ) -> anyhow::Result<()> {
        self.inner
            .call("mint_account")
            .args_borsh((address, init_nonce, init_balance.raw().low_u64()))
            .gas(MAX_GAS)
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    pub async fn get_balance(&self, address: Address) -> anyhow::Result<Wei> {
        let outcome = self
            .inner
            .view("get_balance", address.as_bytes().to_vec())
            .await?;
        Ok(Wei::new(U256::from_big_endian(&outcome.result)))
    }
}
