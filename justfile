set shell := ["cmd.exe", "/c"]

alias b := build
alias f := fix
alias v := validate

# Builds the project
build:
    cargo build

# Generates documentation for public and private items.
doc_all:
    cargo doc --document-private-items

# Fixes issues that can be addressed automatically
fix: format

# Validates that code is formatted correctly
validate_format:
    cargo fmt -- --check

# Formats rust code
format:
    cargo fmt

# Validates code style
lint:
    cargo clippy -- -D warnings

# Runs tests
test:
    cargo test --verbose --all-features

# Validates the project
validate: validate_fmt build test lint

# Validates the formatting of the project
validate_fmt:
    cargo fmt -- --check
