// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "forge-std/Test.sol";
import "../src/Locker.sol";
import "../src/AuroraSdk.sol";
import "openzeppelin-contracts/token/ERC20/IERC20.sol";

contract LockerTest is Test {
    Locker public locker;

    function setUp() public {
        locker = new Locker("factory.near", IERC20(0x4861825E75ab14553E5aF711EbbE6873d369d146));
    }

    function testLock() public {}
}
