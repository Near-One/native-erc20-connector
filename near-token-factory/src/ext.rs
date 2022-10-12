use near_sdk::ext_contract;
use near_token_common as aurora_sdk;

#[ext_contract(ext_near_token)]
pub trait ExtNearToken {
    fn upgrade_contract(&mut self, binary: near_sdk::json_types::Base64VecU8);

    fn deposit(
        &mut self,
        receiver_id: near_sdk::AccountId,
        amount: near_sdk::json_types::U128,
        memo: Option<String>,
    );

    fn update_metadata(&mut self, metadata: aurora_sdk::UpdateFungibleTokenMetadata);
}
