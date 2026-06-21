//! Regression for `typeof <param>` inside a type-predicate's asserted type of a
//! function-**type** alias (and related signature forms).
//!
//! A signature's value parameters are in scope for *every* type position of that
//! signature, including a type-predicate's asserted type. So in
//! `type Guard = (a: T) => a is typeof a & U` the `typeof a` must resolve to the
//! parameter's declared type. tsz seeded `typeof_param_scope` for the return-type
//! annotation but `check_type_predicate_assignability` re-checked the predicate's
//! asserted type without the scope, fabricating a TS2304.
//!
//! The fix seeds `typeof_param_scope` from the lowered function type's parameters
//! before re-checking the predicate's asserted type. These cases lock in:
//!   1. No spurious TS2304 for `typeof <param>` in the predicate's asserted type.
//!   2. The rule is structural — parameter renaming must not change behavior.
//!   3. `typeof <param>` resolving a *different* parameter is also in scope.
//!   4. The previously-working return-type position keeps working.
//!   5. A genuinely undeclared name in the asserted type still errors (no
//!      over-broad suppression).

use tsz_checker::context::CheckerOptions;

fn check(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn ts2304(diags: &[(u32, String)]) -> Vec<&(u32, String)> {
    diags.iter().filter(|(c, _)| *c == 2304).collect()
}

#[test]
fn function_type_predicate_typeof_param_resolves() {
    let source = r#"
type Guard = (a: { z: string }) => a is typeof a & { y: boolean };
export {};
"#;
    let diags = check(source);
    assert!(
        ts2304(&diags).is_empty(),
        "`typeof a` in a function-type predicate's asserted type must resolve via the parameter scope, but got: {diags:?}"
    );
}

#[test]
fn function_type_predicate_typeof_param_resolves_with_alternate_name() {
    // The fix is structural: renaming the parameter must not change behavior.
    let source = r#"
type Guard = (payload: { z: string }) => payload is typeof payload & { y: boolean };
export {};
"#;
    let diags = check(source);
    assert!(
        ts2304(&diags).is_empty(),
        "Renaming the parameter must not change resolution of `typeof <param>`: {diags:?}"
    );
}

#[test]
fn function_type_predicate_typeof_other_param_resolves() {
    // `typeof` of a *different* parameter of the same signature is in scope too.
    let source = r#"
type Guard = (a: { z: string }, b: { w: number }) => a is typeof b & { y: boolean };
export {};
"#;
    let diags = check(source);
    assert!(
        ts2304(&diags).is_empty(),
        "`typeof b` referencing a sibling parameter must resolve in the predicate's asserted type: {diags:?}"
    );
}

#[test]
fn function_type_predicate_typeof_param_in_return_position_still_resolves() {
    // The previously-working return-type position must keep working.
    let source = r#"
type Ret = (a: { z: string }) => typeof a;
export {};
"#;
    let diags = check(source);
    assert!(
        ts2304(&diags).is_empty(),
        "`typeof a` in an ordinary return type must keep resolving: {diags:?}"
    );
}

#[test]
fn function_type_predicate_typeof_undeclared_name_still_errors() {
    // Negative control: a genuinely undeclared name in the asserted type must
    // still produce TS2304 — the fix must not over-broadly suppress the error.
    let source = r#"
type Guard = (a: { z: string }) => a is typeof undeclared & { y: boolean };
export {};
"#;
    let diags = check(source);
    assert!(
        !ts2304(&diags).is_empty(),
        "`typeof undeclared` must still report TS2304: {diags:?}"
    );
}
