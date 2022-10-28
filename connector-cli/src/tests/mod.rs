use near_primitives::views;

mod create_token;
mod deploy;

pub fn default_transaction() -> views::SignedTransactionView {
    views::SignedTransactionView {
        signer_id: "signer.near".parse().unwrap(),
        public_key: near_crypto::PublicKey::empty(near_crypto::KeyType::ED25519),
        nonce: 0,
        receiver_id: "receiver.near".parse().unwrap(),
        actions: Vec::new(),
        signature: near_crypto::Signature::empty(near_crypto::KeyType::ED25519),
        hash: Default::default(),
    }
}

pub fn default_transaction_outcome() -> views::ExecutionOutcomeWithIdView {
    views::ExecutionOutcomeWithIdView {
        proof: Vec::new(),
        block_hash: Default::default(),
        id: Default::default(),
        outcome: views::ExecutionOutcomeView {
            logs: Vec::new(),
            receipt_ids: Vec::new(),
            gas_burnt: 0,
            tokens_burnt: 0,
            executor_id: "executor.near".parse().unwrap(),
            status: views::ExecutionStatusView::SuccessValue(Vec::new()),
            metadata: views::ExecutionMetadataView {
                version: 0,
                gas_profile: None,
            },
        },
    }
}
