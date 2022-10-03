# To Do Items

Items separated by repository

## General

-   Store version and commit hash from every contract on chain. Make a test for this.
-   For every public method on the contracts add comment about who should be able to call the method.
-   Is it worth using https://opensourcelibs.com/lib/changelog-ci?
-   Makefile to build all binaries.
-   **NEAR:** Use contract-builder for NEAR smart contracts reproducible-builds.
    For speed improve contract-builder to cache rust/cargo installation plus all project dependencies.

## Aurora Locker

Methods:

-   Create token counter part on NEAR
-   Lock tokens and send them to NEAR account using ft_transfer
-   Lock tokens and send them to NEAR contract using ft_transfer_call

## NEAR token factory

-   [x] Deploy new accounts.

## NEAR Token contract

-   [x] Regular FT contract primitives.
-   [x] `send_to_aurora` through the factory. Burn the tokens.

## Tests

-   Setup
    -   Download and compile aurora-engine
    -   Deploy engine / factory / locker contracts
    -   Initialize a testing ERC-20 contract on Aurora
    -   Create ERC-20 representative on NEAR
-   Happy Path
    -   Check ERC-20 metadata from deployed contract is correct.
    -   Send back and forth
        User send tokens from Aurora to its NEAR account id.
        Same users send all tokens back.
    -   Send tokens using deposit_call that doesn't consume all the tokens.
        Make sure remaining tokens are properly refunded.
        -   Test scenario when the called function fails. Tokens should still be refunded.
    -   Send tokens from Aurora_A -> NEAR_B -> NEAR_C -> Aurora_D.
        Check proper balance at every point. in particular at the end.
    -   Check updating metadata if it changes on Ethereum is possible.
    -   Check upgrading the contracts is possible.
    -   Check pausing contract is possible.
        -   Paused methods must not work while paused
        -   Methods must work again after unpaused.
    -   Access Control is working as expected.
        Test different scenarios.
-   Test CLI: https://rust-cli.github.io/book/tutorial/testing.html
    -   Logs and config should be properly updated. Transactions should be properly created.
-   Check commit hash and version are properly stored on chain.
    The version should be the same as the one in the contract.
    The hash must match the current git hash.

## CI

-   code coverage.
-   tests / clippy / rustfmt
-   binary compilations. Binary must compile.
-   Make sure that version from all contracts are the same.
-   Cargo audit + check for unused dependencies.
-   Linter for solidity contracts.

## Aurora Connector CLI

Create rust scripts to manage these contracts.

-   deploy
-   upgrade
-   list: List all created fungible tokens.

### Deployment

Deployment MUST be a command that receives arguments from a config file and updates the config file with the new contract addresses.

-   Before deployment all files must be built from the Makefile. Github repository MUST have no changes after binaries are built.
-   There should be a log files where all interactions on chain are stored, including the contract addresses and transactions created.
-   Logs should include all changes to the config file (if a field is replaced, the old value and the new should be stored in the log file).

**Workflow:**

-   Verify that account for factory is created on NEAR, it has no contract, and we have access key for it. Check it is a valid NEAR Account ID (i.e the length is less than 64 - 1 - 40 = 23).
-   Deploy locker using this account id.
-   Deploy factory using locker account id.
