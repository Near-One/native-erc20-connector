use near_sdk::ext_contract;

#[ext_contract(ext_near_token)]
pub trait ExtNearToken {
    fn upgrade_contract(&mut self, binary: near_sdk::json_types::Base64VecU8);

    fn deposit(
        &mut self,
        receiver_id: near_sdk::AccountId,
        amount: near_sdk::json_types::U128,
        memo: Option<String>,
    );
}
