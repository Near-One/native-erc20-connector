use crate::aurora_engine_utils::{self, erc20, erc20::ERC20DeployedAt, repo::AuroraEngineRepo};
use aurora_engine_types::{
    types::{Address, Wei},
    U256,
};

#[tokio::test]
async fn test_compile_aurora_engine() {
    let contract = AuroraEngineRepo::download_and_compile_latest()
        .await
        .unwrap();
    assert!(!contract.is_empty());
}

#[tokio::test]
async fn test_deploy_aurora_engine() {
    let engine = aurora_engine_utils::deploy_latest().await.unwrap();
    let address = Address::decode("000000000000000000000000000000000000000a").unwrap();
    let balance = Wei::new_u64(123456);
    engine.mint_account(address, 0, balance).await.unwrap();
    let view_balance = engine.get_balance(address).await.unwrap();
    assert_eq!(balance, view_balance);
}

#[tokio::test]
async fn test_deploy_erc20() {
    let engine = aurora_engine_utils::deploy_latest().await.unwrap();
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
    let result = engine
        .view_evm_contract(address, erc20.balance_of(recipient), None, Wei::zero())
        .await
        .unwrap();
    let balance = aurora_engine_utils::unwrap_success(result)
        .map(|bytes| U256::from_big_endian(&bytes))
        .unwrap();
    assert_eq!(balance, mint_amount);
}
