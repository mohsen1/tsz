//! Tests for TS2315 emission on explicit `type X = any` aliases used
//! with type arguments.
//!
//! tsc 6.0.3 emits "Type 'X' is not generic" (TS2315) for any non-
//! generic type alias called with type arguments, including those
//! whose body is `any`. The previous tsz guard suppressed TS2315 when
//! the resolved symbol type was `any` to avoid false positives on
//! cross-arena lib symbols whose declarations couldn't be located —
//! that guard over-suppressed for explicit `type X = any`
//! declarations and the diagnostic instead surfaced as a cascading
//! TS2344 on the wrapping `Equal<...>` expression.
//!
//! Test fixtures: type-challenges wrong-code cluster (#4908, #4909,
//! #4913, #4915, #4917, #4923, #4927, #4929, #4932, #4935, #4936,
//! #4937, #4940).

use tsz_checker::test_utils::check_source_diagnostics;

#[test]
fn ts2315_fires_on_explicit_any_alias_called_with_type_args() {
    // Canonical case from #4929: `type Chunk = any` declared, then used
    // with type arguments. tsc 6.0.3 emits TS2315 ("Type 'Chunk' is
    // not generic.") on each call site.
    let source = r#"
type Chunk = any
type x = Chunk<1>
type y = Chunk<1, 2>
"#;
    let diags = check_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.iter().filter(|&&c| c == 2315).count() >= 2,
        "expected TS2315 on both Chunk<...> usages. got: {codes:?}"
    );
    assert!(
        !codes.contains(&2344),
        "TS2344 must not fire — Chunk is non-generic, not constraint-bearing. got: {codes:?}"
    );
}

/// Anti-hardcoding cover: a different alias name and a different
/// number of type arguments. The fix must key on the *structural*
/// shape of the alias body (`TypeReference` to identifier `any`), not
/// on `Chunk` specifically.
#[test]
fn ts2315_renamed_alias_with_explicit_any_body() {
    let source = r#"
type Wrapper = any
type a = Wrapper<string>
type b = Wrapper<number, boolean, void>
"#;
    let diags = check_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.iter().filter(|&&c| c == 2315).count() >= 2,
        "expected TS2315 on both Wrapper<...> usages. got: {codes:?}"
    );
}

#[test]
fn ts2315_fires_on_parenthesized_explicit_any_alias_body() {
    let source = r#"
type Wrapped = ((any))
type x = Wrapped<1>
"#;
    let diags = check_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2315),
        "expected TS2315 through parenthesized explicit-any alias body. got: {codes:?}"
    );
    assert!(
        !codes.contains(&2344),
        "parenthesized explicit-any aliases must not cascade into TS2344. got: {codes:?}"
    );
}

/// Negative cover: a generic alias whose body is `any` is still a
/// generic type and must NOT produce TS2315 when called with the
/// declared number of type arguments.
#[test]
fn no_ts2315_on_generic_any_alias_called_correctly() {
    let source = r#"
type Generic<T> = any
type ok = Generic<string>
"#;
    let diags = check_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2315),
        "TS2315 must not fire — Generic<T> is generic. got: {codes:?}"
    );
}

/// Negative cover: a non-generic alias whose body is NOT `any` (e.g.
/// `string`) already fires TS2315 in main. Locks the rule from
/// silently regressing on the existing behavior.
#[test]
fn ts2315_still_fires_on_non_generic_non_any_alias() {
    let source = r#"
type Plain = string
type x = Plain<1>
"#;
    let diags = check_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2315),
        "TS2315 must fire on `Plain<1>` (Plain is non-generic). got: {codes:?}"
    );
}
