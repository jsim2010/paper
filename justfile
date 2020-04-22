alias b := build
alias f := fix
alias l := lint
alias t := test
alias v := validate

# Create a branch to resolve <issue>
branch issue:
    git switch -c {{issue}}

# Ideally `build` would allow warnings - see https://github.com/rust-lang/cargo/issues/3591.
#
# Builds the project
build:
    cargo build

# Checks dependencies of the project
check_deps:
    cargo deny check

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

# Any lint that is not forbidden is explained below:
# DENY
# - elided_lifetimes_in_paths: allowed by Deserialize and Serialize
# - explicit_outlives_requirements: allowed by Deserialize and Serialize
# - unused_extern_crates: allowed by Deserialize and Serialize
# - unused_qualifications: allowed by Debug
# - unused_results: allowed by io::lsp::utils
# - clippy::nursery: nursery lints are not fully developed
# - clippy::indexing_slicing: required by EnumMap
# - clippy::unreachable: required by Enum
# ALLOW
# - box_pointers: box pointers are okay and useful
# - variant_size_differences: no major impact under normal conditions
# - clippy::multiple_crate_versions: not fixable when caused by dependencies
# - clippy::empty_enum: recommended `!` type is not stable
# - clippy::implicit_return: rust convention calls for implicit return
#
# Validates code style
lint:
    cargo clippy --\
     -F absolute_paths_not_starting_with_crate\
     -F anonymous_parameters\
     -A box_pointers\
     -F deprecated_in_future\
     -D elided_lifetimes_in_paths\
     -D explicit_outlives_requirements\
     -F indirect_structural_match\
     -F keyword_idents\
     -F macro_use_extern_crate\
     -F meta_variable_misuse\
     -F missing_copy_implementations\
     -F missing_debug_implementations\
     -F missing_docs\
     -F missing_doc_code_examples\
     -F non_ascii_idents\
     -F private_doc_tests\
     -F single_use_lifetimes\
     -F trivial_casts\
     -F trivial_numeric_casts\
     -F unreachable_pub\
     -F unsafe_code\
     -D unused_extern_crates\
     -F unused_import_braces\
     -F unused_lifetimes\
     -D unused_qualifications\
     -D unused_results\
     -A variant_size_differences\
     -F warnings\
     -F ambiguous_associated_items\
     -F conflicting_repr_hints\
     -F const_err\
     -F exceeding_bitshifts\
     -F ill_formed_attribute_input\
     -F invalid_type_param_default\
     -F macro_expanded_macro_exports_accessed_by_absolute_paths\
     -F missing_fragment_specifier\
     -F mutable_transmutes\
     -F no_mangle_const_items\
     -F order_dependent_trait_objects\
     -F overflowing_literals\
     -F patterns_in_fns_without_body\
     -F pub_use_of_private_extern_crate\
     -F soft_unstable\
     -F unknown_crate_types\
     -D clippy::nursery\
     -F clippy::cast_lossless\
     -F clippy::cast_possible_truncation\
     -F clippy::cast_possible_wrap\
     -F clippy::cast_precision_loss\
     -F clippy::cast_sign_loss\
     -F clippy::checked_conversions\
     -F clippy::copy_iterator\
     -F clippy::default_trait_access\
     -F clippy::doc_markdown\
     -A clippy::empty_enum\
     -F clippy::enum_glob_use\
     -F clippy::expl_impl_clone_on_copy\
     -F clippy::explicit_into_iter_loop\
     -F clippy::explicit_iter_loop\
     -F clippy::filter_map\
     -F clippy::filter_map_next\
     -F clippy::find_map\
     -F clippy::if_not_else\
     -F clippy::inline_always\
     -F clippy::invalid_upcast_comparisons\
     -F clippy::items_after_statements\
     -F clippy::large_digit_groups\
     -F clippy::large_stack_arrays\
     -F clippy::linkedlist\
     -F clippy::map_flatten\
     -F clippy::match_same_arms\
     -F clippy::maybe_infinite_iter\
     -F clippy::missing_errors_doc\
     -F clippy::module_name_repetitions\
     -F clippy::must_use_candidate\
     -F clippy::mut_mut\
     -F clippy::needless_continue\
     -F clippy::needless_pass_by_value\
     -F clippy::non_ascii_literal\
     -F clippy::option_map_unwrap_or\
     -F clippy::option_map_unwrap_or_else\
     -F clippy::pub_enum_variant_names\
     -F clippy::range_plus_one\
     -F clippy::redundant_closure_for_method_calls\
     -F clippy::replace_consts\
     -F clippy::result_map_unwrap_or_else\
     -F clippy::same_functions_in_if_condition\
     -F clippy::shadow_unrelated\
     -F clippy::similar_names\
     -F clippy::single_match_else\
     -F clippy::string_add_assign\
     -F clippy::too_many_lines\
     -F clippy::type_repetition_in_bounds\
     -F clippy::unicode_not_nfc\
     -F clippy::unseparated_literal_suffix\
     -F clippy::unused_self\
     -F clippy::used_underscore_binding\
     -A clippy::multiple_crate_versions\
     -F clippy::cargo_common_metadata\
     -F clippy::wildcard_dependencies\
     -F clippy::as_conversions\
     -F clippy::clone_on_ref_ptr\
     -F clippy::dbg_macro\
     -F clippy::decimal_literal_representation\
     -F clippy::else_if_without_else\
     -F clippy::exit\
     -F clippy::filetype_is_file\
     -F clippy::float_arithmetic\
     -F clippy::float_cmp_const\
     -F clippy::get_unwrap\
     -A clippy::implicit_return\
     -D clippy::indexing_slicing\
     -F clippy::integer_arithmetic\
     -F clippy::integer_division\
     -F clippy::let_underscore_must_use\
     -F clippy::mem_forget\
     -F clippy::missing_docs_in_private_items\
     -F clippy::missing_inline_in_public_items\
     -F clippy::modulo_arithmetic\
     -F clippy::multiple_inherent_impl\
     -F clippy::option_expect_used\
     -F clippy::option_unwrap_used\
     -F clippy::panic\
     -F clippy::print_stdout\
     -F clippy::result_expect_used\
     -F clippy::result_unwrap_used\
     -F clippy::shadow_reuse\
     -F clippy::shadow_same\
     -F clippy::string_add\
     -F clippy::todo\
     -F clippy::unimplemented\
     -D clippy::unreachable\
     -F clippy::use_debug\
     -F clippy::wildcard_enum_match_arm\
     -F clippy::wrong_pub_self_convention

# Runs tests
test:
    cargo test --verbose --all-features

# Validates the project
validate: check_format check_deps build test lint
