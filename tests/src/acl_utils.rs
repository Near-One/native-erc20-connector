use near_sdk::serde::de::DeserializeOwned;
use near_sdk::serde_json::{self, json};
use std::cmp::PartialEq;
use std::fmt::Debug;
use workspaces::result::{ExecutionFinalResult, ExecutionSuccess};
use workspaces::{Account, AccountId, Contract};

#[derive(Debug)]
pub enum AclTxOutcome {
    Success(ExecutionSuccess),
    AclFailure(AclFailure),
}

#[derive(Debug)]
pub struct AclFailure {
    method_name: String,
    /// The result of the transaction. Not allowing view calls here since
    /// `ViewResultDetails` is not sufficient to verify ACL failure.
    result: ExecutionFinalResult,
}

impl AclTxOutcome {
    /// Asserts the transaction was successful and returned `()`.
    pub fn assert_success_unit_return(&self) {
        let res = match self {
            AclTxOutcome::Success(res) => res,
            AclTxOutcome::AclFailure(failure) => panic!(
                "Expected transaction success but it failed with {:?}",
                failure,
            ),
        };
        assert!(
            res.raw_bytes().unwrap().is_empty(),
            "Unexpected return value",
        );
    }

    /// Asserts the transaction was successful and returned the `expected`
    /// value.
    pub fn assert_success_return_value<T>(&self, expected: T)
    where
        T: DeserializeOwned + PartialEq + Debug,
    {
        let actual = match self {
            AclTxOutcome::Success(res) => res.json::<T>().unwrap(),
            AclTxOutcome::AclFailure(failure) => panic!(
                "Expected transaction success but it failed with {:?}",
                failure,
            ),
        };
        assert_eq!(actual, expected);
    }

    pub fn assert_acl_failure(&self) {
        let failure = match self {
            AclTxOutcome::Success(_) => panic!("Expected transaction failure"),
            AclTxOutcome::AclFailure(failure) => failure,
        };
        assert_insufficient_acl_permissions(failure.result.clone(), failure.method_name.as_str());
    }
}

/// Asserts transaction failure due to insufficient `AccessControllable` (ACL)
/// permissions.
fn assert_insufficient_acl_permissions(res: ExecutionFinalResult, method: &str) {
    let err = res
        .into_result()
        .expect_err("Transaction should have failed");
    let err = format!("{}", err);

    let must_contain = format!(
        "Insufficient permissions for method {} restricted by access control.",
        method,
    );

    assert!(
        err.contains(&must_contain),
        "'{}' is not contained in '{}'",
        must_contain,
        err,
    );
}

pub async fn call_access_controlled_method(
    caller: &Account,
    contract: &Contract,
    method: &str,
    args: serde_json::Value,
) -> anyhow::Result<AclTxOutcome> {
    let res = caller
        .call(contract.id(), method)
        .args_json(args)
        .max_gas()
        .transact()
        .await?;
    let tx_outcome = match res.is_success() {
        true => AclTxOutcome::Success(res.into_result()?),
        false => AclTxOutcome::AclFailure(AclFailure {
            method_name: method.to_string(),
            result: res,
        }),
    };
    Ok(tx_outcome)
}

pub async fn call_acl_has_role(
    contract: &Contract,
    role: &str,
    account_id: &AccountId,
) -> anyhow::Result<bool> {
    let res = contract
        .call("acl_has_role")
        .args_json(json!({
            "role": role,
            "account_id": account_id,
        }))
        .view()
        .await?;
    Ok(res.json::<bool>()?)
}
