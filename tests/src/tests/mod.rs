use crate::{
    acl_utils::{call_access_controlled_method, call_acl_has_role},
    aurora_engine_utils::{self, erc20, erc20::ERC20DeployedAt, repo::AuroraEngineRepo},
    aurora_locker_utils::{self, LockerDeployedAt},
    nep141_utils,
    token_factory_utils::{self, TokenFactory},
    wnear_utils::Wnear,
};
use aurora_engine::parameters::{CallArgs, FunctionCallArgsV2, SubmitResult};
use aurora_engine_precompiles::xcc::cross_contract_call;
use aurora_engine_types::{
    parameters::{CrossContractCallArgs, PromiseArgs, PromiseCreateArgs},
    types::{Address, NearGas, Wei, Yocto},
};
use borsh::BorshSerialize;
use near_sdk::serde_json::json;
use near_token_common::UpdateFungibleTokenMetadata;

mod promise_result;

#[tokio::test]
async fn test_compile_aurora_engine() {
    let contract = AuroraEngineRepo::download_and_compile_latest()
        .await
        .unwrap();
    assert!(!contract.is_empty());
}

#[tokio::test]
async fn test_deploy_aurora_engine() {
    let worker = workspaces::sandbox().await.unwrap();
    let engine = aurora_engine_utils::deploy_latest(&worker).await.unwrap();
    let address = Address::decode("000000000000000000000000000000000000000a").unwrap();
    let balance = Wei::new_u64(123456);
    engine.mint_account(address, 0, balance).await.unwrap();
    let view_balance = engine.get_balance(address).await.unwrap();
    assert_eq!(balance, view_balance);
}

#[tokio::test]
async fn test_deploy_erc20() {
    let worker = workspaces::sandbox().await.unwrap();
    let engine = aurora_engine_utils::deploy_latest(&worker).await.unwrap();
    let constructor = erc20::Constructor::load().await.unwrap();
    let address = engine
        .deploy_evm_contract(constructor.deploy_code("TEST", "AAA"))
        .await
        .unwrap();
    let erc20 = constructor.deployed_at(address);
    let mint_amount = 7654321.into();
    let recipient = Address::decode("000000000000000000000000000000000000000a").unwrap();
    let result = engine
        .call_evm_contract(address, erc20.mint(recipient, mint_amount), Wei::zero())
        .await
        .unwrap();
    aurora_engine_utils::unwrap_success(result.status).unwrap();
    let balance = engine.erc20_balance_of(&erc20, recipient).await.unwrap();
    assert_eq!(balance, mint_amount);
}

#[tokio::test]
async fn test_deploy_wnear() {
    let worker = workspaces::sandbox().await.unwrap();
    let engine = aurora_engine_utils::deploy_latest(&worker).await.unwrap();
    let wnear = Wnear::deploy(&worker, &engine).await.unwrap();

    // Try bridging some wnear into Aurora
    let deposit_amount = 100_567;
    let recipient = Address::decode("000000000000000000000000000000000000000a").unwrap();
    engine
        .mint_wnear(&wnear, recipient, deposit_amount)
        .await
        .unwrap();

    // Aurora Engine account owns the wnear tokens at the NEAR level
    let balance = wnear.ft_balance_of(engine.inner.id()).await.unwrap();
    assert_eq!(balance, deposit_amount);

    // Recipient address owns the tokens inside the EVM
    let balance = engine
        .erc20_balance_of(&wnear.aurora_token, recipient)
        .await
        .unwrap();
    assert_eq!(balance, deposit_amount.into());
}

