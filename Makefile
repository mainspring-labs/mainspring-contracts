.PHONY: build examples test lint fmt

# Tests import the example Wasm, so build it first.
examples:
	stellar contract build --package tipjar-v1
	stellar contract build --package tipjar-v2

build: examples
	stellar contract build --package mainspring-spring
	stellar contract build --package mainspring-factory

test: examples
	cargo test --workspace

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt --all
