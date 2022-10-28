use crate::{
    config::Config,
    log::{AuroraTransactionKind, EventKind, Log, NearTransactionKind},
    near_rpc_ext::client_like::mock::{AllMethods, MockClient},
    tests::{default_transaction, default_transaction_outcome},
};
use aurora_engine::parameters::{SubmitResult, TransactionStatus};
use aurora_engine_types::types::Address;
use borsh::BorshSerialize;
use near_account_id::AccountId;
use near_primitives::hash::CryptoHash;

#[tokio::test]
async fn test_deploy() {
    let mut config = {
        let mut tmp = Config::testnet();
        // Allow changed files in the repo (makes test pass during development)
        tmp.allow_changed_files = true;
        tmp.repository_root = Some("..".into());
        tmp
    };
    let sk = near_crypto::SecretKey::from_random(near_crypto::KeyType::ED25519);
    let key = near_crypto::KeyFile {
        account_id: config.factory_account_id.clone(),
        public_key: sk.public_key(),
        secret_key: sk,
    };
    let params = MockClientParameters {
        block_height: 7,
        block_hash: near_primitives::hash::hash(b"the_block"),
        aurora_account_id: config.aurora_account_id.clone(),
        factory_account_id: config.factory_account_id.clone(),
        codec_lib_address: Address::decode("ab86c24a63b1c364e422f0214dafbc77cb5351ec").unwrap(),
        utils_lib_address: Address::decode("2cd3c85b8d055521e48655cb19ca684e9b88abc4").unwrap(),
        sdk_lib_address: Address::decode("71f31b756c9af6b61174c2411ec1f44021728e05").unwrap(),
        locker_address: Address::decode("41896a6ed87affe5f5059e03466723f7acc0d128").unwrap(),
        factory_code_hash: near_primitives::hash::hash(b"factory_code"),
        deploy_factory_tx_hash: near_primitives::hash::hash(b"deploy_factory_tx"),
        deploy_codec_tx_hash: near_primitives::hash::hash(b"deploy_codec_tx"),
        deploy_utils_tx_hash: near_primitives::hash::hash(b"deploy_utils_tx"),
        deploy_sdk_tx_hash: near_primitives::hash::hash(b"deploy_sdk_tx"),
        deploy_locker_tx_hash: near_primitives::hash::hash(b"deploy_locker_tx"),
        factory_init_tx_hash: near_primitives::hash::hash(b"factory_init_tx"),
        factory_set_token_tx_hash: near_primitives::hash::hash(b"factory_set_token_binary_tx"),
    };
    let client = default_client(params.clone());
    let mut log = Log::default();

    crate::deploy::deploy(&mut config, std::sync::Arc::new(client), &key, &mut log)
        .await
        .unwrap();

    // Locker address should be updated in the config after `deploy`
    assert_eq!(
        config.locker_address,
        Some(near_token_common::Address(params.locker_address.raw().0)),
    );

    // Check the logs contain the expected events:
    // First we make `near-token-factory` and `aurora-locker` (in either order)
    match log.events.get(0).map(|e| &e.kind) {
        Some(EventKind::Make { command }) if command.as_str() == "near-token-factory" => {
            match log.events.get(1).map(|e| &e.kind) {
                Some(EventKind::Make { command }) if command.as_str() == "aurora-locker" => (),
                other => panic!("Unexpected log: {:?}", other),
            }
        }
        Some(EventKind::Make { command }) if command.as_str() == "aurora-locker" => {
            match log.events.get(1).map(|e| &e.kind) {
                Some(EventKind::Make { command }) if command.as_str() == "near-token-factory" => (),
                other => panic!("Unexpected log: {:?}", other),
            }
        }
        other => panic!("Unexpected log: {:?}", other),
    }
    // Then we make the `near-token-contract`
    assert_eq!(
        log.events.get(2).map(|e| &e.kind),
        Some(&EventKind::Make {
            command: "near-token-contract".into()
        })
    );
    // Then we submit the deploy transaction
    assert_eq!(
        log.events.get(3).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSubmitted {
            hash: params.deploy_factory_tx_hash
        })
    );
    // And the deploy was successful
    assert_eq!(
        log.events.get(4).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSuccessful {
            hash: params.deploy_factory_tx_hash,
            kind: NearTransactionKind::DeployCode {
                account_id: params.factory_account_id,
                new_code_hash: params.factory_code_hash,
                previous_code_hash: None
            }
        })
    );
    // Then we submit the deploy codec transaction
    assert_eq!(
        log.events.get(5).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSubmitted {
            hash: params.deploy_codec_tx_hash
        })
    );
    // And it was successful
    assert_eq!(
        log.events.get(6).map(|e| &e.kind),
        Some(&EventKind::AuroraTransactionSuccessful {
            near_hash: Some(params.deploy_codec_tx_hash),
            aurora_hash: None,
            kind: AuroraTransactionKind::DeployContract {
                address: params.codec_lib_address,
            }
        })
    );
    // Then we submit the deploy utils transaction
    assert_eq!(
        log.events.get(7).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSubmitted {
            hash: params.deploy_utils_tx_hash
        })
    );
    // And it was successful
    assert_eq!(
        log.events.get(8).map(|e| &e.kind),
        Some(&EventKind::AuroraTransactionSuccessful {
            near_hash: Some(params.deploy_utils_tx_hash),
            aurora_hash: None,
            kind: AuroraTransactionKind::DeployContract {
                address: params.utils_lib_address,
            }
        })
    );
    // Then we build the aurora_sdk with the deployed libs
    assert_eq!(
        log.events.get(9).map(|e| &e.kind),
        Some(&EventKind::Make {
            command: format!(
                "CODEC=0x{} UTILS=0x{} aurora-locker-sdk",
                params.codec_lib_address.encode(),
                params.utils_lib_address.encode()
            )
        })
    );
    // Then we submit the deploy aurora_sdk transaction
    assert_eq!(
        log.events.get(10).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSubmitted {
            hash: params.deploy_sdk_tx_hash
        })
    );
    // And it was successful
    assert_eq!(
        log.events.get(11).map(|e| &e.kind),
        Some(&EventKind::AuroraTransactionSuccessful {
            near_hash: Some(params.deploy_sdk_tx_hash),
            aurora_hash: None,
            kind: AuroraTransactionKind::DeployContract {
                address: params.sdk_lib_address,
            }
        })
    );
    // Then we build the locker with the deployed libs
    assert_eq!(
        log.events.get(12).map(|e| &e.kind),
        Some(&EventKind::Make {
            command: format!(
                "CODEC=0x{} SDK=0x{} aurora-locker-with-libs",
                params.codec_lib_address.encode(),
                params.sdk_lib_address.encode()
            )
        })
    );
    // Then we submit the deploy locker transaction
    assert_eq!(
        log.events.get(13).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSubmitted {
            hash: params.deploy_locker_tx_hash
        })
    );
    // And it was successful
    assert_eq!(
        log.events.get(14).map(|e| &e.kind),
        Some(&EventKind::AuroraTransactionSuccessful {
            near_hash: Some(params.deploy_locker_tx_hash),
            aurora_hash: None,
            kind: AuroraTransactionKind::DeployContract {
                address: params.locker_address,
            }
        })
    );
    // Then we modify the config file with the new locker address
    assert_eq!(
        log.events.get(15).map(|e| &e.kind),
        Some(&EventKind::ModifyConfigLockerAddress {
            old_value: None,
            new_value: Some(near_token_common::Address(params.locker_address.raw().0))
        })
    );
    // Then we submit the initialize factory transaction
    assert_eq!(
        log.events.get(16).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSubmitted {
            hash: params.factory_init_tx_hash
        })
    );
    // And it was successful
    matches!(
        log.events.get(17).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSuccessful {
            hash,
            kind: NearTransactionKind::FunctionCall { .. }
        }) if hash == params.factory_init_tx_hash
    );
    // Then we submit the factory set_token_binary transaction
    assert_eq!(
        log.events.get(18).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSubmitted {
            hash: params.factory_set_token_tx_hash
        })
    );
    // And it was successful
    assert!(matches!(
        log.events.get(19).map(|e| &e.kind),
        Some(&EventKind::NearTransactionSuccessful {
            hash,
            kind: NearTransactionKind::FunctionCall { .. }
        }) if hash == params.factory_set_token_tx_hash
    ));
}

