use near_plugins::{access_control, access_control_any, AccessControlRole, AccessControllable};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedMap};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, near_bindgen, require, AccountId, Balance, BorshStorageKey, Gas, PanicOnDefault, Promise,
};
use near_token_common as aurora_sdk;
mod ext;

const NEW_TOKEN_DEPOSIT_COST: Balance = 3_000_000_000_000_000_000_000_000;
const TOKEN_STORAGE_DEPOSIT_COST: Balance = 1_250_000_000_000_000_000_000;
const TOKEN_DEPLOYMENT_COST: Gas = Gas(5_000_000_000_000);
const DEPOSIT_COST: Gas = Gas(5_000_000_000_000);
const UPDATE_METADATA_COST: Gas = Gas(5_000_000_000_000);

const ERR_ONLY_LOCKER: &str = "ERR_ONLY_LOCKER: Only locker can call this method.";
const ERR_INVALID_ACCOUNT: &str =
    "ERR_INVALID_ACCOUNT: Account ID too large. Impossible to create token subcontracts.";
const ERR_BINARY_NOT_AVAILABLE: &str = "ERR_BINARY_NOT_AVAILABLE: Token binary is not set.";
const ERR_TOKEN_NOT_REGISTERED: &str = "ERR_TOKEN_NOT_REGISTERED: Token is not registered.";

pub const WITHDRAW_SELECTOR: [u8; 4] = [0xd9, 0xca, 0xed, 0x12];

#[derive(BorshDeserialize, BorshSerialize, BorshStorageKey)]
enum StorageKey {
    TokenBinary,
    TokenMap,
}

#[derive(AccessControlRole, Deserialize, Serialize, Copy, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum AclRole {
    /// Accounts with this role can replace the token binary that will be used for new tokens.
    TokenBinaryUpdater,
    /// Accounts with this role can replace the super admin that will be used for new tokens.
    TokenControllerUpdater,
}

#[access_control(role_type(AclRole))]
#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize, PanicOnDefault)]
pub struct Contract {
    /// Account id of the engine. It is expected to be `aurora`.
    aurora: AccountId,
    /// Account id that will be used as super admin for all deployed tokens.
    token_super_admin: AccountId,
    /// WASM binary of the token contract.
    token_binary: LazyOption<Vec<u8>>,
    /// Version of the token contract.
    token_binary_version: u32,
    /// Iterable map of deployed contracts and their current version.
    tokens: UnorderedMap<AccountId, u32>,
    /// Address of the locker in aurora.
    locker: aurora_sdk::Address,
}

// TODO: Add pausable
#[near_bindgen]
impl Contract {
    /// Initializes the contract. The locker account id MUST be the NEAR
    /// representative of the Aurora address of the locker contract created
    /// using the Cross Contract Call interface.
    #[init]
    pub fn new(
        aurora: AccountId,
        locker: aurora_sdk::Address,
        super_admin: Option<AccountId>,
    ) -> Self {
        require!(
            env::current_account_id().as_str().len() + 1 + 40 <= 63,
            ERR_INVALID_ACCOUNT
        );

        // If not specified, the super_admin is the deployer of this contract.
        let super_admin = super_admin.unwrap_or_else(env::predecessor_account_id);

        let mut contract = Self {
            aurora,
            token_super_admin: super_admin.clone(),
            token_binary: LazyOption::new(StorageKey::TokenBinary, None),
            token_binary_version: 0,
            tokens: UnorderedMap::new(StorageKey::TokenMap),
            locker,
            __acl: Default::default(),
        };

        // Initialize Acl permissions.
        require!(
            contract.acl_init_super_admin(super_admin.clone()),
            "Failed to add factory as initial acl super-admin",
        );

        require!(contract
            .acl_grant_role(AclRole::TokenBinaryUpdater.into(), super_admin.clone())
            .unwrap_or_default());

        require!(contract
            .acl_grant_role(AclRole::TokenControllerUpdater.into(), super_admin)
            .unwrap_or_default());

        contract
    }

