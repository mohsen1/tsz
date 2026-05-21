use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

fn has_ts2322(source: &str) -> bool {
    codes(source).contains(&2322)
}

fn no_errors(source: &str) -> bool {
    codes(source).is_empty()
}

// ── Reported bug: `${any}` must not collapse to `string` ─────────────────────

/// tsc: TS2322 — `string` is not assignable to `` `${any}` ``.
/// tsz was wrongly accepting this (false-negative) by collapsing `${any}` → string.
#[test]
fn any_placeholder_rejects_string_assignment() {
    assert!(
        has_ts2322("declare const s: string; const a: `${any}` = s;"),
        "string must not be assignable to `${{any}}`"
    );
}

/// String literals ARE assignable to `` `${any}` `` — `any` acts as a wildcard.
#[test]
fn any_placeholder_accepts_string_literal() {
    assert!(
        no_errors(r#"const a: `${any}` = "hello";"#),
        "string literal must be assignable to `${{any}}`"
    );
}

/// Different literal values all match the `${any}` wildcard.
#[test]
fn any_placeholder_accepts_various_string_literals() {
    assert!(
        no_errors(
            r#"
const a: `${any}` = "x";
const b: `${any}` = "hello world";
const c: `${any}` = "";
"#
        ),
        "all string literals must be assignable to `${{any}}`"
    );
}

/// `any` is assignable to `` `${any}` `` (any bypasses structural checks).
#[test]
fn any_value_is_assignable_to_any_placeholder() {
    assert!(
        no_errors("declare const v: any; const a: `${any}` = v;"),
        "any must be assignable to `${{any}}`"
    );
}

// ── `${unknown}` — same deferred behavior ────────────────────────────────────

/// `string` must not be assignable to `` `${unknown}` `` either.
#[test]
fn unknown_placeholder_rejects_string_assignment() {
    assert!(
        has_ts2322("declare const s: string; const a: `${unknown}` = s;"),
        "string must not be assignable to `${{unknown}}`"
    );
}

/// String literals are assignable to `` `${unknown}` `` (unknown wildcard in pattern matching).
#[test]
fn unknown_placeholder_accepts_string_literal() {
    assert!(
        no_errors(r#"const a: `${unknown}` = "hello";"#),
        "string literal must be assignable to `${{unknown}}`"
    );
}

// ── Prefix/suffix templates with `any` ───────────────────────────────────────

/// `` a${any}b `` accepts a string literal that matches the surrounding text.
#[test]
fn prefixed_any_accepts_matching_literal() {
    assert!(
        no_errors(r#"const a: `prefix-${any}-suffix` = "prefix-xyz-suffix";"#),
        "matching literal must be assignable to `prefix-${{any}}-suffix`"
    );
}

/// `` a${any}b `` must reject a bare `string` — the fixed text constrains assignability.
#[test]
fn prefixed_any_rejects_string() {
    assert!(
        has_ts2322("declare const s: string; const a: `prefix-${any}-suffix` = s;"),
        "string must not be assignable to `prefix-${{any}}-suffix`"
    );
}

/// `` a${unknown}b `` accepts a string literal matching the surrounding text.
#[test]
fn prefixed_unknown_accepts_matching_literal() {
    assert!(
        no_errors(r#"const a: `hello-${unknown}-world` = "hello-foo-world";"#),
        "matching literal must be assignable to `hello-${{unknown}}-world`"
    );
}

/// `` a${unknown}b `` must reject a bare `string`.
#[test]
fn prefixed_unknown_rejects_string() {
    assert!(
        has_ts2322("declare const s: string; const a: `hello-${unknown}-world` = s;"),
        "string must not be assignable to `hello-${{unknown}}-world`"
    );
}

// ── Control: `${string}` must still accept `string` ──────────────────────────

/// `` `${string}` `` collapses to `string` per tsc — must continue to accept `string`.
#[test]
fn string_placeholder_accepts_string() {
    assert!(
        no_errors("declare const s: string; const a: `${string}` = s;"),
        "`${{string}}` must accept string (it collapses to string)"
    );
}

/// `` `${string}` `` also accepts string literals.
#[test]
fn string_placeholder_accepts_literal() {
    assert!(
        no_errors(r#"const a: `${string}` = "hello";"#),
        "`${{string}}` must accept string literals"
    );
}

// ── Structural rule: identifier names do not affect behaviour (§25) ───────────

/// Same rule holds regardless of what name the type alias uses.
/// If the fix were hardcoded to a specific spelling it would break here.
#[test]
fn any_placeholder_rejects_string_via_alias() {
    assert!(
        has_ts2322(
            r#"
type AnyTemplate = `${any}`;
declare const s: string;
const a: AnyTemplate = s;
"#
        ),
        "string must not be assignable to `${{any}}` through a type alias"
    );
}

/// `${any}` rejects string even when wrapped inside another generic context.
#[test]
fn any_placeholder_rejects_string_in_generic_alias() {
    assert!(
        has_ts2322(
            r#"
type Tmpl<_T> = `${any}`;
declare const s: string;
const a: Tmpl<number> = s;
"#
        ),
        "string must not be assignable to `${{any}}` through a generic alias"
    );
}

// ── Adjacent cases: multi-placeholder templates ───────────────────────────────

/// A two-span template `` `${any}-${any}` `` rejects bare `string`.
#[test]
fn double_any_placeholder_rejects_string() {
    assert!(
        has_ts2322("declare const s: string; const a: `${any}-${any}` = s;"),
        "string must not be assignable to `${{any}}-${{any}}`"
    );
}

/// A two-span template `` `${any}-${any}` `` accepts a matching literal.
#[test]
fn double_any_placeholder_accepts_matching_literal() {
    assert!(
        no_errors(r#"const a: `${any}-${any}` = "foo-bar";"#),
        "matching literal must be assignable to `${{any}}-${{any}}`"
    );
}

/// Mixed template `` `${string}-${any}` `` rejects bare `string` (due to `any` span).
#[test]
fn mixed_string_any_placeholder_rejects_string() {
    assert!(
        has_ts2322("declare const s: string; const a: `${string}-${any}` = s;"),
        "string must not be assignable to `${{string}}-${{any}}`"
    );
}
