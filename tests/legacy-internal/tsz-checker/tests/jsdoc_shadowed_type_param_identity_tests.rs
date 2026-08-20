//! Declaration-identity coverage for checked-JS `@template` binders.
//!
//! JSDoc type parameters have no syntax name node. Their containing
//! declaration (or, for `@overload`, the individual comment) must therefore
//! supply identity so identical surface text does not merge unrelated binders.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use crate::test_utils::{
    check_js_source_code_messages_with_options, check_js_source_codes_with_options,
};
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::construction::{QueryDatabase, TypeInterner};

fn strict_js_codes(source: &str) -> Vec<u32> {
    check_js_source_codes_with_options(
        source,
        "test.js",
        CheckerOptions {
            strict: true,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    )
}

#[test]
fn nested_jsdoc_functions_with_identical_binders_report_ts2719() {
    let codes = strict_js_codes(
        r#"
/**
 * @template T
 * @param {T} outerValue
 */
function outer(outerValue) {
    /**
     * @template T
     * @param {T} innerValue
     */
    function inner(innerValue) {
        outerValue = innerValue;
    }
}
"#,
    );
    assert!(codes.contains(&2719), "expected TS2719, got {codes:?}");
}

#[test]
fn alias_application_bodies_preserve_alpha_equivalence_and_shadow_identity() {
    let compatible = strict_js_codes(
        r#"
/**
 * @template Payload
 * @typedef {{ payload: Payload }} Envelope
 */
/**
 * @template LeftValue
 * @param {Envelope<LeftValue>} input
 * @returns {Envelope<LeftValue>}
 */
function leftIdentity(input) { return input; }
/**
 * @template RightValue
 * @param {Envelope<RightValue>} input
 * @returns {Envelope<RightValue>}
 */
function rightIdentity(input) { return input; }
/** @type {typeof leftIdentity} */
const compatibleIdentity = rightIdentity;
"#,
    );
    assert!(
        !compatible
            .iter()
            .any(|code| matches!(code, 2322 | 2345 | 2719)),
        "renamed alpha-equivalent alias applications must relate: {compatible:?}"
    );

    let shadowed_diagnostics = check_js_source_code_messages_with_options(
        r#"
/**
 * @template Payload
 * @typedef {{ payload: Payload }} Envelope
 */
/**
 * @template ShadowValue
 * @param {Envelope<ShadowValue>} outerValue
 */
function outer(outerValue) {
    /**
     * @template ShadowValue
     * @param {Envelope<ShadowValue>} innerValue
     */
    function inner(innerValue) {
        outerValue = innerValue;
    }
}
"#,
        "test.js",
        CheckerOptions {
            strict: true,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    );
    let shadowed: Vec<_> = shadowed_diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        shadowed.contains(&2719),
        "an alias application must not erase the two declarations' binder identity: {shadowed_diagnostics:?}"
    );

    let different_aliases = strict_js_codes(
        r#"
/**
 * @template Value
 * @typedef {{ value: Value }} FirstBox
 */
/**
 * @template Value
 * @typedef {{ value: Value }} SecondBox
 */
/**
 * @template Outer
 * @param {FirstBox<Outer>} outerValue
 */
function outer(outerValue) {
    /**
     * @template Inner
     * @param {SecondBox<Inner>} innerValue
     */
    function inner(innerValue) { outerValue = innerValue; }
}
"#,
    );
    assert!(
        different_aliases.contains(&2322) && !different_aliases.contains(&2719),
        "different visible alias surfaces must keep TS2322 routing: {different_aliases:?}"
    );
}

#[test]
fn alias_application_construction_keeps_shadowed_type_arguments_distinct() {
    let source = r#"
/**
 * @template Payload
 * @typedef {{ payload: Payload }} Envelope
 */
/**
 * @template ShadowValue
 * @param {Envelope<ShadowValue>} outerValue
 */
function outer(outerValue) {
    /**
     * @template ShadowValue
     * @param {Envelope<ShadowValue>} innerValue
     */
    function inner(innerValue) { outerValue = innerValue; }
}
"#;
    let mut parser = ParserState::new("applications.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let outer = arena
        .get_source_file(arena.get(root).expect("source-file node"))
        .expect("source-file data")
        .statements
        .nodes[0];
    let outer_function = arena
        .get_function(arena.get(outer).expect("outer function node"))
        .expect("outer function data");
    let inner = arena
        .get_block(arena.get(outer_function.body).expect("outer body node"))
        .expect("outer body data")
        .statements
        .nodes[0];

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "applications.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );

    let outer_jsdoc = checker
        .get_jsdoc_for_function(outer)
        .expect("outer function JSDoc");
    let (_, outer_updates) =
        checker.push_jsdoc_template_type_parameters_for_owner(outer, &outer_jsdoc);
    let outer_param = checker.ctx.type_parameter_scope["ShadowValue"];
    let outer_envelope = checker
        .resolve_jsdoc_reference("Envelope<ShadowValue>")
        .expect("outer alias application");

    let inner_jsdoc = checker
        .get_jsdoc_for_function(inner)
        .expect("inner function JSDoc");
    let (_, inner_updates) =
        checker.push_jsdoc_template_type_parameters_for_owner(inner, &inner_jsdoc);
    let inner_param = checker.ctx.type_parameter_scope["ShadowValue"];
    let inner_envelope = checker
        .resolve_jsdoc_reference("Envelope<ShadowValue>")
        .expect("inner alias application");

    assert_ne!(outer_param, inner_param);
    assert_ne!(outer_envelope, inner_envelope);
    assert!(
        !tsz_solver::relations::subtype::SubtypeChecker::new(&types)
            .is_subtype_of(inner_envelope, outer_envelope),
        "the reduced alias bodies must retain their free declaration origins"
    );

    checker.pop_type_parameters(inner_updates);
    checker.pop_type_parameters(outer_updates);

    let outer_type = checker.get_type_of_function(outer);
    let inner_type = checker.get_type_of_function(inner);
    let outer_shape = crate::query_boundaries::common::function_shape_for_type(&types, outer_type)
        .expect("outer function shape");
    let inner_shape = crate::query_boundaries::common::function_shape_for_type(&types, inner_type)
        .expect("inner function shape");
    assert_ne!(outer_shape.params[0].type_id, inner_shape.params[0].type_id);
    assert!(
        !tsz_solver::relations::subtype::SubtypeChecker::new(&types)
            .is_subtype_of(inner_shape.params[0].type_id, outer_shape.params[0].type_id),
        "function signatures must retain the reduced bodies' free origins"
    );

    let inner_function = arena
        .get_function(arena.get(inner).expect("inner function node"))
        .expect("inner function data");
    let assignment_statement = arena
        .get_block(arena.get(inner_function.body).expect("inner body node"))
        .expect("inner body data")
        .statements
        .nodes[0];
    let assignment_expression = arena
        .get_expression_statement(
            arena
                .get(assignment_statement)
                .expect("assignment statement node"),
        )
        .expect("assignment statement data")
        .expression;
    let assignment = arena
        .get(assignment_expression)
        .and_then(|expression| arena.get_binary_expr(expression))
        .expect("assignment expression");
    let lhs = checker.get_type_of_node(assignment.left);
    let rhs = checker.get_type_of_node(assignment.right);
    assert_ne!(lhs, rhs);
    assert!(
        !tsz_solver::relations::subtype::SubtypeChecker::new(&types).is_subtype_of(rhs, lhs),
        "identifier reads must preserve the two cached parameter types"
    );
    assert!(
        !checker.is_assignable_to(rhs, lhs),
        "the checker assignability gateway must retain the free origins"
    );

    let checked_types = TypeInterner::new();
    let mut checked = CheckerState::new(
        arena,
        &binder,
        &checked_types,
        "applications.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    checked.ctx.set_lib_contexts(Vec::new());
    checked.check_source_file(root);
    let checked_lhs = checked.get_type_of_node(assignment.left);
    let checked_rhs = checked.get_type_of_node(assignment.right);
    let checked_assignment_target = checked.get_type_of_assignment_target(assignment.left);
    assert_ne!(
        checked_lhs, checked_rhs,
        "the canonical check pipeline must cache distinct identifier types"
    );
    assert_eq!(
        checked_assignment_target, checked_lhs,
        "the assignment-target query must consume the cached declared parameter type"
    );
    assert!(
        !checked.is_assignable_to(checked_rhs, checked_lhs),
        "the post-check identifier types must remain unassignable"
    );
    let lhs_origins = tsz_solver::visitor::free_decl_scoped_type_parameter_origins_in(
        &checked_types,
        [checked_lhs],
    );
    let rhs_origins = tsz_solver::visitor::free_decl_scoped_type_parameter_origins_in(
        &checked_types,
        [checked_rhs],
    );
    assert_ne!(lhs_origins, rhs_origins);
    assert!(
        crate::query_boundaries::assignability::
            have_same_surface_distinct_decl_scoped_free_type_parameters(
                &checked_types,
                &checked.ctx,
                checked_rhs,
                checked_lhs,
            ),
        "the reduced assignment types must retain one common declared surface: {rhs_origins:?} -> {lhs_origins:?}"
    );
    assert!(
        checked
            .ctx
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == 2719),
        "the assignment boundary must report the unrelated same-surface declarations: {:?}",
        checked
            .ctx
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn identical_constraint_and_default_surfaces_still_get_distinct_binders() {
    for (label, source) in [
        (
            "constraint",
            r#"
/**
 * @template {object} Shape
 * @param {Shape} outerValue
 */
function outer(outerValue) {
    /**
     * @template {object} Shape
     * @param {Shape} innerValue
     */
    function inner(innerValue) { outerValue = innerValue; }
}
"#,
        ),
        (
            "default",
            r#"
/**
 * @template [Item=object]
 * @param {Item} outerValue
 */
function outer(outerValue) {
    /**
     * @template [Item=object]
     * @param {Item} innerValue
     */
    function inner(innerValue) { outerValue = innerValue; }
}
"#,
        ),
    ] {
        let codes = strict_js_codes(source);
        assert!(
            codes.contains(&2719),
            "{label}: identical declaration surfaces must still report TS2719, got {codes:?}"
        );
    }
}

#[test]
fn renamed_and_differently_named_shadowing_keep_diagnostic_routing() {
    let renamed = strict_js_codes(
        r#"
/**
 * @template Elem
 * @param {Elem} outerValue
 */
function outer(outerValue) {
    /**
     * @template Elem
     * @param {Elem} innerValue
     */
    function inner(innerValue) { outerValue = innerValue; }
}
"#,
    );
    assert!(renamed.contains(&2719), "renamed binder: {renamed:?}");

    let different = strict_js_codes(
        r#"
/**
 * @template Outer
 * @param {Outer} outerValue
 */
function outer(outerValue) {
    /**
     * @template Inner
     * @param {Inner} innerValue
     */
    function inner(innerValue) { outerValue = innerValue; }
}
"#,
    );
    assert!(
        different.contains(&2322),
        "different binders: {different:?}"
    );
    assert!(
        !different.contains(&2719),
        "different display names must not route to TS2719: {different:?}"
    );
}

#[test]
fn class_and_method_templates_infer_independently_across_surface_matrix() {
    let cases = [
        (
            "plain",
            r#"
/** @template T */
class Box {
    /** @param {T} value */ constructor(value) { this.value = value; }
    /**
     * @template T
     * @param {T} input
     * @returns {T}
     */
    id(input) { return input; }
}
const box = new Box(1);
/** @type {string} */ const result = box.id("ok");
"#,
        ),
        (
            "renamed",
            r#"
/** @template Elem */
class Box {
    /** @param {Elem} value */ constructor(value) { this.value = value; }
    /**
     * @template Elem
     * @param {Elem} input
     * @returns {Elem}
     */
    id(input) { return input; }
}
const box = new Box(1);
/** @type {string} */ const result = box.id("ok");
"#,
        ),
        (
            "constraint",
            r#"
/** @template {object} T */
class Box {
    /** @param {T} value */ constructor(value) { this.value = value; }
    /**
     * @template {object} T
     * @param {T} input
     * @returns {T}
     */
    id(input) { return input; }
}
const box = new Box({ classOnly: 1 });
/** @type {{methodOnly: string}} */ const result = box.id({ methodOnly: "ok" });
"#,
        ),
        (
            "default",
            r#"
/** @template [T=object] */
class Box {
    /** @param {T} value */ constructor(value) { this.value = value; }
    /**
     * @template [T=object]
     * @param {T} input
     * @returns {T}
     */
    id(input) { return input; }
}
const box = new Box({ classOnly: 1 });
/** @type {{methodOnly: string}} */ const result = box.id({ methodOnly: "ok" });
"#,
        ),
    ];

    for (label, source) in cases {
        let codes = strict_js_codes(source);
        let identity_failures: Vec<_> = codes
            .iter()
            .copied()
            .filter(|code| matches!(code, 2304 | 2322 | 2345 | 2719))
            .collect();
        assert!(
            identity_failures.is_empty(),
            "{label}: class and method binders must infer independently, got {codes:?}"
        );
    }
}

#[test]
fn class_expression_method_shadowing_reports_ts2719() {
    let codes = strict_js_codes(
        r#"
/**
 * @template T
 * @param {T} outer
 */
function make(outer) {
    const Local = class {
        /**
         * @template T
         * @param {T} inner
         */
        id(inner) { outer = inner; }
    };
    return Local;
}
"#,
    );
    assert!(codes.contains(&2719), "expected TS2719, got {codes:?}");
}

#[test]
fn non_shadowing_class_template_use_remains_valid() {
    let codes = strict_js_codes(
        r#"
/** @template T */
class Box {
    /** @param {T} value */ constructor(value) { this.value = value; }
    /** @param {T} input */ set(input) { this.value = input; }
}
new Box(1).set(2);
"#,
    );
    assert!(
        !codes
            .iter()
            .any(|code| matches!(code, 2304 | 2322 | 2345 | 2719)),
        "non-shadowing uses of the class binder must remain valid: {codes:?}"
    );
}

#[test]
fn owner_pushes_reuse_ids_but_distinct_owners_do_not() {
    let source = r#"
/** @template T @param {T} value */
function first(value) { return value; }
/** @template T @param {T} value */
function second(value) { return value; }
"#;
    let mut parser = ParserState::new("owners.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("source-file node");
    let owners = arena
        .get_source_file(root_node)
        .expect("source-file data")
        .statements
        .nodes
        .clone();
    assert_eq!(owners.len(), 2);

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "owners.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );

    let mut ids = Vec::new();
    for &owner in &owners {
        let jsdoc = checker
            .get_jsdoc_for_function(owner)
            .expect("function JSDoc");
        let (params, updates) =
            checker.push_jsdoc_template_type_parameters_for_owner(owner, &jsdoc);
        let first_id = checker.ctx.type_parameter_scope["T"];
        checker.pop_type_parameters(updates);

        let (repeated_params, repeated_updates) =
            checker.push_jsdoc_template_type_parameters_for_owner(owner, &jsdoc);
        let repeated_id = checker.ctx.type_parameter_scope["T"];
        checker.pop_type_parameters(repeated_updates);

        assert_eq!(first_id, repeated_id, "same owner must reuse its TypeId");
        assert_eq!(params, repeated_params, "same owner must reuse its origin");
        assert!(matches!(
            params[0].origin,
            tsz_solver::TypeParamOrigin::JsdocOwnerScoped {
                node,
                ..
            } if node == owner.0
        ));
        ids.push(first_id);
    }
    assert_ne!(ids[0], ids[1], "different owners must not share a TypeId");
}

#[test]
fn cross_arena_same_node_index_uses_the_declaring_file_identity() {
    let source = "/** @template Element */\nclass Container {}";
    let mut left_parser = ParserState::new("left.js".to_string(), source.to_string());
    let left_root = left_parser.parse_source_file();
    let left_arena = left_parser.get_arena();
    let left_owner = left_arena
        .get_source_file(left_arena.get(left_root).expect("left source-file node"))
        .expect("left source-file data")
        .statements
        .nodes[0];

    let mut right_parser = ParserState::new("right.js".to_string(), source.to_string());
    let right_root = right_parser.parse_source_file();
    let right_arena = right_parser.get_arena();
    let right_owner = right_arena
        .get_source_file(right_arena.get(right_root).expect("right source-file node"))
        .expect("right source-file data")
        .statements
        .nodes[0];
    assert_eq!(
        left_owner, right_owner,
        "identical parses should exercise the same numeric NodeIndex in two arenas"
    );

    let mut left_binder = tsz_binder::BinderState::new();
    left_binder.bind_source_file(left_arena, left_root);
    let types = TypeInterner::new();
    let checker = CheckerState::new(
        left_arena,
        &left_binder,
        &types,
        "left.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );

    let extract = |arena, owner, name| {
        checker
            .extract_simple_type_params_from_decl_in_arena(
                arena,
                tsz_binder::symbol_flags::CLASS,
                owner,
                name,
            )
            .expect("cross-arena class type parameters")
    };
    let left = extract(left_arena, left_owner, "Container");
    let right = extract(right_arena, right_owner, "Container");
    let repeated_right = extract(right_arena, right_owner, "Container");
    assert_eq!(left.len(), 1);
    assert_eq!(right.len(), 1);
    assert_eq!(
        right, repeated_right,
        "cross-arena reconstruction must be stable"
    );

    let origin_parts = |origin| match origin {
        tsz_solver::TypeParamOrigin::JsdocOwnerScoped { file, node } => (file, node),
        other => panic!("expected owner-scoped JSDoc binder, got {other:?}"),
    };
    let (left_file, left_node) = origin_parts(left[0].origin);
    let (right_file, right_node) = origin_parts(right[0].origin);
    assert_eq!(left_node, right_node);
    assert_eq!(types.resolve_atom(left_file), "left.js");
    assert_eq!(types.resolve_atom(right_file), "right.js");
    assert_ne!(left[0].origin, right[0].origin);
    assert_ne!(
        types.factory().type_param(left[0]),
        types.factory().type_param(right[0]),
        "equal NodeIndex values in different files must not collide"
    );
}

#[test]
fn identical_overload_comments_have_distinct_stable_binders() {
    let source = r#"
/**
 * @overload
 * @template T
 * @param {T} value
 * @returns {T}
 */
/**
 * @overload
 * @template T
 * @param {T} value
 * @returns {T}
 */
/**
 * @param {*} value
 * @returns {*}
 */
function identity(value) { return value; }
"#;
    let mut parser = ParserState::new("overloads.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("source-file node");
    let function_idx = arena
        .get_source_file(root_node)
        .expect("source-file data")
        .statements
        .nodes[0];
    let function_node = arena.get(function_idx).expect("function node");
    let function = arena.get_function(function_node).expect("function data");

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "overloads.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );

    let signatures = checker.jsdoc_overload_call_signatures_for_function(function, function_idx);
    let repeated = checker.jsdoc_overload_call_signatures_for_function(function, function_idx);
    assert_eq!(signatures.len(), 2);
    assert_eq!(repeated.len(), 2);

    let infos: Vec<_> = signatures.iter().map(|sig| sig.type_params[0]).collect();
    let repeated_infos: Vec<_> = repeated.iter().map(|sig| sig.type_params[0]).collect();
    assert_eq!(
        infos, repeated_infos,
        "overload reconstruction must be stable"
    );
    assert_ne!(infos[0].origin, infos[1].origin);
    let comment_positions: Vec<_> = infos
        .iter()
        .map(|info| match info.origin {
            tsz_solver::TypeParamOrigin::JsdocCommentScoped { pos, .. } => pos,
            other => panic!("expected comment-scoped overload binder, got {other:?}"),
        })
        .collect();
    assert_ne!(comment_positions[0], comment_positions[1]);

    let ids: Vec<_> = infos
        .iter()
        .map(|info| checker.ctx.types.factory().type_param(*info))
        .collect();
    assert_ne!(
        ids[0], ids[1],
        "separate overload comments need separate binders"
    );
}

#[test]
fn comment_binder_identity_survives_incremental_arena_growth_and_is_node_disjoint() {
    use tsz_common::comments::get_jsdoc_content;

    let initial_source = "/** @template T */\nconst marker = 1;";
    let file_name = "incremental.js";
    let mut parser = ParserState::new(file_name.to_string(), initial_source.to_string());
    let root = parser.parse_source_file();
    let comment = parser
        .get_arena()
        .source_files
        .first()
        .and_then(|source_file| source_file.comments.first())
        .cloned()
        .expect("JSDoc comment");
    let jsdoc = get_jsdoc_content(&comment, initial_source);
    let initial_arena_len = parser.get_arena().len();
    let old_pseudo_node = u32::try_from(initial_arena_len).expect("arena length fits u32");
    assert!(
        parser.get_arena().get(NodeIndex(old_pseudo_node)).is_none(),
        "the old arena-length pseudo-node starts outside the arena"
    );

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let (before_id, before_info) = {
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            file_name.to_string(),
            CheckerOptions {
                allow_js: true,
                check_js: true,
                ..CheckerOptions::default()
            },
        );
        let (params, updates) =
            checker.push_jsdoc_template_type_parameters_for_comment(comment.pos, &jsdoc);
        let type_id = checker.ctx.type_parameter_scope["T"];
        checker.pop_type_parameters(updates);
        (type_id, params[0])
    };

    let edited_source = format!("{initial_source}\nconst appended_after_parse = 2;");
    let incremental = parser.parse_source_file_statements_from_offset(
        file_name.to_string(),
        edited_source,
        u32::try_from(initial_source.len()).expect("source length fits u32"),
    );
    assert_eq!(incremental.statements.nodes.len(), 1);
    assert!(parser.get_arena().len() > initial_arena_len);
    assert!(
        parser.get_arena().get(NodeIndex(old_pseudo_node)).is_some(),
        "incremental parsing turns the old pseudo-node value into a real NodeIndex"
    );

    let (after_id, after_info) = {
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            file_name.to_string(),
            CheckerOptions {
                allow_js: true,
                check_js: true,
                ..CheckerOptions::default()
            },
        );
        let (params, updates) =
            checker.push_jsdoc_template_type_parameters_for_comment(comment.pos, &jsdoc);
        let type_id = checker.ctx.type_parameter_scope["T"];
        checker.pop_type_parameters(updates);
        (type_id, params[0])
    };

    assert_eq!(before_info.origin, after_info.origin);
    assert_eq!(before_id, after_id);
    assert!(matches!(
        after_info.origin,
        tsz_solver::TypeParamOrigin::JsdocCommentScoped { pos, .. } if pos == comment.pos
    ));

    let file = types.intern_string(file_name);
    let syntax_info = tsz_solver::TypeParamInfo {
        origin: tsz_solver::TypeParamOrigin::DeclScoped {
            file,
            node: old_pseudo_node,
        },
        ..after_info
    };
    let same_payload_comment_info = tsz_solver::TypeParamInfo {
        origin: tsz_solver::TypeParamOrigin::JsdocCommentScoped {
            file,
            pos: old_pseudo_node,
        },
        ..after_info
    };
    let syntax_id = types.factory().type_param(syntax_info);
    let same_payload_comment_id = types.factory().type_param(same_payload_comment_info);
    assert_ne!(syntax_info.origin, same_payload_comment_info.origin);
    assert_ne!(syntax_id, same_payload_comment_id);
}

#[test]
fn one_comment_gives_sibling_template_parameters_distinct_stable_binders() {
    use tsz_common::comments::get_jsdoc_content;

    let source = "/** @template T, U */\nconst marker = 1;";
    let file_name = "siblings.js";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let comment = parser
        .get_arena()
        .source_files
        .first()
        .and_then(|source_file| source_file.comments.first())
        .cloned()
        .expect("JSDoc comment");
    let jsdoc = get_jsdoc_content(&comment, source);

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );

    let (first_params, first_updates) =
        checker.push_jsdoc_template_type_parameters_for_comment(comment.pos, &jsdoc);
    assert_eq!(first_params.len(), 2);
    let first_ids = [
        checker.ctx.type_parameter_scope["T"],
        checker.ctx.type_parameter_scope["U"],
    ];
    checker.pop_type_parameters(first_updates);

    assert_eq!(first_params[0].origin, first_params[1].origin);
    assert_ne!(first_params[0].name, first_params[1].name);
    assert!(!first_params[0].is_same_binder(first_params[1]));
    assert_ne!(first_ids[0], first_ids[1]);

    let (repeated_params, repeated_updates) =
        checker.push_jsdoc_template_type_parameters_for_comment(comment.pos, &jsdoc);
    let repeated_ids = [
        checker.ctx.type_parameter_scope["T"],
        checker.ctx.type_parameter_scope["U"],
    ];
    checker.pop_type_parameters(repeated_updates);

    assert_eq!(repeated_params, first_params);
    assert_eq!(repeated_ids, first_ids);
}

#[test]
fn nested_typedef_and_callback_comments_do_not_capture_outer_template() {
    use tsz_common::comments::get_jsdoc_content;

    let source = r#"
/**
 * @template T
 * @param {T} outer
 */
function host(outer) {
    /**
     * @template T
     * @typedef {{ value: T }} Box
     */
    /**
     * @template T
     * @callback Identity
     * @param {T} value
     * @returns {T}
     */
    /** @type {Box<string>} */
    const box = { value: "ok" };
    return [outer, box];
}
"#;
    let mut parser = ParserState::new("typedefs.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("source-file node");
    let owner = arena
        .get_source_file(root_node)
        .expect("source-file data")
        .statements
        .nodes[0];
    let source_file = arena.source_files.first().expect("source file");
    let comments = source_file.comments.clone();
    let source_text = source_file.text.to_string();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "typedefs.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );

    let owner_jsdoc = checker
        .get_jsdoc_for_function(owner)
        .expect("outer function JSDoc");
    let (outer_params, outer_updates) =
        checker.push_jsdoc_template_type_parameters_for_owner(owner, &owner_jsdoc);
    let outer_id = checker.ctx.type_parameter_scope["T"];

    let use_pos = comments
        .iter()
        .find(|comment| get_jsdoc_content(comment, &source_text).contains("@type {Box"))
        .expect("typedef use comment")
        .pos;
    checker.ctx.jsdoc_typedef_anchor_pos.set(use_pos);

    let mut declared = Vec::new();
    for name in ["Box", "Identity"] {
        let (body, params) = checker
            .resolve_jsdoc_typedef_info(name, &comments, &source_text)
            .unwrap_or_else(|| panic!("missing {name} definition"));
        assert_eq!(params.len(), 1, "{name} must have one template binder");
        if name == "Identity" {
            let binder_id = checker.ctx.types.factory().type_param(params[0]);
            let signature =
                crate::query_boundaries::common::function_shape_for_type(checker.ctx.types, body)
                    .expect("callback function shape");
            assert_eq!(signature.params[0].type_id, binder_id);
            assert_eq!(signature.return_type, binder_id);
        }
        declared.push(params[0]);
    }

    assert_ne!(declared[0].origin, declared[1].origin);
    let comment_positions: Vec<_> = declared
        .iter()
        .map(|param| match param.origin {
            tsz_solver::TypeParamOrigin::JsdocCommentScoped { pos, .. } => pos,
            other => panic!("expected comment-scoped typedef binder, got {other:?}"),
        })
        .collect();
    assert_ne!(comment_positions[0], comment_positions[1]);
    assert!(
        declared
            .iter()
            .all(|param| param.origin != outer_params[0].origin)
    );
    assert!(
        declared
            .iter()
            .all(|param| { checker.ctx.types.factory().type_param(*param) != outer_id })
    );
    assert_eq!(checker.ctx.type_parameter_scope["T"], outer_id);

    checker.pop_type_parameters(outer_updates);
}

#[test]
fn nested_generic_callback_use_does_not_capture_outer_template() {
    let codes = strict_js_codes(
        r#"
/**
 * @template T
 * @param {T} outer
 */
function make(outer) {
    /**
     * @template T
     * @callback Identity
     * @param {T} value
     * @returns {T}
     */
    /** @type {Identity<string>} */
    const identity = value => value;
    const result = identity("ok");
    /** @type {string} */
    const text = result;
    return [outer, text];
}
"#,
    );

    assert!(
        !codes
            .iter()
            .any(|code| matches!(code, 2304 | 2314 | 2322 | 2345 | 2719)),
        "callback inference must not leak the outer declaration's T: {codes:?}"
    );
}