    /// Set WASM binary for the token contracts. This increases the token binary version,
    /// so all deployed contracts SHOULD be upgraded after calling this function. ONLY the
    /// `Owner` role can call this method.
    #[access_control_any(roles(AclRole::TokenBinaryUpdater))]
    pub fn set_token_binary(&mut self, binary: near_sdk::json_types::Base64VecU8) {
        self.token_binary.set(&binary.into());
        self.token_binary_version += 1;
    }

    /// Replace the account id that will be admin for all new tokens. This has no effect on
    /// tokens that were already deployed.
    #[access_control_any(roles(AclRole::TokenControllerUpdater))]
    pub fn replace_token_admin(&mut self, new_admin: AccountId) {
        self.token_super_admin = new_admin;
    }

    /// Get the most recent binary version or fails if no binary is available.
    fn get_token_binary(&self) -> Vec<u8> {
        match self.token_binary.get() {
            None => env::panic_str(ERR_BINARY_NOT_AVAILABLE),
            Some(binary) => binary,
        }
    }

    /// Create a new token by deploying the current binary in a sub-account. This method
    /// can only be called by the locker.
    #[payable]
    pub fn create_token(
        &mut self,
        #[serializer(borsh)] token_address: aurora_sdk::Address,
    ) -> Promise {
        self.assert_locker();

        let token_account_id = account_id_from_token_address(token_address);
        let binary = self.get_token_binary();

        self.tokens
            .insert(&token_account_id, &self.token_binary_version);

        Promise::new(token_account_id)
            .create_account()
            .transfer(NEW_TOKEN_DEPOSIT_COST)
            .deploy_contract(binary)
            .function_call(
                "new".to_string(),
                near_sdk::serde_json::json!({
                    "super_admin": self.token_super_admin,
                })
                .to_string()
                .into_bytes(),
                TOKEN_STORAGE_DEPOSIT_COST,
                TOKEN_DEPLOYMENT_COST,
            )
    }

    /// Method called by the locker when new tokens were deposited. The same amount of
    /// tokens is minted in the equivalent NEP-141 contract. If such contract doesn't
    /// exist it is deployed.
    #[payable]
    pub fn on_deposit(
        &mut self,
        #[serializer(borsh)] token: aurora_sdk::Address,
        #[serializer(borsh)] receiver_id: AccountId,
        #[serializer(borsh)] amount: u128,
    ) -> Promise {
        self.assert_locker();

        let token_account_id = account_id_from_token_address(token);

        require!(
            self.tokens.get(&token_account_id).is_some(),
            ERR_TOKEN_NOT_REGISTERED
        );

        ext::ext_near_token::ext(token_account_id)
            .with_static_gas(DEPOSIT_COST)
            .deposit(receiver_id, amount.into(), None)
    }

    /// Method invoked by each individual token when an account id calls `withdraw`.
    /// This method is called when tokens are already burned from the token contracts.
    /// The locker in Aurora is called to unlock the equivalent amount of tokens on
    /// the receiver_id account.
    ///
    /// It is important that this method and the next method don't fail, otherwise this
    /// might result in the loss of tokens (in case the tokens are burnt but not unlocked).
    ///
    /// This is a public method with no access control. However calling will only grant
    /// withdraw privileges to the token associated with the caller if any. If the caller
    /// is not a previously deployed token, this method will fail.
    pub fn on_withdraw(
        &mut self,
        receiver_id: aurora_sdk::Address,
        amount: near_sdk::json_types::U128,
    ) -> Promise {
        let token_id = address_from_token_account_id(env::predecessor_account_id());

        let input = abi_encode_withdraw(&token_id, &receiver_id, amount.into());

        aurora_sdk::aurora::ext_aurora::ext(self.aurora.clone())
            .call(aurora_sdk::aurora::call_args(self.locker.clone(), input))
    }

    /// Representative account id of the locker in Aurora.
    pub fn locker_account_id(&self) -> AccountId {
        format!("{}.{}", self.locker, self.aurora).parse().unwrap()
    }

