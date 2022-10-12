// SPDX-License-Identifier: CC-BY-1.0
pragma solidity ^0.8.17;

import "openzeppelin-contracts/token/ERC20/IERC20.sol";
import "openzeppelin-contracts/token/ERC20/extensions/IERC20Metadata.sol";
import "./AuroraSdk.sol";

string constant ERR_METHOD_NOT_IMPLEMENTED = "ERR_METHOD_NOT_IMPLEMENTED";
// TODO: Determine proper values for gas.
uint64 constant ON_DEPOSIT_NEAR_GAS = 25_000_000_000_000;
// TODO: Determine proper values for gas.
uint64 constant CREATE_NEAR_GAS = 50_000_000_000_000;
// TODO: Determine proper values for gas.
uint64 constant DEPOSIT_CALLBACK_NEAR_GAS = 15_000_000_000_000;
uint64 constant ON_UPDATE_TOKEN_METADATA = 3_000_000_000_000;
// TODO: Determine proper values for gas.
uint64 constant STORAGE_DEPOSIT_NEAR_GAS = 5_000_000_000_000;
uint128 constant NEW_TOKEN_DEPOSIT_COST = 3_000_000_000_000_000_000_000_000;
uint128 constant STORAGE_DEPOSIT_COST = 1_250_000_000_000_000_000_000;

// TODO: Implement Pause mechanics.
// TODO: Implement Upgradable mechanics.
// TODO: Implement AdminControlled mechanics.

