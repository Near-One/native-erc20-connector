use crate::aurora_engine_utils::{erc20::ERC20, AuroraEngine};
use std::path::Path;
use workspaces::{network::Sandbox, Contract, Worker};

const STORAGE_DEPOSIT_AMOUNT: u128 = 1_000_000_000_000_000_000_000_000;

pub struct Wnear {
    pub inner: Contract,
    pub aurora_token: ERC20,
}

impl Wnear {
    pub async fn deploy(worker: &Worker<Sandbox>, engine: &AuroraEngine) -> anyhow::Result<Self> {
        let res = Path::new("res");
        let wasm = tokio::fs::read(res.join("w_near.wasm")).await?;

        // Deploy the wasm bytecode
        let contract = worker.dev_deploy(&wasm).await?;

        // Initialize the contract
        contract
            .call("new")
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Bridge to Aurora
        let aurora_token = engine.bridge_nep141(contract.id()).await?;
        engine
            .inner
            .call("factory_set_wnear_address")
            .args(aurora_token.address.as_bytes().to_vec())
            .transact()
            .await?
            .into_result()?;

        let result = Self {
            inner: contract,
            aurora_token,
        };

        // Register the Engine so that it can hold tokens
        result.storage_deposit(engine.inner.as_account()).await?;

        Ok(result)
    }

    pub async fn storage_deposit(&self, account: &workspaces::Account) -> anyhow::Result<()> {
        account
            .call(self.inner.id(), "storage_deposit")
            .args_json(AccountIdArgs {
                account_id: account.id(),
            })
            .deposit(STORAGE_DEPOSIT_AMOUNT)
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    pub async fn near_deposit(
        &self,
        account: &workspaces::Account,
        amount: u128,
    ) -> anyhow::Result<()> {
        account
            .call(self.inner.id(), "near_deposit")
            .deposit(amount)
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    pub async fn ft_balance_of(&self, account_id: &workspaces::AccountId) -> anyhow::Result<u128> {
        let outcome = self
            .inner
            .call("ft_balance_of")
            .args_json(AccountIdArgs { account_id })
            .transact()
            .await?;
        let result: String = outcome.json()?;
        Ok(result.parse()?)
    }
}

#[derive(serde::Serialize)]
struct AccountIdArgs<'a> {
    account_id: &'a workspaces::AccountId,
}