#[tokio::test]
async fn test_engine_xcc() {
    // Deploy engine, wnear; create user NEAR account
    let worker = workspaces::sandbox().await.unwrap();
    let engine = aurora_engine_utils::deploy_latest(&worker).await.unwrap();
    let wnear = Wnear::deploy(&worker, &engine).await.unwrap();
    let user = worker.dev_create_account().await.unwrap();
    let user_address = aurora_engine_sdk::types::near_account_to_evm_address(user.id().as_bytes());
    // A simple contract with one method `hello`, that logs "HELLO {name}", where `name` is an input string.
    let hello_contract = {
        let res = std::path::Path::new("res");
        let wasm = tokio::fs::read(res.join("hello.wasm")).await.unwrap();
        worker.dev_deploy(&wasm).await.unwrap()
    };

    // Give user some WNEAR
    let deposit_amount = 5_000_000_000_000_000_000_000_000;
    engine
        .mint_wnear(&wnear, user_address, deposit_amount)
        .await
        .unwrap();

    // User approves XCC precompile to spend their WNEAR
    let result = engine
        .call_evm_contract_with(
            &user,
            wnear.aurora_token.address,
            wnear
                .aurora_token
                .approve(cross_contract_call::ADDRESS, deposit_amount.into()),
            Wei::zero(),
        )
        .await
        .unwrap();
    aurora_engine_utils::unwrap_success(result.status).unwrap();

    // Call XCC precompile to invoke hello contract
    let promise = PromiseArgs::Create(PromiseCreateArgs {
        target_account_id: hello_contract.id().as_str().parse().unwrap(),
        method: "hello".into(),
        args: r#"{"name": "WORLD!"}"#.as_bytes().to_vec(),
        attached_balance: Yocto::new(0),
        attached_gas: NearGas::new(5_000_000_000_000),
    });
    let args = CallArgs::V2(FunctionCallArgsV2 {
        contract: cross_contract_call::ADDRESS,
        value: [0u8; 32],
        input: CrossContractCallArgs::Eager(promise).try_to_vec().unwrap(),
    });
    let outcome = user
        .call(engine.inner.id(), "call")
        .args_borsh(args)
        .max_gas()
        .transact()
        .await
        .unwrap();
    // Check the cross-contract call was made by looking at the logs
    assert!(outcome.logs().contains(&"HELLO WORLD!"));
    let result: SubmitResult = outcome.borsh().unwrap();
    aurora_engine_utils::unwrap_success(result.status).unwrap();
}

#[tokio::test]
async fn test_deploy_token_factory() {
    let worker = workspaces::sandbox().await.unwrap();
    let engine = aurora_engine_utils::deploy_latest(&worker).await.unwrap();
    // In reality we would deploy the locker contract and get its address,
    // but that is not needed for this test. We can choose any address we like.
    let locker_address = Address::decode("000000000000000000000000000000000000000a").unwrap();
    let _factory = TokenFactory::deploy(&worker, locker_address, engine.inner.id())
        .await
        .unwrap();
}

#[tokio::test]
async fn test_near_token_contract_acl() -> anyhow::Result<()> {
    // Spin up a sandbox, compile, and deploy `near-token-contract`.
    let worker = workspaces::sandbox().await?;
    let wasm = TokenFactory::compile_token().await?;
    let contract = worker.dev_deploy(&wasm).await?;

    // Initialize the contract.
    contract
        .call("new")
        .args_json(json!({
            "super_admin": Some("token_admin"),
        }))
        .deposit(near_sdk::ONE_NEAR)
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    // Calling access controlled method from account without role fails.
    let account_no_roles = worker.dev_create_account().await?;
    call_access_controlled_method(
        &account_no_roles,
        &contract,
        "update_metadata",
        json!({ "metadata": UpdateFungibleTokenMetadata::default() }),
    )
    .await?
    .assert_acl_failure();

    // Calling access controlled method from account with permission succeeds.
    call_access_controlled_method(
        contract.as_account(),
        &contract,
        "update_metadata",
        json!({ "metadata": UpdateFungibleTokenMetadata::default() }),
    )
    .await?
    .assert_success_unit_return();

    // Calling a method provided by `#[access_controllable]`.
    assert!(!call_acl_has_role(&contract, "MetadataUpdater", account_no_roles.id()).await?);
    assert!(call_acl_has_role(&contract, "MetadataUpdater", contract.id()).await?);

    Ok(())
}

