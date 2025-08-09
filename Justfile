coverage:
    cargo llvm-cov --ignore-filename-regex tests\.rs

lcov:
    cargo llvm-cov --lcov --output-path=lcov.info --ignore-filename-regex tests\.rs
    genhtml lcov.info --dark-mode --flat --missed --output-directory "target/coverage_html"

precommit:
    cargo fmt
    cargo clippy --tests -- -D warnings
    cargo check --benches --features bench
    cargo test
