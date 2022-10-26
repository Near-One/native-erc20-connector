//! This module contains a convenience trait for mapping JSON RPC query response types to
//! more useful ones. For example, all `RpcQueryRequest` inputs return the same `RpcQueryResponse`
//! which has a `kind` field with an enum. Naturally, there is a correspondence between the kind
//! of request and the kind of response, but the interface provided by `near_jsonrpc_client` does
//! not reflect this, leading to unnecessary matches in code. To avoid this, the trait in this
//! module allows automatically mapping `RpcQueryResponse` types to the real value they contain.
//! This enables the strongly-types interface exposed in the parent module.

use super::JsonRpcQueryResponse;
use near_jsonrpc_client::methods;
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::{
    hash::CryptoHash,
    views::{AccessKeyView, ContractCodeView},
};

pub trait CoerceFrom<R>: private::Sealed
where
    Self: Sized,
{
    fn coerce_from(response: R) -> anyhow::Result<Self>;
}

impl CoerceFrom<methods::query::RpcQueryResponse> for JsonRpcQueryResponse<AccessKeyView> {
    fn coerce_from(response: methods::query::RpcQueryResponse) -> anyhow::Result<Self> {
        let data = match response.kind {
            QueryResponseKind::AccessKey(info) => Ok(info),
            _ => Err(anyhow::Error::msg("Unexpected QueryResponseKind")),
        }?;
        Ok(JsonRpcQueryResponse {
            data,
            block_height: response.block_height,
            block_hash: response.block_hash,
        })
    }
}

impl CoerceFrom<methods::query::RpcQueryResponse> for JsonRpcQueryResponse<ContractCodeView> {
    fn coerce_from(response: methods::query::RpcQueryResponse) -> anyhow::Result<Self> {
        let data = match response.kind {
            QueryResponseKind::ViewCode(code_view) => Ok(code_view),
            _ => Err(anyhow::Error::msg("Unexpected QueryResponseKind")),
        }?;
        Ok(JsonRpcQueryResponse {
            data,
            block_height: response.block_height,
            block_hash: response.block_hash,
        })
    }
}

impl CoerceFrom<methods::broadcast_tx_async::RpcBroadcastTxAsyncResponse> for CryptoHash {
    fn coerce_from(
        response: methods::broadcast_tx_async::RpcBroadcastTxAsyncResponse,
    ) -> anyhow::Result<Self> {
        Ok(response)
    }
}

mod private {
    use super::*;
    pub trait Sealed {}
    impl Sealed for AccessKeyView {}
    impl Sealed for ContractCodeView {}
    impl Sealed for CryptoHash {}
    impl<T: Sealed> Sealed for JsonRpcQueryResponse<T> {}
}
