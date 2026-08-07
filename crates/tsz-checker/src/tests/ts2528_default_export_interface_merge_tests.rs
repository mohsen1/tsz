//! Regression tests for issue #16730: a run of `export default interface`
//! DECLARATIONS merges into the one `default` interface symbol and reports
//! nothing — tsz emitted a false `TS2528` ("A module cannot have multiple
//! default exports") at every site.
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin, and
//! re-checked against the local `tsc`): tsc keys the default export on the name
//! `default`, so any number of `export default interface` declarations merge
//! like any named interface merge, and optionally absorb a single value
//! (function or class) declaration alongside them. A non-interface,
//! non-value default — a type-only identifier (`export default SomeType`) or a
//! value expression (`export default 1`) — does not merge and stays a genuine
//! `TS2528` conflict; two value declarations conflict as a redeclaration
//! (`TS2323`).
//!
//! Owner: the checker's export-default pass
//! (`declarations/import/core/module_exports.rs`). Its `interface_can_merge`
//! predicate required a function/class sibling (`value_count == 1`) and so
//! never recognized the all-interface run (`value_count == 0`).
//!
//! Binder names and interface counts are varied across rows: the rule is
//! structural (interface DECLARATION vs identifier/expression default, and the
//! non-interface value count), never a particular identifier spelling.

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

/// The exact repro: two `export default interface` declarations merge into one
/// `default` symbol. tsc reports nothing.
#[test]
fn two_default_interface_declarations_merge_with_no_error() {
    let source = "export default interface A { x: string; }\n\
                  export default interface B { y: number; }\n";
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "two default-exported interface declarations merge; tsc reports nothing. Got: {:?}",
        codes(source)
    );
}

/// The merge is not "at most two" — a longer run collapses the same way. Binder
/// spellings are varied so no row can key on `A`/`B`.
#[test]
fn three_default_interface_declarations_merge_with_no_error() {
    let source = "export default interface Alpha { a: 1; }\n\
                  export default interface Bravo { b: 2; }\n\
                  export default interface Charlie { c: 3; }\n";
    assert_eq!(
        count_of(source, 2528),
        0,
        "three default interface declarations are one merged default symbol. Got: {:?}",
        codes(source)
    );
}

/// Same interface name repeated as a default declaration also merges (ordinary
/// same-name interface merge, keyed on `default`).
#[test]
fn repeated_same_name_default_interface_declarations_merge() {
    let source = "export default interface Shape { x: string; }\n\
                  export default interface Shape { y: number; }\n";
    assert_eq!(
        count_of(source, 2528),
        0,
        "same-named default interface declarations merge. Got: {:?}",
        codes(source)
    );
}

/// Regression guard: the pre-existing interface + single value (function/class)
/// merge must stay clean.
#[test]
fn interface_plus_single_value_default_still_merges() {
    let with_function = "export default interface A { x: string; }\n\
                         export default function make() {}\n";
    assert_eq!(
        count_of(with_function, 2528),
        0,
        "interface + function default merges. Got: {:?}",
        codes(with_function)
    );

    let with_class = "export default interface A { x: string; }\n\
                      export default class Impl {}\n";
    assert_eq!(
        count_of(with_class, 2528),
        0,
        "interface + class default merges. Got: {:?}",
        codes(with_class)
    );
}

/// Negative control: an interface default paired with a type-only identifier
/// default does NOT merge — tsc emits `TS2528` on both sites, and the fix must
/// not silence it.
#[test]
fn interface_plus_type_only_identifier_default_still_conflicts() {
    let source = "export default interface A {}\n\
                  interface B {}\n\
                  export default B;\n";
    assert_eq!(
        count_of(source, 2528),
        2,
        "interface + `export default <type-identifier>` is a genuine TS2528 on both. Got: {:?}",
        codes(source)
    );
}

/// Negative control: an interface default paired with a value expression
/// default (`export default 1`) does not merge — genuine `TS2528` on both.
#[test]
fn interface_plus_value_expression_default_still_conflicts() {
    let source = "export default interface A { x: string; }\n\
                  export default 1;\n";
    assert_eq!(
        count_of(source, 2528),
        2,
        "interface + `export default <expr>` is a genuine TS2528 on both. Got: {:?}",
        codes(source)
    );
}

/// Negative control: an interface alongside two value (function implementation)
/// defaults is a redeclaration conflict — tsc reports `TS2323`, never `TS2528`.
/// The merge fix must not turn this into a false clean.
#[test]
fn interface_plus_two_function_implementations_reports_ts2323_not_ts2528() {
    let source = "export default interface A { x: string; }\n\
                  export default function f() {}\n\
                  export default function g() {}\n";
    assert_eq!(
        count_of(source, 2528),
        0,
        "two conflicting value defaults are TS2323, not TS2528. Got: {:?}",
        codes(source)
    );
    assert!(
        count_of(source, 2323) > 0,
        "the conflicting value defaults must still surface TS2323. Got: {:?}",
        codes(source)
    );
}
