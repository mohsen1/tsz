//! Regression tests for the all-function default-export run (`TS2323` vs
//! `TS2528`), issue #16719.
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin):
//! a run of `export default function` declarations — regardless of the names
//! they declare — binds **one** exported `default` symbol, so tsc never
//! reports `TS2528` ("A module cannot have multiple default exports") for a
//! module whose default exports are all function declarations. Conflicts
//! inside the run surface through the duplicate-implementation family
//! instead: with two or more bodies, every declaration that carries a body
//! redeclares the exported binding (`TS2323`, "Cannot redeclare exported
//! variable 'default'.") and every declaration in the run — overload
//! signatures included — is a duplicate implementation site (`TS2393`). With
//! at most one body the run is an overload set and the export-default pass
//! reports nothing.
//!
//! tsz decides this in the checker's export-default pass
//! (`declarations/import/core/module_exports.rs`), in a dedicated emission
//! arm ahead of the mixed-kind arms. The mixed-kind arms (function + class,
//! function + expression, ...) are untouched: `export default function`
//! beside `export default 1` still reports `TS2528` at every site.
//!
//! Diagnostics are asserted by (offset, code) pairs, not code counts alone: a
//! count cannot tell "two sites" from "one site reported twice", and tsc
//! anchors these at the function *name* (or the statement for an anonymous
//! function), which the pre-fix code got wrong for `TS2393`.
//!
//! Every row below was measured against the pin with
//! `--strict false --module commonjs --target es2015`. Binder names vary
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

/// The (offset, code) pairs of every emitted diagnostic, sorted.
fn sites(source: &str) -> Vec<(u32, u32)> {
    let mut sites: Vec<(u32, u32)> = check_module(source)
        .into_iter()
        .map(|diagnostic| (diagnostic.start, diagnostic.code))
        .collect();
    sites.sort_unstable();
    sites
}

/// Byte offset of the `nth` occurrence (0-based) of `needle` in `source`.
fn offset_of(source: &str, needle: &str, nth: usize) -> u32 {
    let mut from = 0;
    for _ in 0..=nth {
        let found = source[from..]
            .find(needle)
            .unwrap_or_else(|| panic!("occurrence {nth} of {needle:?} not found"));
        from += found + 1;
    }
    (from - 1) as u32
}

/// The issue witness: two same-named implementations. tsc reports `TS2323` +
/// `TS2393` at each function name and no `TS2528`.
///
/// ```text
/// tsc: (1,25) TS2323, (1,25) TS2393, (2,25) TS2323, (2,25) TS2393
/// ```
#[test]
fn two_same_named_default_implementations_redeclare_the_exported_binding() {
    let source = "export default function handler(x: string) { return x; }\n\
                  export default function handler(x: number) { return x; }\n";
    let first = offset_of(source, "handler", 0);
    let second = offset_of(source, "handler", 1);

    assert_eq!(
        sites(source),
        vec![(first, 2323), (first, 2393), (second, 2323), (second, 2393)],
        "duplicate default implementations are TS2323 + TS2393 at each name, never TS2528"
    );
}

/// The names do not have to match: the exported binding is `default` either
/// way, so two different-named implementations conflict identically.
#[test]
fn two_different_named_default_implementations_conflict_the_same_way() {
    let source = "export default function alpha(a: string): string { return a; }\n\
                  export default function omega(a: number): number { return a; }\n";
    let first = offset_of(source, "alpha", 0);
    let second = offset_of(source, "omega", 0);

    assert_eq!(
        sites(source),
        vec![(first, 2323), (first, 2393), (second, 2323), (second, 2393)],
        "the conflict is on the exported `default` binding, not the declared names"
    );
}

/// Three implementations mark every site — the rule is per declaration, not
/// "everything after the first".
#[test]
fn a_three_implementation_run_marks_every_site() {
    let source = "export default function f(a: string) { return a; }\n\
                  export default function g(a: number) { return a; }\n\
                  export default function h(a: boolean) { return a; }\n";
    let expected: Vec<(u32, u32)> = {
        let mut expected = Vec::new();
        for name in ["function f", "function g", "function h"] {
            let offset = offset_of(source, name, 0) + "function ".len() as u32;
            expected.push((offset, 2323));
            expected.push((offset, 2393));
        }
        expected.sort_unstable();
        expected
    };

    assert_eq!(sites(source), expected);
}

