//! Unit tests for `parser/flags.rs`: lock every named constant and helper
//! function so any future renumbering or semantic edit fails CI rather than
//! slipping through.
//!
//! `parser/flags.rs` mirrors TypeScript's `NodeFlags`, `ModifierFlags`, and
//! `TransformFlags` enums; the existing `tests.rs` only spot-checked four
//! values. This file is pure-additive coverage.

use super::*;

// Every node_flags single-bit constant paired with its TS bit position.
// `AWAIT_USING` is intentionally excluded because it is the composite
// `CONST | USING` (see `node_flags_await_using_equals_const_or_using`).
const NODE_SINGLE_BITS: &[(u32, u32, &str)] = &[
    (node_flags::LET, 0, "LET"),
    (node_flags::CONST, 1, "CONST"),
    (node_flags::USING, 2, "USING"),
    (node_flags::NESTED_NAMESPACE, 3, "NESTED_NAMESPACE"),
    (node_flags::SYNTHESIZED, 4, "SYNTHESIZED"),
    (node_flags::NAMESPACE, 5, "NAMESPACE"),
    (node_flags::OPTIONAL_CHAIN, 6, "OPTIONAL_CHAIN"),
    (node_flags::EXPORT_CONTEXT, 7, "EXPORT_CONTEXT"),
    (node_flags::CONTAINS_THIS, 8, "CONTAINS_THIS"),
    (node_flags::HAS_IMPLICIT_RETURN, 9, "HAS_IMPLICIT_RETURN"),
    (node_flags::HAS_EXPLICIT_RETURN, 10, "HAS_EXPLICIT_RETURN"),
    (node_flags::GLOBAL_AUGMENTATION, 11, "GLOBAL_AUGMENTATION"),
    (node_flags::HAS_ASYNC_FUNCTIONS, 12, "HAS_ASYNC_FUNCTIONS"),
    (node_flags::DISALLOW_IN_CONTEXT, 13, "DISALLOW_IN_CONTEXT"),
    (node_flags::YIELD_CONTEXT, 14, "YIELD_CONTEXT"),
    (node_flags::DECORATOR_CONTEXT, 15, "DECORATOR_CONTEXT"),
    (node_flags::AWAIT_CONTEXT, 16, "AWAIT_CONTEXT"),
    (
        node_flags::DISALLOW_CONDITIONAL_TYPES_CONTEXT,
        17,
        "DISALLOW_CONDITIONAL_TYPES_CONTEXT",
    ),
    (node_flags::THIS_NODE_HAS_ERROR, 18, "THIS_NODE_HAS_ERROR"),
    (node_flags::JAVASCRIPT_FILE, 19, "JAVASCRIPT_FILE"),
    (
        node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR,
        20,
        "THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR",
    ),
    (
        node_flags::HAS_AGGREGATED_CHILD_DATA,
        21,
        "HAS_AGGREGATED_CHILD_DATA",
    ),
    (
        node_flags::POSSIBLY_CONTAINS_DYNAMIC_IMPORT,
        22,
        "POSSIBLY_CONTAINS_DYNAMIC_IMPORT",
    ),
    (
        node_flags::POSSIBLY_CONTAINS_IMPORT_META,
        23,
        "POSSIBLY_CONTAINS_IMPORT_META",
    ),
    (node_flags::JSDOC, 24, "JSDOC"),
    (node_flags::AMBIENT, 25, "AMBIENT"),
    (node_flags::IN_WITH_STATEMENT, 26, "IN_WITH_STATEMENT"),
    (node_flags::JSON_FILE, 27, "JSON_FILE"),
    (node_flags::TYPE_CACHED, 28, "TYPE_CACHED"),
    (node_flags::DEPRECATED, 29, "DEPRECATED"),
    (node_flags::TYPE_ONLY, 30, "TYPE_ONLY"),
];

