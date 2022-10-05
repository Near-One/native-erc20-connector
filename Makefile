CARGO = cargo
RUSTUP = rustup
FORGE = forge

all: near-token-factory near-token-contract aurora-locker

near-token-factory:
	$(RUSTUP) target add wasm32-unknown-unknown
	$(CARGO) build -p near-token-factory --target wasm32-unknown-unknown --release

near-token-contract:
	$(RUSTUP) target add wasm32-unknown-unknown
	$(CARGO) build -p near-token-contract --target wasm32-unknown-unknown --release

aurora-locker:
	cd aurora-locker; $(FORGE) build

check: check-compile check-fmt check-clippy

check-compile:
	$(CARGO) check --all-targets

check-fmt:
	$(CARGO) fmt -- --check

check-clippy:
	$(CARGO) clippy --all-targets

clean:
	$(CARGO) clean

.PHONY: check clean near-token-factory near-token-contract aurora-locker check-compile check-fmt check-clippy
