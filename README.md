# On Aurora

## Locker contract

Methods:

-   Create token counter part on NEAR
-   Lock tokens and send them to NEAR account using ft_transfer
-   Lock tokens and send them to NEAR contract using ft_transfer_call

## Factory contract on NEAR

-   Deploy new accounts.

## Token Representation on NEAR

-   Regular FT contract primitives.
-   `send_to_aurora` through the factory. Burn the tokens.

## Tests

-   Download and compile aurora-engine
-   Deploy engine / factory / locker contracts
-   Initialize a testing ERC-20 contract on Aurora
-   Migrate ERC-20 contract to NEAR

### Scenario

-   Send token from Aurora to NEAR, and send it back.
-   Transfer tokens between accounts on NEAR.
-   Send tokens from different NEAR account.

-   Updating ERC20-metadata.
-   Upgrade contract.
-   Pause contract.
-   Access control is working properly.
-   100% test coverage.
-   CI:
    -   tests / clippy / rustfmt
    -   binary compilations
    -   Make sure that version from all contracts are the same.
    -   Cargo audit + check for unused dependencies.
-   Store version and commit hash from every contract on chain. Make a test for this.
-   Check visibility of every method. Add comment about who should be able to call the method.
-   Is it worth using https://opensourcelibs.com/lib/changelog-ci?
-   Makefile to build all binaries.
-   Use contract-builder for NEAR smart contracts reproducible-builds. For speed improve contract-builder to cache rust/cargo installation plus all project dependencies.

## Scripts

Create rust scripts to manage these contracts.

-   Deployment must be a command that receives arguments from a config file and updates the config file with the new contract addresses.
    -   Before deployment all files must be built from the Makefile. Github repository MUST have no changes after binaries are built.
-   There should be a log files where all interactions on chain are stored, including the contract addresses and transactions created.
-   Logs should include all changes to the logs files (if a field is replaced, the old value and the new should be stored in the log file).
-   Script should contains functions to manage the contracts after deployment.
    -   Script that upgrades all contracts binary.

## Deployment

-   Verify that account for factory is created on NEAR, it has no contract, and we have access key for it. Check it is a valid NEAR Account ID (i.e the length is less than 64 - 1 - 40 = 23).
-   Deploy locker using this account id.
-   Deploy factory using locker account id.
