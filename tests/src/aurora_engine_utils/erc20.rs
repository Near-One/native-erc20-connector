//! Convenience data-types and functions for deploying/interacting with the OpenZeppelin
//! ERC-20 contract: https://docs.openzeppelin.com/contracts/4.x/erc20#Presets

use crate::aurora_engine_utils::ContractInput;
use aurora_engine_types::{types::Address, U256};
use std::path::Path;

const CONTRACT_NAME: &str = "ERC20PresetMinterPauser";

pub struct Constructor {
    pub code: Vec<u8>,
    pub abi: ethabi::Contract,
}

impl Constructor {
    pub async fn load() -> anyhow::Result<Self> {
        let res = Path::new("res");
        let code_hex =
            tokio::fs::read_to_string(res.join(format!("{}.bin", CONTRACT_NAME))).await?;
        let code = hex::decode(code_hex)?;
        let abi = {
            let abi_bytes = tokio::fs::read(res.join(format!("{}.abi", CONTRACT_NAME))).await?;
            serde_json::from_slice(&abi_bytes)?
        };
        Ok(Self { code, abi })
    }

    pub fn deploy_code(&self, name: &str, symbol: &str) -> Vec<u8> {
        // Unwraps are safe because we statically know there is a constructor and it
        // takes two strings as input.
        self.abi
            .constructor()
            .unwrap()
            .encode_input(
                self.code.clone(),
                &[
                    ethabi::Token::String(name.to_string()),
                    ethabi::Token::String(symbol.to_string()),
                ],
            )
            .unwrap()
    }
}

pub struct ERC20 {
    pub abi: ethabi::Contract,
    pub address: Address,
}

impl ERC20 {
    pub fn mint(&self, recipient: Address, amount: U256) -> ContractInput {
        let data = self
            .abi
            .function("mint")
            .unwrap()
            .encode_input(&[
                ethabi::Token::Address(recipient.raw()),
                ethabi::Token::Uint(amount),
            ])
            .unwrap();
        ContractInput(data)
    }

    pub fn balance_of(&self, address: Address) -> ContractInput {
        let data = self
            .abi
            .function("balanceOf")
            .unwrap()
            .encode_input(&[ethabi::Token::Address(address.raw())])
            .unwrap();
        ContractInput(data)
    }

    pub fn approve(&self, spender: Address, amount: U256) -> ContractInput {
        let data = self
            .abi
            .function("approve")
            .unwrap()
            .encode_input(&[
                ethabi::Token::Address(spender.raw()),
                ethabi::Token::Uint(amount),
            ])
            .unwrap();
        ContractInput(data)
    }
}

pub trait ERC20DeployedAt {
    fn deployed_at(self, address: Address) -> ERC20;
}

impl ERC20DeployedAt for Constructor {
    fn deployed_at(self, address: Address) -> ERC20 {
        ERC20 {
            abi: self.abi,
            address,
        }
    }
}
