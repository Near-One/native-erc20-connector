use crate::{aurora_engine_utils, aurora_locker_utils, nep141_utils, wnear_utils::Wnear};
use aurora_engine::parameters::{CallArgs, FunctionCallArgsV2, SubmitResult};
use aurora_engine_precompiles::xcc::cross_contract_call;
use aurora_engine_types::{
    parameters::{CrossContractCallArgs, PromiseArgs, PromiseCreateArgs, PromiseWithCallbackArgs},
    types::{Address, NearGas, PromiseResult, Wei, Yocto},
};
use borsh::BorshSerialize;

#[tokio::test]
async fn test_promise_result_sdk_fn_successful() {
    let context = promise_result_test_common().await;
    let wnear = &context.wnear;
    let engine = &context.engine;

    let promise_result = promise_result_call(
        PromiseCreateArgs {
            target_account_id: wnear.inner.id().as_str().parse().unwrap(),
            method: "ft_balance_of".into(),
            args: format!(r#"{{"account_id": "{}"}}"#, engine.inner.id().as_str()).into_bytes(),
            attached_balance: Yocto::new(0),
            attached_gas: NearGas::new(5_000_000_000_000),
        },
        &context,
    )
    .await;

    let expected_result = nep141_utils::ft_balance_of(
        context.engine.inner.as_account(),
        wnear.inner.id(),
        engine.inner.id(),
    )
    .await
    .unwrap();
    assert_eq!(
        promise_result,
        PromiseResult::Successful(format!(r#""{}""#, expected_result).into_bytes())
    );
}

#[tokio::test]
async fn test_promise_result_sdk_fn_failed() {
    let context = promise_result_test_common().await;

    let promise_result = promise_result_call(
        PromiseCreateArgs {
            target_account_id: context.wnear.inner.id().as_str().parse().unwrap(),
            method: "ft_balance_of".into(),
            args: r#"{"account_id": "*invalid_account_id*"}"#.as_bytes().to_vec(),
            attached_balance: Yocto::new(0),
            attached_gas: NearGas::new(5_000_000_000_000),
        },
        &context,
    )
    .await;

    assert_eq!(promise_result, PromiseResult::Failed);
}

// Performs a callback to the `AuroraSdk.promiseResult` function from the given `base` promise.
// This is accomplished using the Aurora Engine's XCC feature.
async fn promise_result_call(
    base: PromiseCreateArgs,
    context: &PromiseResultTestContext,
) -> PromiseResult {
    let engine = &context.engine;
    let promise = PromiseArgs::Callback(PromiseWithCallbackArgs {
        base,
        callback: PromiseCreateArgs {
            target_account_id: engine.inner.id().as_str().parse().unwrap(),
            method: "call".into(),
            args: CallArgs::V2(FunctionCallArgsV2 {
                contract: context.sdk_tests,
                value: Wei::zero().to_bytes(),
                input: decode_promise_result_wrapper(0).0,
            })
            .try_to_vec()
            .unwrap(),
            attached_balance: Yocto::new(0),
            attached_gas: NearGas::new(30_000_000_000_000),
        },
    });
    let args = CallArgs::V2(FunctionCallArgsV2 {
        contract: cross_contract_call::ADDRESS,
        value: Wei::zero().to_bytes(),
        input: CrossContractCallArgs::Delayed(promise)
            .try_to_vec()
            .unwrap(),
    });
    let outcome = context
        .user
        .call(engine.inner.id(), "call")
        .args_borsh(args)
        .max_gas()
        .transact()
        .await
        .unwrap();
    let result: SubmitResult = outcome.borsh().unwrap();
    aurora_engine_utils::unwrap_success(result.status).unwrap();

    let router_account = format!("{}.{}", context.user_address.encode(), engine.inner.id())
        .parse()
        .unwrap();
    let exec_outcome = context
        .user
        .call(&router_account, "execute_scheduled")
        .args_json(serde_json::json!({
            "nonce": "0",
        }))
        .max_gas()
        .transact()
        .await
        .unwrap();
    let result: SubmitResult = exec_outcome.borsh().unwrap();
    let output = unwrap_eth_abi_bytes(&aurora_engine_utils::unwrap_success(result.status).unwrap());
    match output[0] {
        0 => PromiseResult::NotReady,
        1 => PromiseResult::Successful(output[1..].to_vec()),
        2 => PromiseResult::Failed,
        _ => panic!("Unexpected output from promiseResult"),
    }
}

async fn promise_result_test_common() -> PromiseResultTestContext {
    let worker = workspaces::sandbox().await.unwrap();
    let engine = aurora_engine_utils::deploy_latest(&worker).await.unwrap();
    let codec_lib = aurora_locker_utils::deploy_codec_lib(&engine)
        .await
        .unwrap();
    let utils_lib = aurora_locker_utils::deploy_utils_lib(&engine)
        .await
        .unwrap();
    let sdk_lib = aurora_locker_utils::deploy_aurora_sdk_lib(&engine, codec_lib, utils_lib)
        .await
        .unwrap();
    let sdk_tests = aurora_locker_utils::deploy_aurora_sdk_test_contract(&engine, sdk_lib)
        .await
        .unwrap();
    let wnear = Wnear::deploy(&worker, &engine).await.unwrap();
    let user = worker.dev_create_account().await.unwrap();
    let user_address = aurora_engine_sdk::types::near_account_to_evm_address(user.id().as_bytes());

    // Give user some WNEAR
    let deposit_amount = 5_000_000_000_000_000_000_000_000;
    engine
        .mint_wnear(&wnear, user_address, deposit_amount)
        .await
        .unwrap();

    // User approves XCC precompile to spend their WNEAR
    let result = engine
        .call_evm_contract_with(
            &user,
            wnear.aurora_token.address,
            wnear
                .aurora_token
                .approve(cross_contract_call::ADDRESS, deposit_amount.into()),
            Wei::zero(),
        )
        .await
        .unwrap();
    aurora_engine_utils::unwrap_success(result.status).unwrap();

    PromiseResultTestContext {
        user,
        user_address,
        wnear,
        engine,
        sdk_tests,
    }
}

struct PromiseResultTestContext {
    user: workspaces::Account,
    user_address: Address,
    wnear: Wnear,
    engine: aurora_engine_utils::AuroraEngine,
    sdk_tests: Address,
}

fn unwrap_eth_abi_bytes(abi_encoded: &[u8]) -> Vec<u8> {
    let mut decoded = ethabi::decode(&[ethabi::ParamType::Bytes], abi_encoded).unwrap();
    match decoded.pop().unwrap() {
        ethabi::Token::Bytes(bytes) => bytes,
        _ => unreachable!(),
    }
}

fn decode_promise_result_wrapper(index: usize) -> aurora_engine_utils::ContractInput {
    #[allow(deprecated)]
    let solidity_fn = ethabi::Function {
        name: "decodePromiseResultWrapper".into(),
        inputs: vec![ethabi::Param {
            name: "index".into(),
            kind: ethabi::ParamType::Uint(256),
            internal_type: None,
        }],
        outputs: vec![ethabi::Param {
            name: "output".into(),
            kind: ethabi::ParamType::Bytes,
            internal_type: None,
        }],
        constant: None,
        state_mutability: ethabi::StateMutability::NonPayable,
    };
    let data = solidity_fn
        .encode_input(&[ethabi::Token::Uint(index.into())])
        .unwrap();
    aurora_engine_utils::ContractInput(data)
}
