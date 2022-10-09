use near_sdk::ext_contract;

#[ext_contract(ext_near_token_factory)]
pub trait ExtNearTokenFactory {
    fn on_withdraw(
        &mut self,
        receiver_id: near_token_common::Address,
        amount: near_sdk::json_types::U128,
    );
}
