alias b := build
alias d := doc
alias f := fix
alias l := lint
alias t := test
alias v := validate

# Ideally `build` would allow warnings - see https://github.com/rust-lang/cargo/issues/3591.
#
# Builds the project
build:
    cargo build

# This ideally would use some conditional functionality built into just.
#
# Installs everything needed for dependencies
_install_deps:
    cargo deny --version || cargo install cargo-deny

# Installs everything needed for formatting
_install_format:
    rustup component add rustfmt

# Installs everything needed for linting
_install_lint:
    rustup component add clippy

# Generates documentation for public items
doc:
    cargo doc

# Generates documentation for public and private items
doc_all:
    cargo doc --document-private-items

# Fixes issues that can be addressed automatically
fix: _install_format fix_format

# Formats rust code
fix_format: _install_format
    cargo fmt

# Any lint that is allowed is explained below:
# - box_pointers: box pointers are okay and useful
# - variant_size_differences: handled by clippy::large_enum_variant
# - clippy::empty_enum: recommended `!` type is not stable
# - clippy::multiple_crate_versions: not fixable when caused by dependencies
# - clippy::implicit_return: rust convention calls for implicit return
# - clippy::redundant_pub_crate: conflicts with clippy::unreachable_pub
#
# Lints the project source code
lint: _install_lint
    cargo clippy -- \
     -D absolute_paths_not_starting_with_crate \
     -D anonymous_parameters \
     -A box_pointers \
     -D deprecated_in_future \
     -D elided_lifetimes_in_paths \
     -D explicit_outlives_requirements \
     -D indirect_structural_match \
     -D keyword_idents \
     -D macro_use_extern_crate \
     -D meta_variable_misuse \
     -D missing_copy_implementations \
     -D missing_debug_implementations \
     -D missing_docs \
     -D missing_doc_code_examples \
     -D non_ascii_idents \
     -D private_doc_tests \
     -D single_use_lifetimes \
     -D trivial_casts \
     -D trivial_numeric_casts \
     -D unreachable_pub \
     -D unsafe_code \
     -D unstable_features \
     -D unused_extern_crates \
     -D unused_import_braces \
     -D unused_lifetimes \
     -D unused_qualifications \
     -D unused_results \
     -A variant_size_differences \
     -D warnings \
     -D clippy::correctness \
     -D clippy::restriction \
     -D clippy::style \
     -D clippy::complexity \
     -D clippy::perf \
     -D clippy::cargo \
     -D clippy::pedantic \
     -D clippy::nursery \
     -A clippy::empty_enum \
     -A clippy::multiple_crate_versions \
     -A clippy::implicit_return \
     -A clippy::redundant_pub_crate \

# Create pull request for resolving <issue_num>
pr issue_num:
    hub pull-request --push -m "`hub issue show -f "%t" {{issue_num}}`" -m "Closes #{{issue_num}}"

# Configures the version of rust
set_rust version:
    rustup override set {{version}}

# Runs tests
test:
    cargo test --verbose --all-features

# Validates the project
validate: (set_rust "1.44.0") validate_format validate_deps lint build test

# Validates dependencies of the project
validate_deps: _install_deps
    cargo deny check

# Validates the formatting of the project
validate_format: _install_format
    cargo fmt -- --check
