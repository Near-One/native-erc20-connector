// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";

contract Locker {
    /// NEAR Account Id of the factory
    string public FactoryAccountId;

    constructor(string memory factoryAccountId) {
        FactoryAccountId = factoryAccountId;
    }

    /**
     * @dev Locks the specified amount of ERC20 token.
     *
     * The exact same amount is minted on the NEAR side for `receiverId`
     */
    function lock(
        IERC20 token,
        uint256 amount,
        string memory receiverId
    ) public {
        sdk.validAccountId(receiverId);
        sdk.result(0);
        sdk.promise().callback().transact();
        sdk.safeJson();
    }

    function lockCall(
        IERC20 token,
        uint256,
        bytes calldata arguments
    ) public {}

    function unlock() public {}
}
