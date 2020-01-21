set shell := ["cmd.exe", "/c"]

alias b := build
alias v := validate
alias x := fix

# Builds project
build:
    cargo build

# Validates project
validate: validate_format build test lint

# Fixes issues that can be resolved automatically
fix: format

# Validates that code is formatted correctly
validate_format:
    cargo fmt -- --check

# Formats rust code
format:
    cargo fmt

# Analyzes code
lint:
    cargo clippy -- -D warnings

# Runs tests
test:
    cargo test --verbose --all-features
