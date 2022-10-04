CARGO = cargo
RUSTUP = rustup

all: near-token-factory near-token-contract

near-token-factory:
	$(RUSTUP) target add wasm32-unknown-unknown
	$(CARGO) build -p near-token-factory --target wasm32-unknown-unknown --release

near-token-contract:
	$(RUSTUP) target add wasm32-unknown-unknown
	$(CARGO) build -p near-token-contract --target wasm32-unknown-unknown --release

check: check-compile check-fmt check-clippy

check-compile:
	$(CARGO) check --all-targets

check-fmt:
	$(CARGO) fmt -- --check

check-clippy:
	$(CARGO) clippy --all-targets

clean:
	$(CARGO) clean

.PHONY: check clean near-token-factory near-token-contract check-compile check-fmt check-clippy
