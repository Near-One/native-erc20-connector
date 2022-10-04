// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "forge-std/Test.sol";
import "../src/Locker.sol";

contract LockerTest is Test {
    Locker public locker;

    function setUp() public {
        locker = new Locker();
    }

    function testIncrement() public {
        locker.lock(0);
    }
}
