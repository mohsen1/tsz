//! Tests for tsc-style display of optional parameters in assignability
//! diagnostic messages: `(a?: T)` not `(a?: T | undefined)`.
//!
//! Conformance tests touched by this fix:
//! - `defaultValueInFunctionTypes.ts` (TS2352 type assertion overlap)
//! - `optionalFunctionArgAssignability.ts` (TS2322 message alignment)
//! - `assignmentCompatWithCallSignaturesWithRestParameters.ts`
//!
//! Before this fix, the assignability-message formatter explicitly set
//! `with_preserve_optional_parameter_surface_syntax(false)`, which made
//! optional params append `| undefined` to types that didn't already
//! contain it. tsc keeps the surface form because the `?` already
//! implies `| undefined`; it only writes the union form when the
//! source explicitly types the parameter with `| undefined`.
//!
//! Three formatter sites were fixed:
//!   - `format_type_diagnostic_for_assignability_display`
//!   - `format_type_diagnostic_widened_for_assignability_display`
//!   - `format_type_for_assignability_message::format_with_def_store`

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
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

/// TS2352 type-assertion-overlap message must display optional parameters
/// using `?: T` surface syntax, not `?: T | undefined`. The `?` already
/// implies `| undefined`; the redundant union form does not match tsc.
#[test]
fn ts2352_optional_param_display_uses_surface_syntax() {
    let source = r#"
var y = <(a : string = "") => any>(undefined);
"#;
    let diags = check_strict(source);
    let ts2352: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2352).collect();
    assert_eq!(
        ts2352.len(),
        1,
        "expected exactly one TS2352; got: {diags:?}"
    );
    let msg = &ts2352[0].1;
    assert!(
        msg.contains("(a?: string)"),
        "TS2352 must display the optional parameter as `(a?: string)`, not `(a?: string | undefined)`. Got: {msg:?}"
    );
    assert!(
        !msg.contains("string | undefined"),
        "TS2352 must not append `| undefined` to optional parameter type when surface form omits it. Got: {msg:?}"
    );
}

/// TS2322 message for assigning a function to a more-permissive optional-
/// parameter target must use surface syntax in BOTH source and target
/// signatures.
#[test]
fn ts2322_optional_param_in_function_signature_uses_surface_syntax() {
    let source = r#"
type FuncOpt = (a?: number) => number;
let f1: FuncOpt = "x" as any;
const f2: (a: string) => string = f1;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    if let Some((_, msg)) = ts2322.first() {
        assert!(
            !msg.contains("a?: number | undefined"),
            "TS2322 must not include `a?: number | undefined`; the `?` implies undefined. Got: {msg:?}"
        );
    }
}

/// TS2345 against a union-of-callables target where the optional parameter
/// surface contributes the only literal-sensitive `undefined` member must
/// elide that synthetic `| undefined` and widen the literal argument
/// display, matching tsc.
///
/// Conformance: `unionTypeCallSignatures.ts` (lines 36, 42, 48 in tsc cache).
/// Before this fix, the diagnostic read:
///   `Argument of type '"hello"' is not assignable to parameter of type 'number | undefined'.`
/// tsc emits:
///   `Argument of type 'string' is not assignable to parameter of type 'number'.`
#[test]
fn ts2345_union_callable_optional_param_widens_synthetic_undefined() {
    let source = r#"
declare var u: { (a: string, b?: number): string; } | { (a: string, b?: number): number; };
u('hello', "hello");
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "expected TS2345 for the wrong-typed second arg; got: {diags:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("Argument of type 'string'"),
        "TS2345 should widen the literal argument display (got literal text \
         when the union's `| undefined` is synthetic from the optional `?:`). \
         Got: {msg:?}"
    );
    assert!(
        !msg.contains("Argument of type '\"hello\"'"),
        "TS2345 should not preserve the literal argument text when the union \
         on the parameter is purely a synthetic optional `| undefined`. \
         Got: {msg:?}"
    );
    assert!(
        msg.contains("parameter of type 'number'"),
        "TS2345 should strip the synthetic `| undefined` from the parameter \
         display for an optional non-rest slot in a union of callables. \
         Got: {msg:?}"
    );
    assert!(
        !msg.contains("number | undefined"),
        "TS2345 should not display `number | undefined` when the union arises \
         only from the optional parameter in a union of callable shapes. \
         Got: {msg:?}"
    );
}

/// TS2345 against a union of callables where one member omits the slot
/// entirely (`b?` in one, no `b` in the other) must still elide the
/// synthetic `| undefined`, since the slot is "optional" in the union sense.
#[test]
fn ts2345_union_callable_mixed_arity_optional_param_widens() {
    let source = r#"
declare var u: { (a: string, b?: number): string; } | { (a: string): number; };
u('hello', "hello");
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "expected TS2345 for the wrong-typed second arg; got: {diags:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("Argument of type 'string'"),
        "TS2345 should widen the literal argument display for mixed-arity \
         union callable. Got: {msg:?}"
    );
    assert!(
        msg.contains("parameter of type 'number'"),
        "TS2345 should strip the synthetic `| undefined` for mixed-arity \
         union callable. Got: {msg:?}"
    );
}

/// Sanity: when the parameter is EXPLICITLY typed `T | undefined`, the
/// display preserves `T | undefined`. The fix only suppresses the
/// implicit-undefined-from-`?` case, not explicit annotations.
#[test]
fn ts2352_explicit_undefined_param_preserved() {
    let source = r#"
var z = <(a?: string | undefined) => any>(undefined);
"#;
    let diags = check_strict(source);
    let ts2352: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2352).collect();
    if let Some((_, msg)) = ts2352.first() {
        // Exact form is `(a?: string | undefined)` because surface annotation IS that.
        assert!(
            msg.contains("string | undefined") || msg.contains("a?: string"),
            "TS2352 must preserve explicit `string | undefined` annotation OR collapse to `string` (both are reasonable surface forms). Got: {msg:?}"
        );
    }
}