#[tokio::test]
async fn test_native_token_connector() {
    let wnear_mint_amount = 5_000_000_000_000_000_000_000_000_u128;
    let token_mint_amount = 0x_1000_0000_0000_0000_u128;
    let token_deposit_amount = 0x_aaaa_bbbb_cccc_u128;
    let context = NativeTokenConnectorTestContext::new().await.unwrap();
    let user = context.worker.dev_create_account().await.unwrap();
    let user_address = aurora_engine_sdk::types::near_account_to_evm_address(user.id().as_bytes());

    // Mint ERC-20 tokens for user in EVM
    let mint_result = context
        .engine
        .call_evm_contract(
            context.erc20.address,
            context.erc20.mint(user_address, token_mint_amount.into()),
            Wei::zero(),
        )
        .await
        .unwrap();
    aurora_engine_utils::unwrap_success(mint_result.status).unwrap();

    // Mint NEAR for user in EVM (a wNEAR balance is required to deploy a new token)
    context
        .engine
        .mint_wnear(&context.wnear, user_address, wnear_mint_amount)
        .await
        .unwrap();

    // Approve locker to take tokens from user
    let approve_result = context
        .engine
        .call_evm_contract_with(
            &user,
            context.erc20.address,
            context
                .erc20
                .approve(context.locker.address, token_mint_amount.into()),
            Wei::zero(),
        )
        .await
        .unwrap();
    aurora_engine_utils::unwrap_success(approve_result.status).unwrap();

    // Approve locker to take NEAR from user
    let approve_result = context
        .engine
        .call_evm_contract_with(
            &user,
            context.wnear.aurora_token.address,
            context
                .wnear
                .aurora_token
                .approve(context.locker.address, wnear_mint_amount.into()),
            Wei::zero(),
        )
        .await
        .unwrap();
    aurora_engine_utils::unwrap_success(approve_result.status).unwrap();

    // Create the token on NEAR
    let create_result = context
        .engine
        .call_evm_contract_with(
            &user,
            context.locker.address,
            context.locker.create_token(context.erc20.address),
            Wei::zero(),
        )
        .await
        .unwrap();
    aurora_engine_utils::unwrap_success(create_result.status).unwrap();

    // Confirm token was created using a view call
    // (if the account was not created the view call would fail because the account does not exist).
    let token_account = format!(
        "{}.{}",
        context.erc20.address.encode(),
        context.factory.inner.id()
    )
    .parse()
    .unwrap();
    let balance = nep141_utils::ft_balance_of(&user, &token_account, context.factory.inner.id())
        .await
        .unwrap();
    assert_eq!(balance, 0);

    // Before a deposit will be accepted, the user must do the storage registration
    let create_result = context
        .engine
        .call_evm_contract_with(
            &user,
            context.locker.address,
            context
                .locker
                .storage_deposit(context.erc20.address, user.id()),
            Wei::zero(),
        )
        .await
        .unwrap();
    aurora_engine_utils::unwrap_success(create_result.status).unwrap();

    // Deposit tokens into locker
    let deposit_result = context
        .engine
        .call_evm_contract_with(
            &user,
            context.locker.address,
            context
                .locker
                .deposit(context.erc20.address, user.id(), token_deposit_amount),
            Wei::zero(),
        )
        .await
        .unwrap();
    aurora_engine_utils::unwrap_success(deposit_result.status).unwrap();

    // The deposit call to the locker only schedules, need to actually execute it
    let locker_near_account = format!(
        "{}.{}",
        context.locker.address.encode(),
        context.engine.inner.id()
    )
    .parse()
    .unwrap();
    let deposit_outcome = user
        .call(&locker_near_account, "execute_scheduled")
        .args_json(serde_json::json!({
            "nonce": "0",
        }))
        .max_gas()
        .transact()
        .await
        .unwrap();
    deposit_outcome.into_result().unwrap();

    // Verify the balance exists on NEAR now
    let balance = nep141_utils::ft_balance_of(&user, &token_account, user.id())
        .await
        .unwrap();
    assert_eq!(balance, token_deposit_amount);

    // Verify the tokens have been taken from the user in the EVM
    let evm_token_balance = context
        .engine
        .erc20_balance_of(&context.erc20, user_address)
        .await
        .unwrap();
    assert_eq!(
        evm_token_balance,
        (token_mint_amount - token_deposit_amount).into()
    );

    // Withdraw the tokens from NEAR back to the EVM
    let withdraw_outcome = user
        .call(&token_account, "withdraw")
        .args_json(serde_json::json!({
            "receiver_id": user_address.encode(),
            "amount": token_deposit_amount.to_string(),
        }))
        .max_gas()
        .transact()
        .await
        .unwrap();
    withdraw_outcome.into_result().unwrap();

    // Verify the balance removed from NEAR
    let balance = nep141_utils::ft_balance_of(&user, &token_account, user.id())
        .await
        .unwrap();
    assert_eq!(balance, 0);

    // Verify the tokens have been returned to the user in the EVM
    let evm_token_balance = context
        .engine
        .erc20_balance_of(&context.erc20, user_address)
        .await
        .unwrap();
    assert_eq!(evm_token_balance, token_mint_amount.into());
}

