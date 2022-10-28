//! Extension to `near-jsonrpc-client` to allow parallel execution and stronger typing.

use near_account_id::AccountId;
use near_crypto::PublicKey;
use near_jsonrpc_client::methods::{self, tx::TransactionInfo};
use near_primitives::{
    errors::TxExecutionError,
    hash::CryptoHash,
    transaction::SignedTransaction,
    types::{BlockHeight, BlockReference},
    views::{
        AccessKeyView, ContractCodeView, FinalExecutionOutcomeView, FinalExecutionStatus,
        QueryRequest,
    },
};
use std::{marker::PhantomData, sync::Arc};
use tokio::task::JoinHandle;

pub mod aurora_engine_utils;
pub mod client_like;
pub mod type_munging;

pub const MAX_NEAR_GAS: u64 = 300_000_000_000_000;

pub fn query_access_key(
    account_id: AccountId,
    public_key: PublicKey,
) -> JsonRpcRequest<methods::query::RpcQueryRequest, JsonRpcQueryResponse<AccessKeyView>> {
    JsonRpcRequest {
        inner: methods::query::RpcQueryRequest {
            block_reference: BlockReference::latest(),
            request: QueryRequest::ViewAccessKey {
                account_id,
                public_key,
            },
        },
        _phantom: Default::default(),
    }
}

pub fn query_code(
    account_id: AccountId,
) -> JsonRpcRequest<methods::query::RpcQueryRequest, JsonRpcQueryResponse<ContractCodeView>> {
    JsonRpcRequest {
        inner: methods::query::RpcQueryRequest {
            block_reference: BlockReference::latest(),
            request: QueryRequest::ViewCode { account_id },
        },
        _phantom: Default::default(),
    }
}

pub fn broadcast_tx_async(
    signed_transaction: SignedTransaction,
) -> JsonRpcRequest<methods::broadcast_tx_async::RpcBroadcastTxAsyncRequest, CryptoHash> {
    JsonRpcRequest {
        inner: methods::broadcast_tx_async::RpcBroadcastTxAsyncRequest { signed_transaction },
        _phantom: Default::default(),
    }
}

pub fn tx_status(
    account_id: AccountId,
    tx_hash: CryptoHash,
) -> JsonRpcRequest<methods::tx::RpcTransactionStatusRequest, FinalExecutionOutcomeView> {
    JsonRpcRequest {
        inner: methods::tx::RpcTransactionStatusRequest {
            transaction_info: TransactionInfo::TransactionId {
                hash: tx_hash,
                account_id,
            },
        },
        _phantom: Default::default(),
    }
}

pub async fn wait_tx_executed<C: client_like::ClientLike>(
    account_id: AccountId,
    tx_hash: CryptoHash,
    client: &C,
) -> anyhow::Result<Result<Vec<u8>, TxExecutionError>> {
    let request = tx_status(account_id, tx_hash);
    loop {
        let outcome = client.do_call(&request.inner).await?;
        match outcome.status {
            FinalExecutionStatus::SuccessValue(bytes) => {
                return Ok(Ok(bytes));
            }
            FinalExecutionStatus::Failure(err) => {
                return Ok(Err(err));
            }
            FinalExecutionStatus::NotStarted | FinalExecutionStatus::Started => {
                // Wait 1 block and then try again
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

/// Represents a request of type `R` to the JSON RPC, where the response is of type `T`.
/// This type is meant to be constructed by the convenience functions above and used by
/// calling `spawn` (defined below) to execute the request and obtain the response.
pub struct JsonRpcRequest<R, T> {
    inner: R,
    _phantom: PhantomData<T>,
}

/// A wrapper around a JSON RPC query response of type `T`. The wrapper includes the
/// block hash and block height the query was executed at.
pub struct JsonRpcQueryResponse<T> {
    pub data: T,
    pub block_height: BlockHeight,
    pub block_hash: CryptoHash,
}

impl<R, T> JsonRpcRequest<R, T>
where
    R: methods::RpcMethod + Send + 'static,
    T: type_munging::CoerceFrom<R::Response> + Send + 'static,
    R::Error: std::error::Error + Send + Sync,
{
    pub fn spawn<C: client_like::ClientLike>(
        self,
        client: Arc<C>,
    ) -> JoinHandle<anyhow::Result<T>> {
        tokio::task::spawn(async move {
            let response = client.do_call(self.inner).await?;
            T::coerce_from(response)
        })
    }
}

impl<R, T> JsonRpcRequest<R, T>
where
    R: methods::RpcMethod + Send + Sync,
    T: type_munging::CoerceFrom<R::Response>,
    R::Error: std::error::Error + Send + Sync + 'static,
{
    pub async fn execute<C: client_like::ClientLike>(&self, client: &C) -> anyhow::Result<T> {
        let response = client.do_call(&self.inner).await?;
        T::coerce_from(response)
    }
}
