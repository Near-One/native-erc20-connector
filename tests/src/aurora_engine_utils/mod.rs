use aurora_engine::parameters::{
    CallArgs, FunctionCallArgsV2, SubmitResult, TransactionStatus, ViewCallArgs,
};
use aurora_engine_types::{
    types::{Address, Wei},
    U256,
};
use workspaces::Contract;

pub mod erc20;
pub mod repo;

const TESTNET_CHAIN_ID: u64 = 1313161555;
const MAX_GAS: u64 = 300_000_000_000_000;

/// Newtype for bytes that are meant to be used as the input for an EVM contract.
pub struct ContractInput(pub Vec<u8>);

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

    pub async fn deploy_evm_contract(&self, code: Vec<u8>) -> anyhow::Result<Address> {
        let outcome = self
            .inner
            .call("deploy_code")
            .args(code)
            .gas(MAX_GAS)
            .transact()
            .await?;
        let result: SubmitResult = outcome.borsh()?;
        let address = unwrap_success(result.status).and_then(|bytes| {
            Address::try_from_slice(&bytes)
                .map_err(|_| anyhow::Error::msg("Deploy result failed to parse as address"))
        })?;
        Ok(address)
    }

    pub async fn call_evm_contract(
        &self,
        address: Address,
        input: ContractInput,
        value: Wei,
    ) -> anyhow::Result<SubmitResult> {
        let args = CallArgs::V2(FunctionCallArgsV2 {
            contract: address,
            value: value.to_bytes(),
            input: input.0,
        });
        let outcome = self
            .inner
            .call("call")
            .args_borsh(args)
            .gas(MAX_GAS)
            .transact()
            .await?;
        let result = outcome.borsh()?;
        Ok(result)
    }

    pub async fn view_evm_contract(
        &self,
        contract: Address,
        input: ContractInput,
        sender: Option<Address>,
        value: Wei,
    ) -> anyhow::Result<TransactionStatus> {
        let args = ViewCallArgs {
            sender: sender.unwrap_or_default(),
            address: contract,
            amount: value.to_bytes(),
            input: input.0,
        };
        let outcome = self
            .inner
            .call("view")
            .args_borsh(args)
            .gas(MAX_GAS)
            .transact()
            .await?;
        let result = outcome.borsh()?;
        Ok(result)
    }
}

pub fn unwrap_success(status: TransactionStatus) -> anyhow::Result<Vec<u8>> {
    match status {
        TransactionStatus::Succeed(bytes) => Ok(bytes),
        status => Err(anyhow::Error::msg(format!(
            "Transaction failed: {:?}",
            status
        ))),
    }
}