const MODIFIER_BITS: &[(u32, u32, &str)] = &[
    (modifier_flags::PUBLIC, 0, "PUBLIC"),
    (modifier_flags::PRIVATE, 1, "PRIVATE"),
    (modifier_flags::PROTECTED, 2, "PROTECTED"),
    (modifier_flags::READONLY, 3, "READONLY"),
    (modifier_flags::OVERRIDE, 4, "OVERRIDE"),
    (modifier_flags::EXPORT, 5, "EXPORT"),
    (modifier_flags::ABSTRACT, 6, "ABSTRACT"),
    (modifier_flags::AMBIENT, 7, "AMBIENT"),
    (modifier_flags::STATIC, 8, "STATIC"),
    (modifier_flags::ACCESSOR, 9, "ACCESSOR"),
    (modifier_flags::ASYNC, 10, "ASYNC"),
    (modifier_flags::DEFAULT, 11, "DEFAULT"),
    (modifier_flags::CONST, 12, "CONST"),
    (modifier_flags::IN, 13, "IN"),
    (modifier_flags::OUT, 14, "OUT"),
    (modifier_flags::DECORATOR, 15, "DECORATOR"),
    (modifier_flags::DEPRECATED, 16, "DEPRECATED"),
];

const TRANSFORM_BITS: &[(u32, u32, &str)] = &[
    (
        transform_flags::CONTAINS_TYPESCRIPT,
        0,
        "CONTAINS_TYPESCRIPT",
    ),
    (transform_flags::CONTAINS_JSX, 1, "CONTAINS_JSX"),
    (transform_flags::CONTAINS_ESNEXT, 2, "CONTAINS_ESNEXT"),
    (transform_flags::CONTAINS_ES2022, 3, "CONTAINS_ES2022"),
    (transform_flags::CONTAINS_ES2021, 4, "CONTAINS_ES2021"),
    (transform_flags::CONTAINS_ES2020, 5, "CONTAINS_ES2020"),
    (transform_flags::CONTAINS_ES2019, 6, "CONTAINS_ES2019"),
    (transform_flags::CONTAINS_ES2018, 7, "CONTAINS_ES2018"),
    (transform_flags::CONTAINS_ES2017, 8, "CONTAINS_ES2017"),
    (transform_flags::CONTAINS_ES2016, 9, "CONTAINS_ES2016"),
    (transform_flags::CONTAINS_ES2015, 10, "CONTAINS_ES2015"),
    (
        transform_flags::CONTAINS_GENERATOR,
        11,
        "CONTAINS_GENERATOR",
    ),
    (
        transform_flags::CONTAINS_DESTRUCTURING_ASSIGNMENT,
        12,
        "CONTAINS_DESTRUCTURING_ASSIGNMENT",
    ),
    (
        transform_flags::CONTAINS_TYPESCRIPT_CLASS_SYNTAX,
        13,
        "CONTAINS_TYPESCRIPT_CLASS_SYNTAX",
    ),
    (
        transform_flags::CONTAINS_LEXICAL_THIS,
        14,
        "CONTAINS_LEXICAL_THIS",
    ),
    (
        transform_flags::CONTAINS_REST_OR_SPREAD,
        15,
        "CONTAINS_REST_OR_SPREAD",
    ),
    (
        transform_flags::CONTAINS_OBJECT_REST_OR_SPREAD,
        16,
        "CONTAINS_OBJECT_REST_OR_SPREAD",
    ),
    (
        transform_flags::CONTAINS_COMPUTED_PROPERTY_NAME,
        17,
        "CONTAINS_COMPUTED_PROPERTY_NAME",
    ),
    (
        transform_flags::CONTAINS_BLOCK_SCOPED_BINDING,
        18,
        "CONTAINS_BLOCK_SCOPED_BINDING",
    ),
    (
        transform_flags::CONTAINS_BINDING_PATTERN,
        19,
        "CONTAINS_BINDING_PATTERN",
    ),
    (transform_flags::CONTAINS_YIELD, 20, "CONTAINS_YIELD"),
    (transform_flags::CONTAINS_AWAIT, 21, "CONTAINS_AWAIT"),
    (
        transform_flags::CONTAINS_HOISTED_DECLARATION_OR_COMPLETION,
        22,
        "CONTAINS_HOISTED_DECLARATION_OR_COMPLETION",
    ),
    (
        transform_flags::CONTAINS_DYNAMIC_IMPORT,
        23,
        "CONTAINS_DYNAMIC_IMPORT",
    ),
    (
        transform_flags::CONTAINS_CLASS_FIELDS,
        24,
        "CONTAINS_CLASS_FIELDS",
    ),
    (
        transform_flags::CONTAINS_DECORATORS,
        25,
        "CONTAINS_DECORATORS",
    ),
    (
        transform_flags::CONTAINS_POSSIBLE_TOP_LEVEL_AWAIT,
        26,
        "CONTAINS_POSSIBLE_TOP_LEVEL_AWAIT",
    ),
    (
        transform_flags::CONTAINS_LEXICAL_SUPER,
        27,
        "CONTAINS_LEXICAL_SUPER",
    ),
    (
        transform_flags::CONTAINS_UPDATE_EXPRESSION_FOR_IDENTIFIER,
        28,
        "CONTAINS_UPDATE_EXPRESSION_FOR_IDENTIFIER",
    ),
    (
        transform_flags::CONTAINS_PRIVATE_IDENTIFIER_IN_EXPRESSION,
        29,
        "CONTAINS_PRIVATE_IDENTIFIER_IN_EXPRESSION",
    ),
    (
        transform_flags::HAS_COMPUTED_FLAGS,
        31,
        "HAS_COMPUTED_FLAGS",
    ),
];

