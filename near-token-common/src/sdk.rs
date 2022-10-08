use std::fmt::Display;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, BorshDeserialize, BorshSerialize)]
pub struct Address([u8; 20]);

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}
