set shell := ["cmd.exe", "/c"]

alias b := build
alias c := check
alias x := fix

# Builds project
build:
    cargo build

# Checks that code is valid
check: build test check_format lint

# Checks that code is formatted correctly
check_format:
    cargo fmt -- --check

# Fixes issues that can be addressed automatically
fix: format

# Formats rust code
format:
    cargo fmt

# Checks code style
lint:
    cargo clippy -- -D warnings

# Runs tests
test:
    cargo test --verbose