    /// Method that allows updating the metadata of a particular token. This method can only
    /// be called by the locker.
    pub fn update_token_metadata(
        &mut self,
        #[serializer(borsh)] token: aurora_sdk::Address,
        #[serializer(borsh)] metadata: ERC20Metadata,
    ) -> Promise {
        self.assert_locker();

        let token_account_id = account_id_from_token_address(token);

        if self.tokens.get(&token_account_id).is_none() {
            env::panic_str(ERR_TOKEN_NOT_REGISTERED);
        }

        ext::ext_near_token::ext(token_account_id)
            .with_static_gas(UPDATE_METADATA_COST)
            .update_metadata(aurora_sdk::UpdateFungibleTokenMetadata {
                name: Some(metadata.name),
                symbol: Some(metadata.symbol),
                decimals: Some(metadata.decimals),
                ..Default::default()
            })
    }
}

impl Contract {
    fn assert_locker(&self) {
        require!(
            env::predecessor_account_id() == self.locker_account_id(),
            ERR_ONLY_LOCKER
        );
    }
}

/// Convert Aurora address of an ERC-20 to the NEAR account ID NEP-141 representative.
fn account_id_from_token_address(address: aurora_sdk::Address) -> AccountId {
    format!("{}.{}", address, env::current_account_id())
        .parse()
        .unwrap()
}

/// Convert a NEAR account ID NEP-141 representative to the Aurora address of an ERC-20.
fn address_from_token_account_id(account_id: AccountId) -> aurora_sdk::Address {
    let mut buffer = [0u8; 20];
    hex::decode_to_slice(&account_id.as_bytes()[0..40], &mut buffer).unwrap();
    buffer.into()
}

/// Manual implementation of abi encoding for efficiency.
fn abi_encode_withdraw(
    token_id: &aurora_sdk::Address,
    receiver_id: &aurora_sdk::Address,
    amount: u128,
) -> Vec<u8> {
    let mut buffer = [0u8; 4 + 32 + 32 + 32];
    buffer[0..4].copy_from_slice(&WITHDRAW_SELECTOR);
    buffer[16..36].copy_from_slice(&token_id.0);
    buffer[48..68].copy_from_slice(&receiver_id.0);
    buffer[84..100].copy_from_slice(&amount.to_be_bytes());
    buffer.to_vec()
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize)]
pub struct ERC20Metadata {
    name: String,
    symbol: String,
    decimals: u8,
}

#[cfg(test)]
mod tests {
    use crate::aurora_sdk::Address;
    use crate::{abi_encode_withdraw, WITHDRAW_SELECTOR};

    #[test]
    /// Check withdraw selector is properly computed. Function signature is:
    /// "withdraw(address,address,uint256)"
    fn test_withdraw_select() {
        assert_eq!(
            &ethabi::short_signature(
                "withdraw",
                &[
                    ethabi::ParamType::Address,
                    ethabi::ParamType::Address,
                    ethabi::ParamType::Uint(256)
                ],
            ),
            &WITHDRAW_SELECTOR
        );
    }

    #[test]
    fn test_abi_encode_withdraw() {
        let token_id = [
            12, 34, 56, 78, 90, 12, 34, 56, 78, 90, 12, 34, 56, 78, 90, 12, 34, 56, 78, 90,
        ];
        let receiver_id = [
            12, 12, 12, 12, 34, 34, 34, 34, 56, 56, 56, 56, 78, 78, 78, 78, 90, 90, 90, 90,
        ];
        let amount = 0x998877665544332211u128;

        assert_eq!(
            &abi_encode_withdraw(&Address(token_id), &Address(receiver_id), amount,)[4..],
            &ethabi::encode(&[
                ethabi::Token::Address(ethabi::Address::try_from(&token_id).unwrap()),
                ethabi::Token::Address(ethabi::Address::try_from(&receiver_id).unwrap()),
                ethabi::Token::Uint(ethabi::Uint::from(amount)),
            ])
        );
    }
}