fn assert_bit_layout(bits: &[(u32, u32, &str)]) {
    for &(value, position, name) in bits {
        assert_eq!(
            value,
            1u32 << position,
            "{name} expected 1 << {position}, got {value:#x}"
        );
        assert_ne!(value, 0, "{name} must be non-zero");
        assert_eq!(
            value & value.wrapping_sub(1),
            0,
            "{name} ({value:#x}) is not a single bit"
        );
    }
    for (i, &(a, _, na)) in bits.iter().enumerate() {
        for &(b, _, nb) in &bits[i + 1..] {
            assert_eq!(a & b, 0, "{na} and {nb} share a bit");
        }
    }
}

// =============================================================================
// node_flags
// =============================================================================

#[test]
fn node_flags_none_is_zero() {
    assert_eq!(node_flags::NONE, 0);
}

#[test]
fn node_flags_single_bits_match_ts_layout_and_are_disjoint() {
    assert_bit_layout(NODE_SINGLE_BITS);
}

#[test]
fn node_flags_await_using_equals_const_or_using() {
    // AWAIT_USING is the composite (CONST | USING) — NOT a single bit.
    assert_eq!(
        node_flags::AWAIT_USING,
        node_flags::CONST | node_flags::USING
    );
    assert_eq!(node_flags::AWAIT_USING, 6);
}

// ----- is_await_using -----

#[test]
fn is_await_using_true_when_both_const_and_using_set() {
    assert!(node_flags::is_await_using(node_flags::AWAIT_USING));
    assert!(node_flags::is_await_using(
        node_flags::AWAIT_USING | node_flags::EXPORT_CONTEXT
    ));
    assert!(node_flags::is_await_using(
        node_flags::AWAIT_USING | node_flags::JSDOC
    ));
}

#[test]
fn is_await_using_false_when_either_bit_missing() {
    assert!(!node_flags::is_await_using(node_flags::NONE));
    assert!(!node_flags::is_await_using(node_flags::LET));
    assert!(!node_flags::is_await_using(node_flags::CONST));
    assert!(!node_flags::is_await_using(node_flags::USING));
    // Const-only or using-only, even with extra noise, is still false.
    assert!(!node_flags::is_await_using(
        node_flags::CONST | node_flags::EXPORT_CONTEXT
    ));
    assert!(!node_flags::is_await_using(
        node_flags::USING | node_flags::EXPORT_CONTEXT
    ));
}

// ----- is_let_or_const -----

#[test]
fn is_let_or_const_true_for_let_const_and_combined() {
    assert!(node_flags::is_let_or_const(node_flags::LET));
    assert!(node_flags::is_let_or_const(node_flags::CONST));
    assert!(node_flags::is_let_or_const(
        node_flags::LET | node_flags::CONST
    ));
}

