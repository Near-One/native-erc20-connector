use ext::ext_near_token_factory;
use near_contract_standards::fungible_token::events::{FtBurn, FtMint};
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider,
};
use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{
    assert_self, env, near_bindgen, BorshStorageKey, PanicOnDefault, Promise, PromiseOrValue,
};
use near_sdk::{require, AccountId, Gas};
use near_token_common as aurora_sdk;

mod ext;

// TODO: Determine properly what are good gas constants for both of these steps.
const GAS_FOR_UNLOCKING_TOKENS: Gas = Gas(10_000_000_000_000);
const GAS_FOR_ON_WITHDRAW: Gas = Gas(10_000_000_000_000 + GAS_FOR_UNLOCKING_TOKENS.0);

macro_rules! maybe_update_metadata {
    ($self:ident, $field_name:ident) => {
        if let Some($field_name) = $field_name {
            $self.metadata.$field_name = $field_name
        }
    };
}

macro_rules! maybe_update_optional_metadata {
    ($self:ident, $field_name:ident) => {
        if let Some($field_name) = $field_name {
            $self.metadata.$field_name = Some($field_name)
        }
    };
}

#[derive(BorshDeserialize, BorshSerialize, BorshStorageKey)]
enum StorageKeys {
    FungibleToken,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Account id of the factory determined at deployment time.
    factory: AccountId,
    /// Interface that implements NEP-141 Fungible Token standard.
    token: FungibleToken,
    /// Metadata for the token.
    metadata: FungibleTokenMetadata,
}

// TODO: Pausable methods.
// TODO: Access Control methods.
// TODO:    Add super-admin with full-access-key control.
#[near_bindgen]
impl Contract {
    /// Initializes the contract. This function must be called exactly once
    /// by the factory during the contract deployment. Moreover deployment
    /// and initialization MUST happen atomically in a batched transaction,
    /// to make sure the factory and other input arguments are properly set.
    ///
    /// Method is payable since the factory needs to pay the storage to be
    /// registered automatically.
    #[init]
    #[payable]
    pub fn new() -> Self {
        let factory = env::predecessor_account_id();

        let mut contract = Self {
            factory: factory.clone(),
            token: FungibleToken::new(StorageKeys::FungibleToken),
            metadata: default_metadata(),
        };

        // Automatically register the factory as a minter.
        contract.token.storage_deposit(Some(factory), None);

        contract
    }

    /// Mint new tokens sent from Aurora to the `receiver_id`. It increases
    /// the total supply since new tokens are minted. This method MUST be
    /// executed only if the predecessor account id is the factory.
    ///
    /// Emit `FtMint` event.
    #[payable]
    pub fn deposit(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        // Only the factory can deposit tokens
        self.assert_factory();

        // Mint exact amount of tokens for the receiver
        self.token.internal_deposit(&receiver_id, amount.into());

        // Emit minting event
        FtMint {
            owner_id: &receiver_id,
            amount: &amount,
            memo: memo.as_deref(),
        }
        .emit();
    }

    /// Similar to `ft_transfer_call`. Allows the user to transfer from
    /// Aurora to NEAR contract and immediately call a method on the
    /// NEAR contract.
    ///
    /// Mint new tokens sent from Aurora to the `receiver_id`. It increases
    /// the total supply since new tokens are minted. This method MUST be
    /// executed only if the predecessor account id is the factory.
    ///
    /// Tokens are initially minted to the factory. They are automatically
    /// transferred to the `receiver_id` using `ft_transfer_call` method.
    /// In the and `deposit_resolve` is called, where unused tokens are burnt
    /// to refund the original sender.
    ///
    /// Emit FtMint event, FtTransfer event, and potentially FtBurn event (in
    /// case refund is required).
    pub fn deposit_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> Promise {
        // Only the factory can deposit tokens
        self.assert_factory();

        // Mint tokens for the factory
        self.token.internal_deposit(&self.factory, amount.into());

        // Emit minting event
        FtMint {
            owner_id: &self.factory,
            amount: &amount,
            memo: memo.as_deref(),
        }
        .emit();

        // Call the receiver contract
        let promise_or_value = self.token.ft_transfer_call(receiver_id, amount, memo, msg);

        // `ft_transfer_call` always returns a promise, so it is safe to unwrap it.
        unwrap_promise(promise_or_value)
            .then(Contract::ext(env::current_account_id()).deposit_resolve(amount))
    }

