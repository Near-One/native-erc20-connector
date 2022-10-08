CARGO = cargo
RUSTUP = rustup
FORGE = forge

# https://doc.rust-lang.org/stable/clippy/continuous_integration/index.html#continuous-integration
export RUSTFLAGS = -Dwarnings

all: near-token-factory near-token-contract

near-token-factory:
	$(RUSTUP) target add wasm32-unknown-unknown
	$(CARGO) build -p near-token-factory --target wasm32-unknown-unknown --release

near-token-contract:
	$(RUSTUP) target add wasm32-unknown-unknown
	$(CARGO) build -p near-token-contract --target wasm32-unknown-unknown --release

aurora-locker:
	cd aurora-locker; $(FORGE) build

check: check-rust check-solidity test

test: test-rust test-solidity

test-rust:
	$(CARGO) test

check-rust: check-compile-rust check-fmt-rust check-clippy test-rust

check-compile-rust:
	$(CARGO) check --all-targets

check-fmt-rust:
	$(CARGO) fmt -- --check

check-clippy:
	$(CARGO) clippy --all-targets

test-solidity:
	cd aurora-locker; $(FORGE) test -vvv

check-solidity: check-fmt-solidity

check-fmt-solidity:
	cd aurora-locker; $(FORGE) fmt --check

clean:
	$(CARGO) clean
	cd aurora-locker; $(FORGE) clean

.PHONY: all check clean near-token-factory near-token-contract aurora-locker check test test-rust check-rust check-compile-rust check-fmt-rust check-clippy test-solidity check-solidity check-fmt-solidity
