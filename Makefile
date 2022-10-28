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

aurora-locker-sdk:
	cd aurora-locker; $(FORGE) build --libraries src/Codec.sol:Codec:$(CODEC) --libraries src/Utils.sol:Utils:$(UTILS)

aurora-locker-with-libs:
	cd aurora-locker; $(FORGE) build --libraries src/Codec.sol:Codec:$(CODEC) --libraries src/AuroraSdk.sol:AuroraSdk:$(SDK)

test:
	$(CARGO) test

check: check-compile check-fmt check-clippy test

check-compile:
	$(CARGO) check --all-targets

check-fmt:
	$(CARGO) fmt -- --check

check-clippy:
	$(CARGO) clippy --all-targets

clean:
	$(CARGO) clean

.PHONY: check clean near-token-factory near-token-contract aurora-locker aurora-locker-sdk aurora-locker-with-libs check-compile check-fmt check-clippy test