#[derive(Debug, Clone)]
struct MockClientParameters {
    pub block_height: u64,
    pub block_hash: CryptoHash,
    pub aurora_account_id: AccountId,
    pub factory_account_id: AccountId,
    pub codec_lib_address: Address,
    pub utils_lib_address: Address,
    pub sdk_lib_address: Address,
    pub locker_address: Address,
    pub factory_code_hash: CryptoHash,
    pub deploy_factory_tx_hash: CryptoHash,
    pub deploy_codec_tx_hash: CryptoHash,
    pub deploy_utils_tx_hash: CryptoHash,
    pub deploy_sdk_tx_hash: CryptoHash,
    pub deploy_locker_tx_hash: CryptoHash,
    pub factory_init_tx_hash: CryptoHash,
    pub factory_set_token_tx_hash: CryptoHash,
}

#[derive(Debug)]
struct MockClientState {
    params: MockClientParameters,
    state_machine: std::sync::Mutex<MockClientStateMachine>,
}

impl MockClientState {
    fn process_client_method(&self, method: AllMethods) -> serde_json::Value {
        use std::ops::DerefMut;

        let params = &self.params;
        let mut state_guard = self.state_machine.lock().unwrap();
        let state = state_guard.deref_mut();
        match state {
            MockClientStateMachine::FactoryAccountSanity {
                code_check,
                key_check,
            } => match method {
                AllMethods::Query(query) => {
                    let response = match query.request {
                        near_primitives::views::QueryRequest::ViewCode { account_id } => {
                            if account_id != params.factory_account_id {
                                panic!("Unexpected ViewCode query to {:?}", account_id);
                            }
                            *code_check = true;
                            near_jsonrpc_client::methods::query::RpcQueryResponse {
                                    kind: near_jsonrpc_primitives::types::query::QueryResponseKind::ViewCode(
                                        near_primitives::views::ContractCodeView {
                                            code: Vec::new(),
                                            hash: Default::default(),
                                        },
                                    ),
                                    block_height: params.block_height,
                                    block_hash: params.block_hash,
                                }
                        }
                        near_primitives::views::QueryRequest::ViewAccessKey {
                            account_id, ..
                        } => {
                            if account_id != params.factory_account_id {
                                panic!("Unexpected ViewAccessKey query to {:?}", account_id);
                            }
                            *key_check = true;
                            near_jsonrpc_client::methods::query::RpcQueryResponse {
                                    kind: near_jsonrpc_primitives::types::query::QueryResponseKind::AccessKey(
                                        near_primitives::views::AccessKeyView {
                                            nonce: 0,
                                            permission: near_primitives::views::AccessKeyPermissionView::FullAccess,
                                        },
                                    ),
                                    block_height: params.block_height,
                                    block_hash: params.block_hash,
                                }
                        }
                        other => panic!("Unexpected query: {:?}", other),
                    };
                    if *code_check && *key_check {
                        *state = MockClientStateMachine::DeployFactoryCode;
                    }
                    serde_json::to_value(response).unwrap()
                }
                other => panic!("Unexpected request {:?} in state {:?}", other, state),
            },
            MockClientStateMachine::DeployFactoryCode => match method {
                AllMethods::BroadcastTxAsync(tx) => {
                    let rx = &tx.signed_transaction.transaction.receiver_id;
                    if rx != &params.factory_account_id {
                        panic!("Unexpected transaction to {:?}", rx)
                    }
                    match tx.signed_transaction.transaction.actions.first() {
                        Some(near_primitives::transaction::Action::DeployContract(_)) => (),
                        other => panic!("Unexpected transaction action {:?}", other),
                    }
                    let response = params.deploy_factory_tx_hash;
                    *state = MockClientStateMachine::AwaitDeployExecute;
                    serde_json::to_value(response).unwrap()
                }
                other => panic!("Unexpected request {:?} in state {:?}", other, state),
            },
            MockClientStateMachine::AwaitDeployExecute => {
                let response = await_tx_handler(
                    method,
                    params.deploy_factory_tx_hash,
                    &params.factory_account_id,
                    Vec::new(),
                    state,
                );
                *state = MockClientStateMachine::CheckCodeDeployed;
                response
            }
            MockClientStateMachine::CheckCodeDeployed => match method {
                AllMethods::Query(query) => {
                    let response = match query.request {
                        near_primitives::views::QueryRequest::ViewCode { account_id } => {
                            if account_id != params.factory_account_id {
                                panic!("Unexpected ViewCode query to {:?}", account_id);
                            }
                            near_jsonrpc_client::methods::query::RpcQueryResponse {
                                    kind: near_jsonrpc_primitives::types::query::QueryResponseKind::ViewCode(
                                        near_primitives::views::ContractCodeView {
                                            code: b"factory_code".to_vec(),
                                            hash: params.factory_code_hash,
                                        },
                                    ),
                                    block_height: params.block_height,
                                    block_hash: params.block_hash,
                                }
                        }
                        other => panic!("Unexpected query: {:?}", other),
                    };
                    *state = MockClientStateMachine::DeployCodec;
                    serde_json::to_value(response).unwrap()
                }
                other => panic!("Unexpected request {:?} in state {:?}", other, state),
            },
            MockClientStateMachine::DeployCodec => {
                let response = aurora_deploy_handler(
                    method,
                    &params.aurora_account_id,
                    params.deploy_codec_tx_hash,
                    state,
                );
                *state = MockClientStateMachine::AwaitDeployCodec;
                response
            }
            MockClientStateMachine::AwaitDeployCodec => {
                let response = await_tx_handler(
                    method,
                    params.deploy_codec_tx_hash,
                    &params.factory_account_id,
                    submit_result_with_address(params.codec_lib_address)
                        .try_to_vec()
                        .unwrap(),
                    state,
                );
                *state = MockClientStateMachine::DeployUtils;
                response
            }
            MockClientStateMachine::DeployUtils => {
                let response = aurora_deploy_handler(
                    method,
                    &params.aurora_account_id,
                    params.deploy_utils_tx_hash,
                    state,
                );
                *state = MockClientStateMachine::AwaitDeployUtils;
                response
            }
            MockClientStateMachine::AwaitDeployUtils => {
                let response = await_tx_handler(
                    method,
                    params.deploy_utils_tx_hash,
                    &params.factory_account_id,
                    submit_result_with_address(params.utils_lib_address)
                        .try_to_vec()
                        .unwrap(),
                    state,
                );
                *state = MockClientStateMachine::DeployAuroraSdk;
                response
            }
            MockClientStateMachine::DeployAuroraSdk => {
                let response = aurora_deploy_handler(
                    method,
                    &params.aurora_account_id,
                    params.deploy_sdk_tx_hash,
                    state,
                );
                *state = MockClientStateMachine::AwaitDeployAuroraSdk;
                response
            }
            MockClientStateMachine::AwaitDeployAuroraSdk => {
                let response = await_tx_handler(
                    method,
                    params.deploy_sdk_tx_hash,
                    &params.factory_account_id,
                    submit_result_with_address(params.sdk_lib_address)
                        .try_to_vec()
                        .unwrap(),
                    state,
                );
                *state = MockClientStateMachine::DeployLocker;
                response
            }
            MockClientStateMachine::DeployLocker => {
                let response = aurora_deploy_handler(
                    method,
                    &params.aurora_account_id,
                    params.deploy_locker_tx_hash,
                    state,
                );
                *state = MockClientStateMachine::AwaitDeployLocker;
                response
            }
            MockClientStateMachine::AwaitDeployLocker => {
                let response = await_tx_handler(
                    method,
                    params.deploy_locker_tx_hash,
                    &params.factory_account_id,
                    submit_result_with_address(params.locker_address)
                        .try_to_vec()
                        .unwrap(),
                    state,
                );
                *state = MockClientStateMachine::InitializeFactory;
                response
            }
            MockClientStateMachine::InitializeFactory => {
                let response = match method {
                    AllMethods::BroadcastTxAsync(tx) => {
                        let rx = &tx.signed_transaction.transaction.receiver_id;
                        if rx != &params.factory_account_id {
                            panic!("Unexpected transaction to {:?}", rx)
                        }
                        match tx.signed_transaction.transaction.actions.first() {
                            Some(near_primitives::transaction::Action::FunctionCall(f)) => {
                                assert_eq!(f.method_name.as_str(), "new");
                            }
                            other => panic!("Unexpected transaction action {:?}", other),
                        }
                        let response = params.factory_init_tx_hash;
                        serde_json::to_value(response).unwrap()
                    }
                    other => panic!("Unexpected request {:?} in state {:?}", other, state),
                };
                *state = MockClientStateMachine::AwaitInitializeFactory;
                response
            }
            MockClientStateMachine::AwaitInitializeFactory => {
                let response = await_tx_handler(
                    method,
                    params.factory_init_tx_hash,
                    &params.factory_account_id,
                    Vec::new(),
                    state,
                );
                *state = MockClientStateMachine::SetTokenBinary;
                response
            }
            MockClientStateMachine::SetTokenBinary => {
                let response = match method {
                    AllMethods::BroadcastTxAsync(tx) => {
                        let rx = &tx.signed_transaction.transaction.receiver_id;
                        if rx != &params.factory_account_id {
                            panic!("Unexpected transaction to {:?}", rx)
                        }
                        match tx.signed_transaction.transaction.actions.first() {
                            Some(near_primitives::transaction::Action::FunctionCall(f)) => {
                                assert_eq!(f.method_name.as_str(), "set_token_binary");
                            }
                            other => panic!("Unexpected transaction action {:?}", other),
                        }
                        let response = params.factory_set_token_tx_hash;
                        serde_json::to_value(response).unwrap()
                    }
                    other => panic!("Unexpected request {:?} in state {:?}", other, state),
                };
                *state = MockClientStateMachine::AwaitSetTokenBinary;
                response
            }
            MockClientStateMachine::AwaitSetTokenBinary => {
                let response = await_tx_handler(
                    method,
                    params.factory_set_token_tx_hash,
                    &params.factory_account_id,
                    Vec::new(),
                    state,
                );
                *state = MockClientStateMachine::Done;
                response
            }
            MockClientStateMachine::Done => {
                panic!("Unexpected RPC method in Done state {:?}", method);
            }
        }
    }
}

