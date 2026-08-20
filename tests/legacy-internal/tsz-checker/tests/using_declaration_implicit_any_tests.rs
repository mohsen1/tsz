//! An uninitialized `using` / `await using` binding is const-*like*: it cannot
//! be reassigned, so it never becomes an evolving ("auto") `any` that control
//! flow later fixes. `tsc` reports its implicit-any at the declaration site
//! (TS7005) under `noImplicitAny`, exactly as it does for `const` — and unlike
//! `let` / `var`, which stay silent because a later assignment may still give
//! them a concrete type.
//!
//! tsz's declaration-site implicit-any gate keyed on
//! `is_const_variable_declaration`, which tests only the `CONST` node-flag bit.
//! `await using` carries it (flag `6 = CONST | USING`) and so was reported;
//! plain `using` sets only `USING` (`4`) and slipped through into the deferred
//! `let`/`var` path, which never fires for a binding that can't be reassigned —
//! so `using x;` silently lost its TS7005. The gate now uses
//! `is_var_const_like_declaration` (tsc's `isVarConstLike`: `const` | `using` |
//! `await using`), so all three const-like forms report uniformly.
//!
//! Oracle-verified against `tsc` (`--strict`): `using x;` reports TS1155 +
//! TS7005; `let x;` / `var x;` report neither; `using x: number;` reports only
//! TS1155 (the annotation supplies the type).
//!
//! These tests assert the checker-owned TS7005 half. TS1155 ("must be
//! initialized") is parser-owned since #17251 and lives outside
//! `checker.ctx.diagnostics` (the checker-only harness here), so it is covered
//! exhaustively in the parser's `const_using_uninitialized_grammar_tests.rs`
//! rather than re-asserted through this harness (#17253).

use crate::test_utils::check_source_strict_codes;

/// The core regression: a bare uninitialized `using` reports the implicit-any
/// at its declaration site, not just the must-initialize TS1155.
#[test]
fn uninitialized_using_reports_ts7005_implicit_any() {
    let codes = check_source_strict_codes("using x;\n");
    assert!(
        codes.contains(&7005),
        "a `using` binding is const-like, so its uninitialized implicit-any is \
         reported at the declaration site (TS7005) like `const`; got {codes:?}"
    );
}

/// Parity control that pins the fix's motivation: `await using` already
/// reported TS7005 (its flag carried `CONST`); plain `using` must now match it.
#[test]
fn uninitialized_await_using_and_plain_using_agree_on_ts7005() {
    let plain = check_source_strict_codes("using a;\n");
    let awaited = check_source_strict_codes("async function f() { await using b; }\n");
    assert!(
        plain.contains(&7005),
        "plain `using` must report TS7005; got {plain:?}"
    );
    assert!(
        awaited.contains(&7005),
        "`await using` reports TS7005; got {awaited:?}"
    );
}

/// Negative control: `let` / `var` are NOT const-like — an uninitialized one is
/// an evolving `any` that a later assignment may fix, so tsc stays silent and
/// tsz must too. This is the boundary the fix must not cross.
#[test]
fn uninitialized_let_and_var_do_not_report_ts7005() {
    for src in ["let x;\n", "var x;\n", "let y;\ny;\n", "var z;\nz;\n"] {
        let codes = check_source_strict_codes(src);
        assert!(
            !codes.contains(&7005),
            "`let`/`var` get evolving-any treatment, not a declaration-site \
             TS7005; source {src:?} got {codes:?}"
        );
    }
}

/// An annotated `using` supplies its own type, so the implicit-any gate is
/// correctly skipped — no TS7005 — when a type annotation is present. (The
/// parser still reports the must-initialize TS1155; that half is covered in
/// the parser's grammar tests.)
#[test]
fn annotated_uninitialized_using_reports_no_ts7005() {
    let codes = check_source_strict_codes("using x: number;\n");
    assert!(
        !codes.contains(&7005),
        "the `: number` annotation supplies the type, so no implicit-any; got {codes:?}"
    );
}

/// An initialized `using` has a real type from its initializer — no TS7005.
/// Guards against the gate over-firing on the common valid form.
#[test]
fn initialized_using_reports_no_ts7005() {
    let codes = check_source_strict_codes("using x = null as any;\n");
    assert!(
        !codes.contains(&7005),
        "an initialized `using` is well-formed; got {codes:?}"
    );
}

/// The fix scales to every uninitialized declarator in a multi-declarator
/// list, and leaves initialized siblings alone.
#[test]
fn multi_declarator_using_reports_ts7005_per_uninitialized_binding() {
    let both_uninit = check_source_strict_codes("using a, b;\n");
    assert_eq!(
        both_uninit.iter().filter(|&&c| c == 7005).count(),
        2,
        "each uninitialized `using` declarator earns its own TS7005; got {both_uninit:?}"
    );

    let one_each = check_source_strict_codes("using a = null as any, b;\n");
    assert_eq!(
        one_each.iter().filter(|&&c| c == 7005).count(),
        1,
        "only the uninitialized `b` earns TS7005; the initialized `a` does not; got {one_each:?}"
    );
}

/// Container independence: the const-like implicit-any verdict comes from the
/// declaration form, not where it sits. A `using` in a plain block, a function
/// body, and an async function body all report TS7005 the same way. Renamed
/// binders throughout so no name drives the result.
#[test]
fn uninitialized_using_reports_ts7005_in_every_container() {
    for src in [
        "{ using blockScoped; }\n",
        "function make() { using localHandle; }\n",
        "async function run() { using asyncLocal; }\n",
    ] {
        let codes = check_source_strict_codes(src);
        assert!(
            codes.contains(&7005),
            "a `using` binding is const-like regardless of container; source {src:?} \
             got {codes:?}"
        );
    }
}

/// Non-strict / `noImplicitAny: false` control: TS7005 is a `noImplicitAny`
/// diagnostic, so it must not fire when the flag is off. (The must-initialize
/// TS1155 is a grammar rule unaffected by `noImplicitAny`, but it is
/// parser-owned and covered in the parser's grammar tests.)
#[test]
fn uninitialized_using_without_no_implicit_any_reports_no_ts7005() {
    let codes = crate::test_utils::check_source_non_strict_codes("using x;\n");
    assert!(
        !codes.contains(&7005),
        "TS7005 is gated on noImplicitAny, which is off here; got {codes:?}"
    );
}