    /// Callback that is called in the end of `deposit_call` method. All unused tokens
    /// will be sent back to the original sender in Aurora. Tokens are immediately burnt
    /// from this contract, and the amount is passed to the factory on the result, which
    /// is passed to the Aurora Locker contract. The locker contract MUST unlock the
    /// tokens in the same transactions. This is a callback function that can be only
    /// executed from the contract itself.
    ///
    /// Return the amount of unused tokens. Emit `FtBurn` event if refund amount is non-zero.
    pub fn deposit_resolve(&mut self, amount: U128, #[callback_unwrap] used_amount: U128) -> U128 {
        // Only the contract itself can call this method.
        assert_self();

        if amount != used_amount {
            let amount: u128 = amount.into();
            let used_amount = used_amount.into();

            require!(
                amount > used_amount,
                "Used amount is greater than the total amount"
            );

            // Burn the tokens that were minted for the factory.
            let refund_amount = amount - used_amount;
            self.token.internal_withdraw(&self.factory, refund_amount);

            // Emit burning event
            let refund_amount = U128::from(refund_amount);
            FtBurn {
                owner_id: &env::predecessor_account_id(),
                amount: &refund_amount,
                memo: Some("Refund unused tokens from deposit_call"),
            }
            .emit();

            refund_amount
        } else {
            // If all tokens were used, refund zero tokens.
            U128::from(0)
        }
    }

    /// Burn tokens owned by the predecessor account id, and unlock the equivalent
    /// amount on Aurora for `receiver_id`. It decreases the total supply. Anyone
    /// can call this method, including other contracts.
    ///
    /// Emit `FtBurn` event.
    pub fn withdraw(
        &mut self,
        receiver_id: aurora_sdk::Address,
        amount: U128,
        memo: Option<String>,
    ) -> Promise {
        // Burn tokens from the factory
        self.token
            .internal_withdraw(&env::predecessor_account_id(), amount.into());

        // Emit burning event
        FtBurn {
            owner_id: &env::predecessor_account_id(),
            amount: &amount,
            memo: memo.as_deref(),
        }
        .emit();

        ext_near_token_factory::ext(self.factory.clone())
            .with_static_gas(GAS_FOR_ON_WITHDRAW)
            .on_withdraw(receiver_id, amount)
    }

    /// Upgrade the contract to a newer version. This method MUST be
    /// executed only if the predecessor account id is the factory.
    pub fn upgrade_contract(&mut self, binary: near_sdk::json_types::Base64VecU8) -> Promise {
        // Only the factory can upgrade the contract
        self.assert_factory();

        // Deploy the new contract
        Promise::new(env::current_account_id()).deploy_contract(binary.into())
    }

    /// Update the metadata for the token. ONLY accounts with `ControlMetadata`
    /// role can call this method. In particular it is expected that the factory
    /// has this role. This allows a trustless workflow where metadata can be
    /// updated by any user starting the call from the locker in Aurora.
    pub fn update_metadata(&mut self, metadata: aurora_sdk::UpdateFungibleTokenMetadata) {
        // TODO: Make sure accounts with the proper role can call this function.
        self.assert_factory();

        let aurora_sdk::UpdateFungibleTokenMetadata {
            name,
            symbol,
            icon,
            reference,
            reference_hash,
            decimals,
        } = metadata;

        // Update only parts of the metadata that were specified.
        maybe_update_metadata!(self, name);
        maybe_update_metadata!(self, symbol);
        maybe_update_optional_metadata!(self, icon);
        maybe_update_optional_metadata!(self, reference);
        maybe_update_optional_metadata!(self, reference_hash);
        maybe_update_metadata!(self, decimals);
    }

    /// Triggers call in ERC20 Locker contract on Aurora to update the metadata of
    /// this contract. This method is public and can be called by any user. The effect
    /// of this method is that the fields "name", "symbol" and "decimals" are updated.
    /// Other fields remain unchanged.
    pub fn pull_metadata(&mut self) -> Promise {
        todo!()
    }
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    /// Returns the metadata for the token.
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.clone()
    }
}

impl Contract {
    fn assert_factory(&self) {
        require!(
            env::predecessor_account_id() == self.factory,
            "Only factory can call this method"
        );
    }
}

fn unwrap_promise<T>(promise_or_value: PromiseOrValue<T>) -> near_sdk::Promise {
    match promise_or_value {
        PromiseOrValue::Promise(promise) => promise,
        PromiseOrValue::Value(_) => panic!("Expected promise, got value"),
    }
}

near_contract_standards::impl_fungible_token_core!(Contract, token);
near_contract_standards::impl_fungible_token_storage!(Contract, token);

fn default_metadata() -> FungibleTokenMetadata {
    FungibleTokenMetadata {
        spec: "ft-1.0.0".to_string(),
        name: "".to_string(),
        symbol: "".to_string(),
        icon: None,
        reference: None,
        reference_hash: None,
        decimals: 0,
    }
}
