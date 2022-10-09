use near_sdk::borsh;
use near_sdk::ext_contract;

use crate::types::CallArgs;

#[ext_contract(ext_aurora)]
pub trait Aurora {
    fn call(&mut self, #[serializer(borsh)] args: CallArgs);
}

pub fn call_args(to: crate::Address, input: Vec<u8>) -> CallArgs {
    CallArgs::V2(crate::FunctionCallArgs {
        contract: to,
        value: Default::default(),
        input,
    })
}