#[test]
fn is_let_or_const_excludes_using_alone() {
    // `using` (without const bit) is NOT let-or-const.
    assert!(!node_flags::is_let_or_const(node_flags::USING));
    assert!(!node_flags::is_let_or_const(node_flags::NONE));
    assert!(!node_flags::is_let_or_const(node_flags::EXPORT_CONTEXT));
    assert!(!node_flags::is_let_or_const(node_flags::JSDOC));
}

#[test]
fn is_let_or_const_true_for_await_using_via_const_bit() {
    // AWAIT_USING == CONST|USING, so the CONST bit is observed by the helper.
    // Lock that behaviour so a later refactor must revisit explicitly.
    assert!(node_flags::is_let_or_const(node_flags::AWAIT_USING));
}

// ----- is_block_scoped -----

#[test]
fn is_block_scoped_true_for_let_const_using_and_await_using() {
    assert!(node_flags::is_block_scoped(node_flags::LET));
    assert!(node_flags::is_block_scoped(node_flags::CONST));
    assert!(node_flags::is_block_scoped(node_flags::USING));
    assert!(node_flags::is_block_scoped(node_flags::AWAIT_USING));
}

#[test]
fn is_block_scoped_false_for_unrelated_flags_and_true_with_extras() {
    assert!(!node_flags::is_block_scoped(node_flags::NONE));
    assert!(!node_flags::is_block_scoped(node_flags::EXPORT_CONTEXT));
    assert!(!node_flags::is_block_scoped(node_flags::AMBIENT));
    assert!(!node_flags::is_block_scoped(node_flags::JSDOC));
    assert!(!node_flags::is_block_scoped(
        node_flags::EXPORT_CONTEXT | node_flags::JSDOC
    ));
    // Block-scope bit alongside unrelated high bits stays true.
    assert!(node_flags::is_block_scoped(
        node_flags::LET | node_flags::JSDOC | node_flags::AMBIENT
    ));
}

// =============================================================================
// modifier_flags
// =============================================================================

#[test]
fn modifier_flags_none_is_zero() {
    assert_eq!(modifier_flags::NONE, 0);
}

#[test]
fn modifier_flags_match_ts_layout_and_are_disjoint() {
    assert_bit_layout(MODIFIER_BITS);
}

// =============================================================================
// transform_flags
// =============================================================================

#[test]
fn transform_flags_none_is_zero() {
    assert_eq!(transform_flags::NONE, 0);
}

#[test]
fn transform_flags_match_ts_layout_and_are_disjoint() {
    assert_bit_layout(TRANSFORM_BITS);
}

#[test]
fn transform_flags_has_computed_flags_is_high_bit() {
    // 1 << 31 cannot be expressed via `1u32 << position` arithmetic in
    // `assert_bit_layout` without a wrapping_sub edge case; pin it explicitly.
    assert_eq!(transform_flags::HAS_COMPUTED_FLAGS, 0x8000_0000);
}

// =============================================================================
// Cross-module sanity
// =============================================================================

#[test]
fn node_flags_helpers_treat_input_as_node_flag_layout() {
    // `modifier_flags::PUBLIC` and `node_flags::LET` happen to share bit 0;
    // helpers operate on `node_flags` semantics regardless of caller intent.
    let bit_zero = modifier_flags::PUBLIC; // numerically equal to LET
    assert!(node_flags::is_let_or_const(bit_zero));
    assert!(node_flags::is_block_scoped(bit_zero));
    assert!(!node_flags::is_await_using(bit_zero));
}

// Compile-time lock: helpers remain `const fn`. A non-const fn regression
// would fail compilation here rather than at the (rare) const call sites.
const _IS_AWAIT_USING_CONST_OK: bool = node_flags::is_await_using(node_flags::AWAIT_USING);
const _IS_LET_OR_CONST_CONST_OK: bool = node_flags::is_let_or_const(node_flags::LET);
const _IS_BLOCK_SCOPED_CONST_OK: bool = node_flags::is_block_scoped(node_flags::USING);
const _: () = {
    assert!(_IS_AWAIT_USING_CONST_OK);
    assert!(_IS_LET_OR_CONST_CONST_OK);
    assert!(_IS_BLOCK_SCOPED_CONST_OK);
};