fn submit_result_with_address(address: Address) -> SubmitResult {
    SubmitResult::new(
        TransactionStatus::Succeed(address.as_bytes().to_vec()),
        0,
        Vec::new(),
    )
}

#[track_caller]
fn aurora_deploy_handler(
    method: AllMethods,
    aurora_account_id: &AccountId,
    return_value: CryptoHash,
    state: &MockClientStateMachine,
) -> serde_json::Value {
    match method {
        AllMethods::BroadcastTxAsync(tx) => {
            let rx = &tx.signed_transaction.transaction.receiver_id;
            if rx != aurora_account_id {
                panic!("Unexpected transaction to {:?}", rx)
            }
            match tx.signed_transaction.transaction.actions.first() {
                Some(near_primitives::transaction::Action::FunctionCall(f)) => {
                    assert_eq!(f.method_name.as_str(), "deploy_code");
                }
                other => panic!("Unexpected transaction action {:?}", other),
            }
            let response = return_value;
            serde_json::to_value(response).unwrap()
        }
        other => panic!("Unexpected request {:?} in state {:?}", other, state),
    }
}

#[track_caller]
fn await_tx_handler(
    method: AllMethods,
    expected_hash: CryptoHash,
    expected_account_id: &AccountId,
    return_value: Vec<u8>,
    state: &MockClientStateMachine,
) -> serde_json::Value {
    match method {
        AllMethods::TxStatus(request) => {
            match request.transaction_info {
                near_jsonrpc_client::methods::tx::TransactionInfo::Transaction(_) => {
                    panic!("Unexpected TransactionInfo query")
                }
                near_jsonrpc_client::methods::tx::TransactionInfo::TransactionId {
                    hash,
                    account_id,
                } => {
                    if hash != expected_hash {
                        panic!("Unexpected query of tx {:?}", hash);
                    }
                    if &account_id != expected_account_id {
                        panic!("Unexpected query of tx to account {:?}", account_id);
                    }
                }
            }
            let response = near_primitives::views::FinalExecutionOutcomeView {
                status: near_primitives::views::FinalExecutionStatus::SuccessValue(return_value),
                transaction: default_transaction(),
                transaction_outcome: default_transaction_outcome(),
                receipts_outcome: Vec::new(),
            };
            serde_json::to_value(response).unwrap()
        }
        other => panic!("Unexpected request {:?} in state {:?}", other, state),
    }
}

