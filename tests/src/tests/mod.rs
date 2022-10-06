use aurora_engine_types::types::{Address, Wei};

use crate::aurora_engine_utils::repo::AuroraEngineRepo;

#[tokio::test]
async fn test_compile_aurora_engine() {
    let contract = AuroraEngineRepo::download_and_compile_latest()
        .await
        .unwrap();
    assert!(!contract.is_empty());
}

#[tokio::test]
async fn test_deploy_aurora_engine() {
    let engine = crate::aurora_engine_utils::deploy_latest().await.unwrap();
    let address = Address::decode("000000000000000000000000000000000000000a").unwrap();
    let balance = Wei::new_u64(123456);
    engine.mint_account(address, 0, balance).await.unwrap();
    let view_balance = engine.get_balance(address).await.unwrap();
    assert_eq!(balance, view_balance);
}
