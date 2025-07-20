precommit:
    cargo fmt
    cargo clippy --tests --benches -- -D warnings
    cargo test