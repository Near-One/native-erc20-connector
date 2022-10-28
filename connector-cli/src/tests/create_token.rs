use crate::{
    config::Config,
    log::{EventKind, Log},
    near_rpc_ext::client_like::mock::{AllMethods, MockClient},
    tests::{default_transaction, default_transaction_outcome},
};
use aurora_engine::parameters::{SubmitResult, TransactionStatus};
use aurora_engine_types::types::Address;
use borsh::BorshSerialize;
use near_account_id::AccountId;
use near_primitives::hash::CryptoHash;

#[tokio::test]
async fn test_create_token() {
    let config = {
        let mut tmp = Config::testnet();
        // Pretend the locker is deployed already
        tmp.locker_address = Some(near_token_common::Address(
            hex::decode("c02079e49b8b2dc5b302d37d6c8beea26504dcd5")
                .unwrap()
                .try_into()
                .unwrap(),
        ));
        tmp
    };
    let sk = near_crypto::SecretKey::from_random(near_crypto::KeyType::ED25519);
    let key = near_crypto::KeyFile {
        account_id: config.factory_account_id.clone(),
        public_key: sk.public_key(),
        secret_key: sk,
    };

    let aurora_account_id = config.aurora_account_id.clone();
    let client = MockClient::new(move |method| rpc_handler(method, &aurora_account_id));

    let mut log = Log::default();

    let token_address = Address::decode("ed48f70ae9da554129680abade12c53d4d3ed51e").unwrap();
    crate::create_token::create_token(
        token_address,
        &config,
        std::sync::Arc::new(client),
        &key,
        &mut log,
    )
    .await
    .unwrap();

    // We submit the `create_token` transaction
    assert!(matches!(
        log.events.first().map(|e| &e.kind),
        Some(EventKind::NearTransactionSubmitted { .. }),
    ));
    // And it is successful
    assert!(matches!(
        log.events.get(1).map(|e| &e.kind),
        Some(EventKind::AuroraTransactionSuccessful { .. }),
    ));
}

fn rpc_handler(method: AllMethods, aurora_account_id: &AccountId) -> serde_json::Value {
    match method {
        AllMethods::Query(query) => {
            let response = match query.request {
                near_primitives::views::QueryRequest::ViewAccessKey { .. } => {
                    near_jsonrpc_client::methods::query::RpcQueryResponse {
                        kind: near_jsonrpc_primitives::types::query::QueryResponseKind::AccessKey(
                            near_primitives::views::AccessKeyView {
                                nonce: 0,
                                permission:
                                    near_primitives::views::AccessKeyPermissionView::FullAccess,
                            },
                        ),
                        block_height: 0,
                        block_hash: Default::default(),
                    }
                }
                other => panic!("Unexpected query: {:?}", other),
            };
            serde_json::to_value(response).unwrap()
        }
        AllMethods::BroadcastTxAsync(tx) => {
            let rx = &tx.signed_transaction.transaction.receiver_id;
            if rx != aurora_account_id {
                panic!("Unexpected transaction to {:?}", rx)
            }
            match tx.signed_transaction.transaction.actions.first() {
                Some(near_primitives::transaction::Action::FunctionCall(f)) => {
                    assert_eq!(f.method_name.as_str(), "call");
                }
                other => panic!("Unexpected transaction action {:?}", other),
            }
            let response = CryptoHash::default();
            serde_json::to_value(response).unwrap()
        }
        AllMethods::TxStatus(_request) => {
            let response = near_primitives::views::FinalExecutionOutcomeView {
                status: near_primitives::views::FinalExecutionStatus::SuccessValue(
                    SubmitResult::new(TransactionStatus::Succeed(Vec::new()), 0, Vec::new())
                        .try_to_vec()
                        .unwrap(),
                ),
                transaction: default_transaction(),
                transaction_outcome: default_transaction_outcome(),
                receipts_outcome: Vec::new(),
            };
            serde_json::to_value(response).unwrap()
        }
    }
}
