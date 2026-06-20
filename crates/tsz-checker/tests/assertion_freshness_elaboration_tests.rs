//! Freshness through a type assertion (`as T` / `<T>expr`) — refs #13942 / #8432.
//!
//! A plain `expr as T` / `<T>expr` assertion yields the asserted (non-fresh)
//! type `T`. `tsc`'s `elaborateError` descends through *parenthesized* operands
//! but NOT through `as` / `<T>` type assertions, so fresh-literal elaboration
//! (per-property `TS2322` and excess-property `TS2353`) never runs on an
//! assertion operand. A non-fresh asserted source instead surfaces the
//! argument/assignment-level report: the weak-type `TS2559`, or the
//! argument-level `TS2345` with the relation's own structural elaboration
//! chain. `satisfies` and `as const` preserve freshness, so they still
//! elaborate per-element/property.
//!
//! Binder, parameter, and property names are varied across cases so the rule is
//! structural over the assertion shape, not keyed on any spelling
//! (anti-hardcoding).

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_codes};

#[test]
fn weak_target_assertion_arg_reports_ts2559_not_ts2353() {
    // `{ gamma: 5 } as { gamma: number }` passed to a weak parameter
    // (all-optional) has no common properties: `tsc` reports the weak-type
    // TS2559, never the fresh-literal excess TS2353. (Regression: tsz descended
    // through the `as` into the inner fresh literal and emitted TS2353.)
    let diags = check_source_diagnostics(
        r#"
interface Settings { alpha?: number; beta?: string }
declare function configure(opts: Settings): void;
configure({ gamma: 5 } as { gamma: number });
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2559),
        "Expected weak-type TS2559, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    assert!(
        !diags.iter().any(|d| d.code == 2353),
        "Did not expect fresh-literal excess TS2353 for an `as`-asserted source, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn weak_target_assertion_spellings_all_report_ts2559() {
    // Every plain-assertion spelling strips freshness: angle-bracket, a double
    // `as ... as ...`, and a parenthesized `as`. None should excess-check.
    for (label, source) in [
        (
            "angle",
            r#"
interface Prefs { one?: number; two?: string }
declare function apply(p: Prefs): void;
apply(<{ three: number }>{ three: 9 });
"#,
        ),
        (
            "double_as",
            r#"
interface Opts { aa?: number; bb?: string }
declare function run(o: Opts): void;
run({ zz: 1 } as any as { zz: number });
"#,
        ),
        (
            "paren_as",
            r#"
interface Conf { p?: number; q?: string }
declare function init(c: Conf): void;
init(({ r: 2 } as { r: number }));
"#,
        ),
    ] {
        let diags = check_source_diagnostics(source);
        assert!(
            diags.iter().any(|d| d.code == 2559) && !diags.iter().any(|d| d.code == 2353),
            "[{label}] expected TS2559 and no TS2353, got: {:?}",
            diagnostic_codes(&diags)
        );
    }
}

#[test]
fn fresh_and_freshness_preserving_forms_still_excess_check() {
    // A bare fresh literal, `satisfies`, and `as const` all keep freshness, so
    // the excess check (TS2353) still fires — proving the assertion fix is
    // narrowly scoped to freshness-stripping `as` / `<T>` assertions. These
    // three cases share one binder spelling and vary only the wrapper form, so
    // the preamble is factored out (name-variation across the sibling tests
    // already covers the anti-hardcoding requirement).
    let preamble = "interface Settings { alpha?: number; beta?: string }\n\
                    declare function configure(opts: Settings): void;\n";
    for (label, call) in [
        ("fresh", "configure({ gamma: 5 });"),
        (
            "satisfies",
            "configure({ gamma: 5 } satisfies { gamma: number });",
        ),
        ("as_const", "configure({ gamma: 5 } as const);"),
    ] {
        let diags = check_source_diagnostics(&format!("{preamble}{call}"));
        assert!(
            diags.iter().any(|d| d.code == 2353),
            "[{label}] expected fresh-literal excess TS2353, got: {:?}",
            diagnostic_codes(&diags)
        );
    }
}

#[test]
fn property_mismatch_assertion_arg_reports_argument_level_not_per_property() {
    // `{ key: "x" } as { key: string }` to a parameter whose `key` is numeric:
    // the asserted source is non-fresh, so `tsc` reports the argument-level
    // TS2345 (with a nested "Types of property" chain), not a per-property
    // TS2322.
    let asserted = check_source_diagnostics(
        r#"
interface Holder { key?: number; note?: string }
declare function accept(h: Holder): void;
accept({ key: "x" } as { key: string });
"#,
    );
    assert!(
        asserted.iter().any(|d| d.code == 2345),
        "Expected argument-level TS2345 for an `as`-asserted property mismatch, got: {:?}",
        asserted
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // A fresh literal in the same position still elaborates to per-property
    // TS2322; the assertion form must not.
    let fresh = check_source_diagnostics(
        r#"
interface Holder { key?: number; note?: string }
declare function accept(h: Holder): void;
accept({ key: "x" });
"#,
    );
    assert!(
        fresh.iter().any(|d| d.code == 2322),
        "Expected fresh literal to still elaborate per-property TS2322, got: {:?}",
        diagnostic_codes(&fresh)
    );
}
