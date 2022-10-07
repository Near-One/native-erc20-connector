use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedMap};
use near_sdk::json_types::U128;
use near_sdk::{env, near_bindgen, require, AccountId, BorshStorageKey, PanicOnDefault};
mod ext;

#[derive(BorshDeserialize, BorshSerialize, BorshStorageKey)]
enum StorageKey {
    TokenBinary,
    TokenMap,
}

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize, PanicOnDefault)]
pub struct Contract {
    /// WASM binary of the token contract.
    token_binary: LazyOption<Vec<u8>>,
    /// Version of the token contract.
    token_binary_version: u32,
    /// Iterable map of deployed contracts and their current version.
    tokens: UnorderedMap<AccountId, u32>,
    /// Locker contract ID. It is represented by the NEAR Account ID
    /// representative of the inner Aurora address.
    locker: AccountId,
}

// TODO: Add pausable
// TODO: Add access control
#[near_bindgen]
impl Contract {
    /// Initializes the contract. The locker account id MUST be the NEAR
    /// representative of the Aurora address of the locker contract created
    /// using the Cross Contract Call interface.
    #[init]
    pub fn new(locker: AccountId) -> Self {
        // TODO: Check current_account_id is acceptable
        require!(
            env::current_account_id().as_str().len() + 1 + 40 <= 63,
            "Account ID too large. Impossible to create token subcontracts."
        );

        Self {
            token_binary: LazyOption::new(StorageKey::TokenBinary, None),
            token_binary_version: 0,
            tokens: UnorderedMap::new(StorageKey::TokenMap),
            locker,
        }
    }

    /// Set WASM binary for the token contracts. This increases the token binary version,
    /// so all deployed contracts SHOULD be upgraded after calling this function. ONLY the
    /// `Owner` role can call this method.
    pub fn set_token_binary(&mut self, binary: near_token_common::BytesBase64) {
        // TODO: Only the owner role can call this method.
        self.token_binary.set(&binary.into());
        self.token_binary_version += 1;
    }

    /// Create a new token by deploying the current binary in a sub-account. This method
    /// can only be called by the locker.
    pub fn create_token(&mut self, token_address: aurora_sdk::Address) {
        // TODO: Add metadata to this call
        require!(
            env::predecessor_account_id() == self.locker,
            "Only locker can deploy contracts"
        );

        let _token_account_id = account_id_from_token_address(token_address);

        match self.token_binary.get() {
            None => env::panic_str("Token binary is not set"),
            Some(_binary) => {
                // TODO: Deploy contract
            }
        }
    }

    // TODO: Use borsh for input serialization
    pub fn on_deposit(&mut self, receiver_id: AccountId, amount: U128) {
        todo!();
    }

    /// Method invoked by each individual token when an account id calls `withdraw`.
    /// This method is called when tokens are already burned from the token contracts.
    /// The locker in Aurora is called to unlock the equivalent amount of tokens on
    /// the receiver_id account.
    ///
    /// It is important that this method and the next method don't fail, otherwise this
    /// might result in the loss of tokens (in case the tokens are burnt but not unlocked).
    pub fn on_withdraw(&mut self, _receiver_id: aurora_sdk::Address, _amount: u128) {
        todo!();
    }
}

/// Convert Aurora address of an ERC-20 to the NEAR account ID NEP-141 representative.
fn account_id_from_token_address(address: aurora_sdk::Address) -> AccountId {
    format!("{}.{}", address, env::current_account_id())
        .parse()
        .unwrap()
}
