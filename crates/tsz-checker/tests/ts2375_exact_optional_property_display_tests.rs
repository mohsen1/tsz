//! Tests for exact optional property type display in TS2375 diagnostic messages.
//!
//! When `exactOptionalPropertyTypes: true`, `foo?: T` means the property is
//! either absent or holds a value of type `T` — it does NOT implicitly include
//! `undefined`. Diagnostic messages must display the target type as `{ foo?: T }`
//! not `{ foo?: T | undefined }`.
//!
//! Conformance test: `strictOptionalProperties1.ts`
//! Root cause: `TypeFormatter` was appending `| undefined` to optional property
//! types even with `exactOptionalPropertyTypes: true`. Fixed by
//! `with_exact_optional_property_types(bool)` on `TypeFormatter`.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn check_strict_exact_optional(source: &str) -> Vec<(u32, String)> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            exact_optional_property_types: true,
            ..Default::default()
        },
    )
}

fn check_strict_no_exact(source: &str) -> Vec<(u32, String)> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            exact_optional_property_types: false,
            ..Default::default()
        },
    )
}

/// With `exactOptionalPropertyTypes: true`, assigning `{ foo: undefined }` to
/// `{ foo?: number }` should produce a TS2375 message that shows the target as
/// `{ foo?: number }`, not `{ foo?: number | undefined }`.
///
/// tsc: `Type '{ foo: undefined; }' is not assignable to type '{ foo?: number; }'
///       with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to
///       the types of the target's properties.`
#[test]
fn ts2375_target_type_does_not_append_undefined_with_exact_optional() {
    let source = r#"
const x: { foo?: number } = { foo: undefined };
"#;
    let diags = check_strict_exact_optional(source);
    let ts2375: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2375).collect();
    assert!(
        !ts2375.is_empty(),
        "expected TS2375 for assigning undefined to an exact-optional property; got: {diags:?}"
    );
    let msg = &ts2375[0].1;
    assert!(
        msg.contains("foo?: number"),
        "TS2375 target type must display `foo?: number` (not `foo?: number | undefined`) with exactOptionalPropertyTypes. Got: {msg:?}"
    );
    assert!(
        !msg.contains("number | undefined"),
        "TS2375 must not append `| undefined` to optional property type when exactOptionalPropertyTypes is true. Got: {msg:?}"
    );
}

/// Without `exactOptionalPropertyTypes`, assigning `{ foo: undefined }` to
/// `{ foo?: number }` should NOT produce TS2375 (the property implicitly
/// includes undefined). Existing behavior must be preserved.
#[test]
fn ts2375_not_emitted_without_exact_optional_property_types() {
    let source = r#"
const x: { foo?: number } = { foo: undefined };
"#;
    let diags = check_strict_no_exact(source);
    let ts2375: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2375).collect();
    assert!(
        ts2375.is_empty(),
        "TS2375 must not be emitted without exactOptionalPropertyTypes. Got: {diags:?}"
    );
}

/// With `exactOptionalPropertyTypes: true`, a simple valid assignment must
/// not produce any diagnostic — ensures the fix doesn't cause false positives.
#[test]
fn no_false_positive_for_valid_optional_property_assignment() {
    let source = r#"
const x: { foo?: number } = { foo: 42 };
"#;
    let diags = check_strict_exact_optional(source);
    assert!(
        diags.is_empty(),
        "valid assignment to optional property must produce no diagnostics. Got: {diags:?}"
    );
}

/// With `exactOptionalPropertyTypes: true`, assigning `undefined` to a property
/// explicitly typed `string | undefined` (not just `string?`) must NOT produce
/// TS2375 — only explicitly-undefined-typed properties may hold undefined.
#[test]
fn ts2375_not_emitted_for_explicit_undefined_union_type() {
    let source = r#"
const x: { foo?: string | undefined } = { foo: undefined };
"#;
    let diags = check_strict_exact_optional(source);
    let ts2375: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2375).collect();
    assert!(
        ts2375.is_empty(),
        "TS2375 must not be emitted when the property type explicitly includes undefined. Got: {diags:?}"
    );
}
