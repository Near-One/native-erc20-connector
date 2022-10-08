use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedMap};
use near_sdk::{
    env, near_bindgen, require, AccountId, Balance, BorshStorageKey, Gas, PanicOnDefault, Promise,
};
mod ext;

const TOKEN_STORAGE_DEPOSIT_COST: Balance = 1_000_000_000_000_000_000;
const TOKEN_DEPLOYMENT_COST: Gas = Gas(5_000_000_000_000);
const DEPOSIT_COST: Gas = Gas(2_000_000_000_000);

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
    pub fn create_token(&mut self, token_address: near_token_common::sdk::Address) -> Promise {
        require!(
            env::predecessor_account_id() == self.locker,
            "Only locker can deploy contracts"
        );

        let token_account_id = account_id_from_token_address(token_address);

        match self.token_binary.get() {
            None => env::panic_str("Token binary is not set"),
            Some(_binary) => Promise::new(token_account_id)
                .create_account()
                .deploy_contract(self.token_binary.get().unwrap())
                .function_call(
                    "new".to_string(),
                    vec![],
                    TOKEN_STORAGE_DEPOSIT_COST,
                    TOKEN_DEPLOYMENT_COST,
                ),
        }
    }

    /// Method called by the locker when new tokens were deposited. The same amount of
    /// tokens is minted in the equivalent NEP-141 contract. If such contract doesn't
    /// exist it is deployed.
    #[payable]
    pub fn on_deposit(
        &mut self,
        #[serializer(borsh)] token: near_token_common::sdk::Address,
        #[serializer(borsh)] receiver_id: AccountId,
        #[serializer(borsh)] amount: u128,
    ) -> Promise {
        require!(
            env::predecessor_account_id() == self.locker,
            "Only locker can deploy contracts"
        );

        let token_account_id = account_id_from_token_address(token);

        if self.tokens.get(&token_account_id).is_none() {
            Promise::new(token_account_id)
                .create_account()
                .deploy_contract(self.token_binary.get().unwrap())
                .function_call(
                    "new".to_string(),
                    vec![],
                    TOKEN_STORAGE_DEPOSIT_COST,
                    TOKEN_DEPLOYMENT_COST,
                )
                .function_call(
                    "deposit".to_string(),
                    near_sdk::serde_json::json!({
                        "receiver_id": receiver_id,
                        "amount": amount,
                    })
                    .to_string()
                    .into_bytes(),
                    0,
                    DEPOSIT_COST,
                )
        } else {
            ext::ext_near_token::ext(token_account_id)
                .with_static_gas(DEPOSIT_COST)
                .deposit(receiver_id, amount.into(), None)
        }
    }

    /// Method invoked by each individual token when an account id calls `withdraw`.
    /// This method is called when tokens are already burned from the token contracts.
    /// The locker in Aurora is called to unlock the equivalent amount of tokens on
    /// the receiver_id account.
    ///
    /// It is important that this method and the next method don't fail, otherwise this
    /// might result in the loss of tokens (in case the tokens are burnt but not unlocked).
    pub fn on_withdraw(&mut self, _receiver_id: near_token_common::sdk::Address, _amount: u128) {
        todo!();
    }
}

/// Convert Aurora address of an ERC-20 to the NEAR account ID NEP-141 representative.
fn account_id_from_token_address(address: near_token_common::sdk::Address) -> AccountId {
    format!("{}.{}", address, env::current_account_id())
        .parse()
        .unwrap()
}
