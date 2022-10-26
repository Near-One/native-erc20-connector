//! Module containing an abstraction over `near_jsonrpc_client::JsonRpcClient`.
//! This is useful because it makes mocking RPC interactions in tests easier
//! (we only need to provide an alternate implementation of this trait instead
//! of spinning up a real RPC server).

use async_trait::async_trait;
use near_jsonrpc_client::{methods::RpcMethod, JsonRpcClient, MethodCallResult};

/// An abstraction over `near_jsonrpc_client::JsonRpcClient`.
/// See module level-documentation for details.
#[async_trait]
pub trait ClientLike: Send + Sync + 'static {
    async fn do_call<M: RpcMethod + Send>(
        &self,
        method: M,
    ) -> MethodCallResult<M::Response, M::Error>;
}

#[async_trait]
impl ClientLike for JsonRpcClient {
    async fn do_call<M: RpcMethod + Send>(
        &self,
        method: M,
    ) -> MethodCallResult<M::Response, M::Error> {
        self.call(method).await
    }
}

#[cfg(test)]
pub mod mock {
    use near_jsonrpc_client::methods::tx::TransactionInfo;
    use near_primitives::{borsh::BorshDeserialize, transaction::SignedTransaction};

    use super::*;

    #[derive(Debug)]
    pub enum AllMethods {
        Query(near_jsonrpc_client::methods::query::RpcQueryRequest),
        BroadcastTxAsync(
            near_jsonrpc_client::methods::broadcast_tx_async::RpcBroadcastTxAsyncRequest,
        ),
        TxStatus(near_jsonrpc_client::methods::tx::RpcTransactionStatusRequest),
    }

    pub struct MockClient<F> {
        dispatch: F,
    }

    impl<F> MockClient<F>
    where
        F: Fn(AllMethods) -> serde_json::Value,
    {
        pub fn new(dispatch: F) -> Self {
            Self { dispatch }
        }
    }

    #[async_trait]
    impl<F> ClientLike for MockClient<F>
    where
        F: Fn(AllMethods) -> serde_json::Value + Send + Sync + 'static,
    {
        async fn do_call<M: RpcMethod + Send>(
            &self,
            method: M,
        ) -> MethodCallResult<M::Response, M::Error> {
            let name = method.method_name();
            let params = method.params().unwrap();

            let request = match name {
                "query" => AllMethods::Query(serde_json::from_value(params).unwrap()),
                "broadcast_tx_async" => {
                    let tx_base64 = params
                        .as_array()
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_str()
                        .unwrap();
                    let signed_tx = SignedTransaction::try_from_slice(
                        &near_primitives::serialize::from_base64(tx_base64).unwrap(),
                    )
                    .unwrap();
                    AllMethods::BroadcastTxAsync(near_jsonrpc_client::methods::broadcast_tx_async::RpcBroadcastTxAsyncRequest {signed_transaction: signed_tx})
                }
                "tx" => {
                    let params_arr = params.as_array().unwrap();
                    let transaction_info = if params_arr.len() == 1 {
                        let tx_base64 = params_arr.first().unwrap().as_str().unwrap();
                        let signed_tx = SignedTransaction::try_from_slice(
                            &near_primitives::serialize::from_base64(tx_base64).unwrap(),
                        )
                        .unwrap();
                        TransactionInfo::Transaction(signed_tx)
                    } else {
                        let hash = serde_json::from_value(params_arr[0].clone()).unwrap();
                        let account_id = serde_json::from_value(params_arr[1].clone()).unwrap();
                        TransactionInfo::TransactionId { hash, account_id }
                    };
                    AllMethods::TxStatus(
                        near_jsonrpc_client::methods::tx::RpcTransactionStatusRequest {
                            transaction_info,
                        },
                    )
                }
                other => panic!("RPC method {} not mocked", other),
            };
            let response = (self.dispatch)(request);
            M::parse_handler_response(response).unwrap().map_err(|e| {
                near_jsonrpc_client::errors::JsonRpcError::ServerError(
                    near_jsonrpc_client::errors::JsonRpcServerError::HandlerError(e),
                )
            })
        }
    }
}