/// The Locker contract holds all ERC20 tokens that are sent from Aurora to NEAR.
/// It provides an interface for depositing and withdrawing the tokens. After deposit
/// the same amount of tokens are minted on the equivalent NEP141 contract. In a similar
/// way, when tokens are burnt on NEAR side, they are withdrawn back to the specified
///  user on Aurora.
contract Locker {
    using AuroraSdk for NEAR;
    using AuroraSdk for PromiseWithCallback;
    using AuroraSdk for PromiseCreateArgs;
    using Codec for bytes;
    using Codec for uint128;
    using Codec for uint8;

    /// NEAR Account Id of the factory
    string public factoryAccountId;
    /// Implicit addres for the factory
    address public factoryImplicitAddress;
    /// Interface to interact with NEAR smart contracts.
    NEAR public near;
    /// Implicit address for representative NEAR account of this contract.
    address public immutable selfReprsentativeImplicitAddress;
    /// An address is present in this mapping with a non-zero value if the corresponding NEAR
    /// token has been created.
    mapping(IERC20 => uint256) public registeredTokens;

    constructor(string memory factoryAccountId_, IERC20 wNEAR) {
        factoryAccountId = factoryAccountId_;
        factoryImplicitAddress = AuroraSdk.implicitAuroraAddress(factoryAccountId);
        near = AuroraSdk.initNear(wNEAR);
        selfReprsentativeImplicitAddress = AuroraSdk.nearRepresentitiveImplicitAddress(address(this));
    }

    /// Perform a do-nothing transaction to force the Locker's NEAR account to be created.
    /// This means that the first deposit to the locker will not need to cover the initialization cost.
    function initNearAccount() public {
        PromiseCreateArgs memory create_near_account = near.call(factoryAccountId, "touch", "", 0, 1);
        create_near_account.transact();
    }

    /// Function to bridge a new token to NEAR. Must be called before the token will
    /// be accepted by `deposit`.
    function createToken(IERC20 token) public {
        require(registeredTokens[token] == 0, "ERR_TOKEN_ALREADY_REGISTERED");

        registeredTokens[token] = 1;

        // Require user to cover the cost of creating a new token by
        // attaching a balance of `NEW_TOKEN_DEPOSIT_COST`.
        PromiseCreateArgs memory createOnNear = near.call(
            factoryAccountId, "create_token", abi.encodePacked(token), NEW_TOKEN_DEPOSIT_COST, CREATE_NEAR_GAS
        );

        createOnNear.transact();
    }

    /// Pays the NEP-141 storage deposit of the given token for the given account ID.
    function storageDeposit(IERC20 token, string memory accountId) public {
        require(registeredTokens[token] > 0, "ERR_TOKEN_NOT_FOUND");

        string memory tokenAccountId = AuroraSdk.addressSubAccount(address(token), factoryAccountId);

        // Require user to cover the cost of the storage deposit by
        // attaching a balance of `NEW_TOKEN_DEPOSIT_COST`.
        PromiseCreateArgs memory storageDepositOnNear = near.call(
            tokenAccountId,
            "storage_deposit",
            abi.encodePacked("{\"account_id\":\"", accountId, "\"}"),
            STORAGE_DEPOSIT_COST,
            STORAGE_DEPOSIT_NEAR_GAS
        );

        storageDepositOnNear.transact();
    }

    /// ERC20 tokens are locked in this contract, while the equivalent
    /// amount is minted on NEAR, in a contract implementing the NEP141
    /// interface. The user must approve this contract for the amount to
    /// be transferred in advance, and it must specify the NEAR recipient.
    /// The sender should make sure that the NEAR receipient is registered
    /// in the target contract.
    ///
    /// If the transaction fails, the tokens are automatically returned to
    /// the sender of the transaction.
    function deposit(IERC20 token, string memory receiverId, uint128 amount) public {
        require(registeredTokens[token] > 0, "ERR_TOKEN_NOT_FOUND");

        // First transfer the tokens from the caller to the locker contract.
        token.transferFrom(msg.sender, address(this), amount);

        // Issue a call to the factory on NEAR factory to mint the same amount
        // of tokens for the receiverId on NEAR for this token.
        PromiseCreateArgs memory mintOnNear = near.call(
            factoryAccountId,
            "on_deposit",
            abi.encodePacked(token, bytes(receiverId).encode(), amount.encodeU128()),
            0,
            ON_DEPOSIT_NEAR_GAS
        );

        // Prepare callback to return tokens to the sender if the call to
        // the factory fails.
        PromiseCreateArgs memory callback = near.auroraCall(
            address(this),
            abi.encodeWithSelector(this.depositCallback.selector, token, msg.sender, amount),
            0,
            DEPOSIT_CALLBACK_NEAR_GAS
        );

        // Combine the two promises into a single promise and schedule it.
        mintOnNear.then(callback).lazy_transact();
    }

    /// Callback to return tokens to the sender if the call to the factory
    /// fails. This method can only be called by the representative NEAR
    /// account of this contract.
    function depositCallback(IERC20 token, address sender, uint128 amount) public {
        // Only the representative NEAR account of this contract can call this
        // method.
        require(msg.sender == selfReprsentativeImplicitAddress, "ERR_ACCESS_DENIED");

        // Transaction to mint tokens failed, so we need to return the tokens
        // to the sender.
        if (AuroraSdk.promiseResult(0).status != PromiseResultStatus.Successful) {
            token.transfer(sender, amount);
        }
    }

    /// Finish the transfer of tokens from NEAR to Aurora.
    ///
    /// This function CAN only be called from the factory contract. Tokens
    /// are transferred to the receiver after they are burnt on NEAR side.
    /// It is important that this function MUST never fail. In particular the
    /// amount to be withdrawn WILL be owned by the contract, since it was
    /// deposited before during a transfer.
    function withdraw(IERC20 token, address receiver, uint256 amount) public {
        // Only the factory contract can call this method.
        require(msg.sender == factoryImplicitAddress, "ERR_ACCESS_DENIED");

        // Transfer the tokens to the receiver.
        token.transfer(receiver, amount);
    }

    /// Fetch the current metadata of the specified ERC20 token and updates
    /// the metadata on the NEAR side with the same values. This methods can
    /// be called by anyone, even for tokens that already has metadata. This
    /// is relevant in case metadata is updated on the ERC20 side.
    ///
    /// In particular if the token have not been registered on NEAR side, the
    /// promise to update the metadata will fail.
    function syncTokenMetadata(IERC20Metadata token) public {
        // Fetch the metadata from the ERC20 token.
        string memory name = token.name();
        string memory symbol = token.symbol();
        uint8 decimals = token.decimals();

        // Issue a call to the factory on NEAR factory to update the metadata
        // for this token.
        PromiseCreateArgs memory updateMetadataOnNear = near.call(
            factoryAccountId,
            "update_token_metadata",
            abi.encodePacked(token, bytes(name).encode(), bytes(symbol).encode(), decimals.encodeU8()),
            0,
            ON_UPDATE_TOKEN_METADATA
        );

        // Schedule the promise.
        updateMetadataOnNear.transact();
    }

    /// Transfer ERC20 tokens from Aurora to NEAR chain and execute a
    /// function call.
    ///
    /// Similar to `transfer`, but also executes a function call on the
    /// NEAR side. Check comments and considerations for `transfer` function.
    ///
    /// Insipired by `ft_transfer_call` on NEP141:
    /// https://nomicon.io/Standards/Tokens/FungibleToken/Core.
    function depositCall(
        IERC20, // token
        uint256, // amount
        bytes calldata // payload
    ) internal pure {
        require(false, ERR_METHOD_NOT_IMPLEMENTED);
    }

    /// Sends new metadata from ERC20 token to the representative token on NEAR.
    ///
    /// This function SHOULD be used to keep metadata synced between the two
    /// tokens. In particular if the metadata changes in the ERC20 token, the
    /// NEP141 token on NEAR CAN be updated as well.
    function updateMetadata(
        IERC20Metadata //token
    ) internal pure {
        require(false, ERR_METHOD_NOT_IMPLEMENTED);
    }
}