struct NativeTokenConnectorTestContext {
    pub worker: workspaces::Worker<workspaces::network::Sandbox>,
    pub engine: aurora_engine_utils::AuroraEngine,
    pub wnear: Wnear,
    pub locker: aurora_locker_utils::AuroraLocker,
    pub factory: TokenFactory,
    pub erc20: erc20::ERC20,
}

impl NativeTokenConnectorTestContext {
    pub async fn new() -> anyhow::Result<Self> {
        let worker = workspaces::sandbox().await?;
        let engine = aurora_engine_utils::deploy_latest(&worker).await?;
        let wnear = Wnear::deploy(&worker, &engine).await?;
        let locker = {
            let constructor = aurora_locker_utils::create_locker_constructor(&engine).await?;
            let address = engine
                .deploy_evm_contract(constructor.deploy_code(
                    &token_factory_utils::FACTORY_ACCOUNT_ID.parse()?,
                    wnear.aurora_token.address,
                ))
                .await?;
            constructor.deployed_at(address)
        };
        let factory = TokenFactory::deploy(&worker, locker.address, engine.inner.id()).await?;

        // The engine (ie Aurora) will fund the creation of the Locker's NEAR account.
        // To do this it needs to have some wnear and approve the locker to use it.
        let engine_implicit_address =
            aurora_engine_sdk::types::near_account_to_evm_address(engine.inner.id().as_bytes());
        let wnear_mint_amount = 5_000_000_000_000_000_000_000_000_u128;
        engine
            .mint_wnear(&wnear, engine_implicit_address, wnear_mint_amount)
            .await
            .unwrap();
        let approve_result = engine
            .call_evm_contract(
                wnear.aurora_token.address,
                wnear
                    .aurora_token
                    .approve(locker.address, wnear_mint_amount.into()),
                Wei::zero(),
            )
            .await
            .unwrap();
        aurora_engine_utils::unwrap_success(approve_result.status).unwrap();
        let init_result = engine
            .call_evm_contract(locker.address, locker.init_near_account(), Wei::zero())
            .await
            .unwrap();
        aurora_engine_utils::unwrap_success(init_result.status).unwrap();

        let erc20 = {
            let constructor = erc20::Constructor::load().await?;
            let address = engine
                .deploy_evm_contract(constructor.deploy_code("TEST", "AAA"))
                .await?;
            constructor.deployed_at(address)
        };
        Ok(Self {
            worker,
            engine,
            wnear,
            locker,
            factory,
            erc20,
        })
    }
}
