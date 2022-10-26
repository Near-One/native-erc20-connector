use crate::{
    config::Config,
    log::{EventKind, Log, NearTransactionKind},
    near_rpc_ext::client_like::mock::{AllMethods, MockClient},
};
use near_account_id::AccountId;
use near_primitives::{hash::CryptoHash, views};

#[tokio::test]
async fn test_deploy() {
    let config = {
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
        factory_account_id: config.factory_account_id.clone(),
        factory_code_hash: near_primitives::hash::hash(b"factory_code"),
        deploy_factory_tx_hash: near_primitives::hash::hash(b"deploy_factory_tx"),
    };
    let client = default_client(params.clone());
    let mut log = Log::default();

    crate::deploy::deploy(&config, std::sync::Arc::new(client), &key, &mut log)
        .await
        .unwrap();

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
}

#[derive(Debug, Clone)]
struct MockClientParameters {
    pub block_height: u64,
    pub block_hash: CryptoHash,
    pub factory_account_id: AccountId,
    pub factory_code_hash: CryptoHash,
    pub deploy_factory_tx_hash: CryptoHash,
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
            MockClientStateMachine::AwaitDeployExecute => match method {
                AllMethods::TxStatus(request) => {
                    match request.transaction_info {
                        near_jsonrpc_client::methods::tx::TransactionInfo::Transaction(_) => {
                            panic!("Unexpected TransactionInfo query")
                        }
                        near_jsonrpc_client::methods::tx::TransactionInfo::TransactionId {
                            hash,
                            account_id,
                        } => {
                            if hash != params.deploy_factory_tx_hash {
                                panic!("Unexpected query of tx {:?}", hash);
                            }
                            if account_id != params.factory_account_id {
                                panic!("Unexpected query of tx to account {:?}", account_id);
                            }
                        }
                    }
                    let response = near_primitives::views::FinalExecutionOutcomeView {
                        status: near_primitives::views::FinalExecutionStatus::SuccessValue(
                            Vec::new(),
                        ),
                        transaction: default_transaction(),
                        transaction_outcome: default_transaction_outcome(),
                        receipts_outcome: Vec::new(),
                    };
                    *state = MockClientStateMachine::CheckCodeDeployed;
                    serde_json::to_value(response).unwrap()
                }
                other => panic!("Unexpected request {:?} in state {:?}", other, state),
            },
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
                    *state = MockClientStateMachine::Done;
                    serde_json::to_value(response).unwrap()
                }
                other => panic!("Unexpected request {:?} in state {:?}", other, state),
            },
            MockClientStateMachine::Done => {
                panic!("Unexpected RPC method in Done state {:?}", method);
            }
        }
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

fn default_transaction() -> views::SignedTransactionView {
    views::SignedTransactionView {
        signer_id: "signer.near".parse().unwrap(),
        public_key: near_crypto::PublicKey::empty(near_crypto::KeyType::ED25519),
        nonce: 0,
        receiver_id: "receiver.near".parse().unwrap(),
        actions: Vec::new(),
        signature: near_crypto::Signature::empty(near_crypto::KeyType::ED25519),
        hash: Default::default(),
    }
}

fn default_transaction_outcome() -> views::ExecutionOutcomeWithIdView {
    views::ExecutionOutcomeWithIdView {
        proof: Vec::new(),
        block_hash: Default::default(),
        id: Default::default(),
        outcome: views::ExecutionOutcomeView {
            logs: Vec::new(),
            receipt_ids: Vec::new(),
            gas_burnt: 0,
            tokens_burnt: 0,
            executor_id: "executor.near".parse().unwrap(),
            status: views::ExecutionStatusView::SuccessValue(Vec::new()),
            metadata: views::ExecutionMetadataView {
                version: 0,
                gas_profile: None,
            },
        },
    }
}
