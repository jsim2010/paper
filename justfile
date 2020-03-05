alias b := build
alias f := fix
alias t := test
alias v := validate

# Builds the project
build:
    cargo build

# Checks the formatting of the project
check_format:
    cargo fmt -- --check

# Generates documentation for public items.
doc:
    cargo doc

# Generates documentation for public and private items.
doc_all:
    cargo doc --document-private-items

# Fixes issues that can be addressed automatically
fix: format

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
validate: check_format build test lint
