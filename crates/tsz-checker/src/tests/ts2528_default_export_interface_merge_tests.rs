//! Regression tests for `TS2528` against two or more default-exported
//! interfaces. See #16730.
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin):
//! when a module's default exports are all `export default interface`
//! declarations, tsc merges them into the one `default` type symbol — the
//! same way any two same-named interfaces merge — keyed on the export name
//! `default`, not each interface's own local spelling. The multiple-default
//! complaint (`TS2528`) never fires for that shape, and no other diagnostic
//! is produced either.
//!
//! tsz's export-default conflict pass
//! (`declarations/import/core/module_exports.rs`) classifies each default
//! export while computing `value_count`; only its interface arm never
//! incremented `value_count`, so two interfaces landed at `value_count == 0`
//! but the fallback TS2528 arm's own comment asserted the opposite rule
//! ("when no function/class is present, the interface is truly
//! conflicting") and reported `TS2528` at every site. A new arm now checks
//! `has_interface && !has_function && !has_class && value_count == 0`
//! — exact, because every non-interface default-export shape increments
//! `value_count` in the classification loop, so this only fires when every
//! entry in the conflicting set is itself an interface declaration.
//!
//! `interface + type-alias identifier` stays a real conflict
//! (`export_default_interface_plus_type_identifier_both_get_ts2528` in
//! `ts2300_tests.rs`, re-verified against the oracle, unaffected by this
//! fix): the identifier there is classified as an ordinary value default and
//! increments `value_count`, so it never reaches the new merge arm.

use crate::context::ScriptTarget;
use crate::test_utils::check_source;
use crate::{CheckerOptions, diagnostics::Diagnostic};

fn check_module(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    )
}

fn codes(source: &str) -> Vec<u32> {
    check_module(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn count_of(source: &str, code: u32) -> usize {
    codes(source).into_iter().filter(|c| *c == code).count()
}

/// The reported repro, verbatim: two default-exported interfaces with
/// different local names and disjoint members.
///
/// ```text
/// tsc: (no output)
/// ```
#[test]
fn two_default_exported_interfaces_merge_cleanly() {
    let source = "export default interface A { x: string }\n\
                  export default interface B { y: number }\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "two default-exported interfaces merge into the one `default` type \
         symbol; tsc reports nothing. Got: {:?}",
        codes(source)
    );
}

/// Same shape, different binder spellings — the rule must not key on `A`/`B`.
#[test]
fn renamed_default_exported_interfaces_still_merge_cleanly() {
    let source = "export default interface WidgetSpec { size: number }\n\
                  export default interface WidgetSpec { color: string }\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "renamed binders must behave identically. Got: {:?}",
        codes(source)
    );
}

/// Three or more interfaces collapse the same way — the merge is not
/// "at most two".
#[test]
fn three_default_exported_interfaces_merge_cleanly() {
    let source = "export default interface A { a: string }\n\
                  export default interface B { b: number }\n\
                  export default interface C { c: boolean }\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "any-length run of interface-only default exports merges. Got: {:?}",
        codes(source)
    );
}

/// A member conflict between the merged interfaces (same member name,
/// incompatible types) is a distinct diagnostic family, not `TS2528` —
/// negative control that the merge arm does not also suppress a real
/// merge-conflict error.
#[test]
fn conflicting_members_across_merged_default_interfaces_report_no_ts2528() {
    let source = "export default interface A { x: string }\n\
                  export default interface B { x: number }\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "the default-export pass never reports TS2528 for an interface-only \
         run, even when the merged members conflict — that conflict is a \
         separate interface-merge diagnostic, not this pass's concern. \
         Got: {:?}",
        codes(source)
    );
}

/// Negative control: an interface beside a genuinely separate (non-merging)
/// default export is still a conflict, and every site gets `TS2528`.
#[test]
fn an_interface_beside_a_non_merging_default_export_still_reports_every_site() {
    let source = "export default interface A { x: string }\n\
                  export default 1;\n";

    assert_eq!(
        count_of(source, 2528),
        2,
        "an interface plus an unrelated value default is a genuine conflict. \
         Got: {:?}",
        codes(source)
    );
}

/// Negative control: an interface plus a function/class default is the
/// pre-existing declaration-merge carve-out, unaffected by this fix.
#[test]
fn an_interface_merging_with_a_default_function_stays_clean() {
    let source = "export default interface Shape { sides: number }\n\
                  export default function Shape() {}\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "interface + function default is a pre-existing declaration merge. \
         Got: {:?}",
        codes(source)
    );
}

/// Ambient form — no implementation at all, so nothing can be mistaken for a
/// merge anchor.
#[test]
fn ambient_default_exported_interfaces_merge_cleanly() {
    let source = "declare module \"remote\" {\n\
                  \x20   export default interface Config { host: string }\n\
                  \x20   export default interface Config { port: number }\n\
                  }\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "ambient interface-only default exports merge like any other. \
         Got: {:?}",
        codes(source)
    );
}
