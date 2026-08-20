//! Regression tests for `TS2528` against a default-exported overload set.
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin):
//! a run of `export default function f(...)` declarations that share a name is
//! **one** default-exported symbol, not one per statement — tsc merges overload
//! signatures into a single `default` symbol before it ever asks whether the
//! module has multiple default exports. A module whose only default export is
//! such an overload set therefore reports nothing.
//!
//! tsz decides this in the checker's dedicated export-default pass
//! (`declarations/import/core/module_exports.rs`). That pass already collapsed
//! same-named function defaults for its `value_count`, but the conflict
//! predicate was
//!
//! ```text
//! value_count > 1 || (effective_default_indices.len() > 1 && !interface_can_merge)
//! ```
//!
//! and the second disjunct counts *statements*. An overload set with no other
//! default export in the file at all still tripped it on statement count alone,
//! so every signature got a false `TS2528`. The count is now taken after the
//! same merge `value_count` uses.
//!
//! The merge is deliberately not "same name wins": two same-named function
//! declarations that **both carry a body** are duplicate implementations, which
//! tsc keeps as separate conflicting declarations rather than merging. Only a
//! run with at most one body is an overload set.
//!
//! Corpus witness: `conformance/es6/modules/defaultExportWithOverloads01.ts`, a
//! pure false-positive row (extra `TS2528`, nothing missing) in
//! `scripts/conformance/conformance-detail.json`.
//!
//! Every row below was measured against the pin with
//! `--strict false --module commonjs --target es2015`. Binder names are varied
//! across rows: the rule is structural, so no row may depend on a particular
//! identifier spelling.

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

/// The corpus witness, verbatim: two bodyless signatures plus the
/// implementation, all named the same.
///
/// ```text
/// tsc: (no output)
/// ```
#[test]
fn a_default_exported_overload_set_is_one_default_export() {
    let source = "export default function f();\n\
                  export default function f(x: string);\n\
                  export default function f(...args: any[]) {\n}\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "three signatures of one default-exported function are one default export; \
         tsc reports nothing. Got: {:?}",
        codes(source)
    );
}

/// Same shape, different binder spelling — the rule must not key on `f`.
#[test]
fn the_overload_merge_does_not_depend_on_the_binder_spelling() {
    let source = "export default function widgetFactory(alpha: string);\n\
                  export default function widgetFactory(alpha: number);\n\
                  export default function widgetFactory(alpha: any) { return alpha; }\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "renamed binders must behave identically. Got: {:?}",
        codes(source)
    );
}

/// Longer runs collapse the same way — the merge is not "at most two".
#[test]
fn a_three_signature_overload_set_is_still_one_default_export() {
    let source = "export default function pick(a: string): string;\n\
                  export default function pick(a: number): number;\n\
                  export default function pick(a: boolean): boolean;\n\
                  export default function pick(a: any): any { return a; }\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "four statements, one symbol. Got: {:?}",
        codes(source)
    );
}

/// A bodyless overload set inside an ambient module declaration — no
/// implementation at all, so nothing can be mistaken for the merge anchor.
#[test]
fn an_ambient_module_default_overload_set_is_one_default_export() {
    let source = "declare module \"remote\" {\n\
                  \x20   export default function connect(host: string): void;\n\
                  \x20   export default function connect(port: number): void;\n\
                  }\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "ambient bodyless signatures merge like any other overload set. Got: {:?}",
        codes(source)
    );
}

/// An interface may merge with the function the overload set declares — the
/// pre-existing interface carve-out still applies once the run is collapsed.
#[test]
fn an_interface_merging_with_a_default_overload_set_is_clean() {
    let source = "export default interface Shape { sides: number }\n\
                  export default function Shape(spec: string);\n\
                  export default function Shape(spec: any) { return spec; }\n";

    assert_eq!(
        count_of(source, 2528),
        0,
        "interface + function default is a declaration merge. Got: {:?}",
        codes(source)
    );
}

/// Negative control, the one that matters: an overload set plus a genuinely
/// separate default export is still a conflict, and tsc reports `TS2528` on
/// **every** default site — three here, not one.
#[test]
fn an_overload_set_beside_another_default_export_still_reports_every_site() {
    let source = "export default function render(input: string);\n\
                  export default function render(input: any) { return input; }\n\
                  export default 1;\n";

    assert_eq!(
        count_of(source, 2528),
        3,
        "the overload set collapses to one entity but the module still has two, \
         and tsc marks all three statements. Got: {:?}",
        codes(source)
    );
}

/// Negative control: two same-named function declarations that both carry a
/// body are duplicate implementations, not an overload set. They must not be
/// collapsed away into a single clean default export.
#[test]
fn two_default_exported_implementations_of_one_name_are_not_an_overload_set() {
    let source = "export default function handler(x: string) { return x; }\n\
                  export default function handler(x: number) { return x; }\n";
    let observed = codes(source);

    assert!(
        observed.contains(&2393),
        "duplicate implementations still report TS2393. Got: {observed:?}"
    );
    assert!(
        observed.iter().any(|c| *c == 2528 || *c == 2323),
        "a duplicate-implementation pair is still a default-export conflict, not a \
         merged overload set. Got: {observed:?}"
    );
}

/// Negative control: a default-exported function beside a default-exported
/// class is the function/class merge family (`TS2323`/`TS2813`/`TS2814`), and
/// collapsing the function's own signatures must not disturb it.
#[test]
fn a_default_overload_set_beside_a_default_class_keeps_the_merge_family() {
    let source = "export default function Model(spec: string);\n\
                  export default function Model(spec: any) { return spec; }\n\
                  export default class Model {}\n";
    let observed = codes(source);

    assert!(
        observed.contains(&2323),
        "function + class default exports report the merge family. Got: {observed:?}"
    );
    assert_eq!(
        observed.iter().filter(|c| **c == 2528).count(),
        0,
        "the function/class family uses TS2323/TS2813/TS2814, never TS2528. \
         Got: {observed:?}"
    );
}
