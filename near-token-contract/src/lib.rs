// TODO: Pausable methods.
// TODO: Access Control methods.
// TODO:    Add super-admin with full-access-key control.
use ext::ext_near_token_factory;
use near_contract_standards::fungible_token::events::{FtBurn, FtMint};
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider,
};
use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{env, near_bindgen, BorshStorageKey, PanicOnDefault, Promise, PromiseOrValue};
use near_sdk::{require, AccountId, Gas};

mod ext;

// TODO: Determine properly what are good gas constants for both of these steps.
const GAS_FOR_UNLOCKING_TOKENS: Gas = Gas(10_000_000_000_000);
const GAS_FOR_ON_WITHDRAW: Gas = Gas(10_000_000_000_000 + GAS_FOR_UNLOCKING_TOKENS.0);

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
    pub fn new(metadata: FungibleTokenMetadata) -> Self {
        let factory = env::predecessor_account_id();

        let mut contract = Self {
            factory: factory.clone(),
            token: FungibleToken::new(StorageKeys::FungibleToken),
            metadata,
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
    /// Emit `FtMint` event.
    pub fn deposit_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        // Only the factory can deposit tokens
        self.assert_factory();

        // Mint tokens for the factory
        self.token
            .internal_deposit(&env::predecessor_account_id(), amount.into());

        // Emit minting event
        FtMint {
            owner_id: &receiver_id,
            amount: &amount,
            memo: memo.as_deref(),
        }
        .emit();

        // Call the receiver contract
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
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
            .on_withdraw(receiver_id, amount.into())
    }

    // TODO: Evaluate gas difference between using BytesBase64 vs Borsh
    //       If bytes64 is acceptable we should use it, since it is more
    //       human friendly.
    /// Upgrade the contract to a newer version. This method MUST be
    /// executed only if the predecessor account id is the factory.
    pub fn upgrade_contract(&mut self, binary: near_token_common::BytesBase64) -> Promise {
        // Only the factory can upgrade the contract
        self.assert_factory();

        // Deploy the new contract
        Promise::new(env::current_account_id()).deploy_contract(binary.into())
    }

    /// Update the metadata for the token. ONLY accounts with `ControlMetadata`
    /// role can call this method. In particular it is expected that the factory
    /// has this role. This allows a trustless workflow where metadata can be
    /// updated by any user starting the call from the locker in Aurora.
    pub fn update_metadata(&mut self, metadata: FungibleTokenMetadata) {
        // TODO: Make sure accounts with the proper role can call this function.

        // Update the metadata with the new information.
        self.metadata = metadata;
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

near_contract_standards::impl_fungible_token_core!(Contract, token);
near_contract_standards::impl_fungible_token_storage!(Contract, token);