/// An overload signature mixed into a dual-implementation run gets `TS2393`
/// but not `TS2323` — only declarations that carry a body redeclare the
/// exported binding.
///
/// ```text
/// tsc: (1,25) TS2393, (2,25) TS2323, (2,25) TS2393, (3,25) TS2323, (3,25) TS2393
/// ```
#[test]
fn a_signature_in_a_dual_implementation_run_is_a_duplicate_site_but_not_a_redeclaration() {
    let source = "export default function build(a: string): string;\n\
                  export default function build(a: any): any { return a; }\n\
                  export default function make(a: number): number { return a; }\n";
    let signature = offset_of(source, "build", 0);
    let first_impl = offset_of(source, "build", 1);
    let second_impl = offset_of(source, "make", 0);

    assert_eq!(
        sites(source),
        vec![
            (signature, 2393),
            (first_impl, 2323),
            (first_impl, 2393),
            (second_impl, 2323),
            (second_impl, 2393)
        ],
        "TS2323 is per body; TS2393 covers signatures in the run too"
    );
}

/// Anonymous default functions anchor at the statement instead of a name and
/// conflict the same way.
#[test]
fn two_anonymous_default_implementations_anchor_at_the_statements() {
    let source = "export default function (x: string) { return x; }\n\
                  export default function (x: number) { return x; }\n";
    let first = offset_of(source, "export", 0);
    let second = offset_of(source, "export", 1);

    assert_eq!(
        sites(source),
        vec![(first, 2323), (first, 2393), (second, 2323), (second, 2393)],
        "anonymous implementations conflict identically, anchored at the statement"
    );
}

/// A cross-name run with at most one body is still an overload set of the
/// merged `default` symbol as far as this pass is concerned: no `TS2323`, no
/// `TS2528`. (tsc's residual `TS2391`/`TS2394` for the unimplemented
/// signature belong to function-implementation checking, a different owner.)
#[test]
fn a_cross_name_run_with_one_body_reports_no_default_export_conflict() {
    let source = "export default function first(a: string): string;\n\
                  export default function second(a: number): number;\n\
                  export default function second(a: number): number { return a; }\n";
    let observed = sites(source);

    assert!(
        !observed
            .iter()
            .any(|(_, code)| *code == 2528 || *code == 2323),
        "one body means no redeclaration and no multiple-default-exports report. \
         Got: {observed:?}"
    );
}

/// Positive control: a function implementation beside a non-function default
/// export is not an all-function run — the mixed arms still own it and tsc
/// reports `TS2528` at both sites.
#[test]
fn an_implementation_beside_an_expression_default_still_reports_ts2528() {
    let source = "export default function router(x: string) { return x; }\n\
                  export default 1;\n";
    let function_site = offset_of(source, "router", 0);
    let expression_site = offset_of(source, "export", 1);

    assert_eq!(
        sites(source),
        vec![(function_site, 2528), (expression_site, 2528)],
        "function + expression defaults keep the TS2528 classification"
    );
}

/// Positive control from the #16714 suite, re-pinned by position: an overload
/// set beside a separate default export marks all three statements with
/// `TS2528` and nothing else.
#[test]
fn an_overload_set_beside_an_expression_default_marks_all_three_sites() {
    let source = "export default function render(input: string): void;\n\
                  export default function render(input: any): void {}\n\
                  export default 2;\n";
    let signature = offset_of(source, "render", 0);
    let implementation = offset_of(source, "render", 1);
    let expression = offset_of(source, "export", 2);

    assert_eq!(
        sites(source),
        vec![
            (signature, 2528),
            (implementation, 2528),
            (expression, 2528)
        ],
        "the collapsed overload set still counts as one of two default exports"
    );
}
