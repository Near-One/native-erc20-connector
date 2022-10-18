// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "forge-std/Test.sol";
import "../src/AuroraSdk.sol";
import "openzeppelin-contracts/token/ERC20/IERC20.sol";

contract AuroraSdkTest is Test {
    function testImplicitAuroraAddress() public {
        assertEq(
            AuroraSdk.implicitAuroraAddress("nearCrossContractCall"),
            address(0x516Cded1D16af10CAd47D6D49128E2eB7d27b372)
        );
    }

    function decodePromiseResultWrapper(uint256 index) public returns (bytes memory) {
        PromiseResult memory promiseResult = AuroraSdk.promiseResult(index);
        return abi.encodePacked(promiseResult.status, promiseResult.output);
    }
}
