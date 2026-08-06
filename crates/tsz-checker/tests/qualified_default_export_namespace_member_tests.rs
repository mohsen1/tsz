//! Member lookup through a default-imported namespace/enum (#16503).
//!
//! `#16486` fixed the *diagnostic gate* — a `D.foo` qualifier reached through a
//! default import of `export default <namespace>` no longer emits a spurious
//! `TS2702`. But the fix only patched the gate: it left the member-lookup path
//! anchored on the synthetic `default` ALIAS symbol rather than the real
//! namespace/enum it references. So a *missing* member (`D.Missing`) produced no
//! diagnostic at all (a false negative) instead of tsc's `TS2694`, and the
//! *present*-member row only "passed" because the unresolved qualified name
//! collapsed to `TypeId::ERROR` before any member check ran.
//!
//! Every row is measured against `typescript@7.0.2`,
//! `--noEmit --strict --target es2015`.

use crate::test_utils::check_multi_file_with_global_index;
use tsz_common::CheckerOptions;

fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_multi_file_with_global_index(files, entry, options)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn messages(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_multi_file_with_global_index(files, entry, options)
        .into_iter()
        .map(|diagnostic| diagnostic.message_text)
        .collect()
}

/// A genuinely missing member of a default-exported namespace: tsc reports
/// `TS2694` ("Namespace 'D' has no exported member 'Missing'.").
#[test]
fn default_exported_namespace_missing_member_reports_ts2694() {
    let got = codes(
        &[
            (
                "dep.ts",
                "namespace m { export interface foo { a: number } }\nexport default m;\n",
            ),
            ("main.ts", "import D from './dep';\nvar bad: D.Missing;\n"),
        ],
        "main.ts",
    );
    assert!(
        got.contains(&2694),
        "a missing member of an export-default namespace must be TS2694, got: {got:?}"
    );
    assert!(
        !got.contains(&2702),
        "must not regress to TS2702, got: {got:?}"
    );
}

/// The TS2694 names the namespace by the qualifier as written (`D`), matching
/// tsc, not by the resolved declaration (`m`).
#[test]
fn default_exported_namespace_missing_member_names_the_qualifier() {
    let rendered = messages(
        &[
            (
                "dep.ts",
                "namespace m { export interface foo { a: number } }\nexport default m;\n",
            ),
            ("main.ts", "import D from './dep';\nvar bad: D.Missing;\n"),
        ],
        "main.ts",
    );
    assert!(
        rendered
            .iter()
            .any(|m| m == "Namespace 'D' has no exported member 'Missing'."),
        "expected tsc's exact TS2694 text naming 'D', got: {rendered:?}"
    );
}

/// A present member of a default-exported namespace resolves cleanly.
#[test]
fn default_exported_namespace_present_member_resolves() {
    let got = codes(
        &[
            (
                "dep.ts",
                "namespace m { export interface foo { a: number } }\nexport default m;\n",
            ),
            ("main.ts", "import D from './dep';\nvar q: D.foo;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        got,
        Vec::<u32>::new(),
        "a present member of an export-default namespace resolves clean, got: {got:?}"
    );
}

/// Default-exported enum, missing member.
#[test]
fn default_exported_enum_missing_member_reports_ts2694() {
    let got = codes(
        &[
            ("e.ts", "enum Color { Red, Green }\nexport default Color;\n"),
            ("main.ts", "import C from './e';\nvar bad: C.Nope;\n"),
        ],
        "main.ts",
    );
    assert!(
        got.contains(&2694),
        "a missing member of an export-default enum must be TS2694, got: {got:?}"
    );
}

/// Default-exported enum, present member (`C.Red` as a type). Resolves clean.
#[test]
fn default_exported_enum_present_member_resolves() {
    let got = codes(
        &[
            ("e.ts", "enum Color { Red, Green }\nexport default Color;\n"),
            ("main.ts", "import C from './e';\nvar ok: C.Red;\n"),
        ],
        "main.ts",
    );
    assert_eq!(
        got,
        Vec::<u32>::new(),
        "a present member of an export-default enum resolves clean, got: {got:?}"
    );
}

/// Renamed local binding still resolves through the default export slot's
/// target, not the local spelling.
#[test]
fn renamed_default_import_missing_member_reports_ts2694() {
    let got = codes(
        &[
            (
                "dep2.ts",
                "namespace m { export interface foo { a: number } }\nexport default m;\n",
            ),
            (
                "main2.ts",
                "import Renamed from './dep2';\nvar bad: Renamed.Missing;\n",
            ),
        ],
        "main2.ts",
    );
    assert!(
        got.contains(&2694),
        "renamed default import of a namespace still reports TS2694, got: {got:?}"
    );
}
