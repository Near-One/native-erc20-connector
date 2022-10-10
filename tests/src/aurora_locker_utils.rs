use aurora_engine_types::types::Address;

pub struct AuroraLocker {
    pub address: Address,
    pub abi: ethabi::Contract,
}
