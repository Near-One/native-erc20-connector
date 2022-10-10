use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub enum CallArgs {
    V2(FunctionCallArgs),
    /// Legacy variant. Not supported by sdk, but present here for future compatibility with new variants.
    NotSupported,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct FunctionCallArgs {
    pub contract: Address,
    pub value: WeiU256,
    pub input: Vec<u8>,
}

impl From<FunctionCallArgs> for CallArgs {
    fn from(args: FunctionCallArgs) -> Self {
        CallArgs::V2(args)
    }
}

impl From<CallArgs> for FunctionCallArgs {
    fn from(args: CallArgs) -> Self {
        match args {
            CallArgs::V2(args) => args,
            CallArgs::NotSupported => near_sdk::env::panic_str("ERR_LEGACY_VARIANT_NOT_SUPPORTED"),
        }
    }
}

pub type WeiU256 = [u8; 32];

#[derive(Serialize, Deserialize, BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct Address(pub [u8; 20]);

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

impl From<[u8; 20]> for Address {
    fn from(address: [u8; 20]) -> Self {
        Self(address)
    }
}
