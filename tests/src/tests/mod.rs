use crate::{
    aurora_engine_utils::{self, erc20, erc20::ERC20DeployedAt, repo::AuroraEngineRepo},
    wnear_utils::Wnear,
};
use aurora_engine::parameters::{CallArgs, FunctionCallArgsV2, SubmitResult};
use aurora_engine_precompiles::xcc::cross_contract_call;
use aurora_engine_types::{
    parameters::{CrossContractCallArgs, PromiseArgs, PromiseCreateArgs},
    types::{Address, NearGas, Wei, Yocto},
};
use borsh::BorshSerialize;

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
    let erc20 = constructor.abi.deployed_at(address);
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