/// State machine describing how the Deploy flow should proceed
#[derive(Debug)]
enum MockClientStateMachine {
    /// 1. check factory account has no code and we have an access key for it
    FactoryAccountSanity { code_check: bool, key_check: bool },
    /// 2. send deploy factory code transaction
    DeployFactoryCode,
    /// 3. send tx status check to see if deploy is done
    AwaitDeployExecute,
    /// 4. check the factory account code hash again
    CheckCodeDeployed,
    /// 5. deploy codec library to Aurora
    DeployCodec,
    /// 6. send tx status check to see if deploy is done
    AwaitDeployCodec,
    /// 7. deploy utils library to Aurora
    DeployUtils,
    /// 8. send tx status check to see if deploy is done
    AwaitDeployUtils,
    /// 9. deploy Aurora SDK library to Aurora
    DeployAuroraSdk,
    /// 10. send tx status check to see if deploy is done
    AwaitDeployAuroraSdk,
    /// 11. deploy Locker contract to Aurora
    DeployLocker,
    /// 12. send tx status check to see if deploy is done
    AwaitDeployLocker,
    /// 13. call `new` function in Factory
    InitializeFactory,
    /// 14. send tx status check to see if call is done
    AwaitInitializeFactory,
    /// 15. call `set_token_binary` function in Factory
    SetTokenBinary,
    /// 16. send tx status check to see if call is done
    AwaitSetTokenBinary,
    /// Nothing else to do
    Done,
}

impl Default for MockClientStateMachine {
    fn default() -> Self {
        Self::FactoryAccountSanity {
            code_check: false,
            key_check: false,
        }
    }
}

fn default_client(
    params: MockClientParameters,
) -> MockClient<impl Fn(AllMethods) -> serde_json::Value + Send + Sync + 'static> {
    let state = MockClientState {
        params,
        state_machine: std::sync::Mutex::new(MockClientStateMachine::default()),
    };

    MockClient::new(move |method: AllMethods| state.process_client_method(method))
}
