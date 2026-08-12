//! #17197: a plain qualified type reference `NS.Missing`, where `NS` is a
//! **named import** of an `export * as NS` namespace re-export, must report
//! TS2694 ("namespace has no exported member") anchored at the missing member —
//! not TS2702 ("only refers to a type, but is being used as a namespace here")
//! anchored at `NS`.
//!
//! Structural rule: `export * as NS from "./m"` binds a *namespace-style alias*
//! (`SymbolFlags.Alias` with an `import * as` shape — `import_name() == "*"`
//! and an `import_module()` — and no `NAMESPACE_MODULE` flag) whose members are
//! `./m`'s exports. A named import that reaches such an alias carries namespace
//! meaning, so `NS.Member` resolves through it and a missing member is an
//! ordinary TS2694 miss — exactly as a direct `import * as NS from "./m"`
//! behaves. tsz used to classify the alias as type-only on the missing-member
//! path and emit TS2702, blaming `NS`.
//!
//! Scope note: the namespace *name* rendered in the message (tsz uses the local
//! anchor name, e.g. `NS`; `tsc` uses the backing module path, `"…/g/globals"`)
//! is a separate, pre-existing display gap shared with the direct
//! `import * as NS` baseline (see `direct_namespace_import_missing_member_stays_ts2694`),
//! analogous to the import-type display fix in #17177. This suite pins the
//! TS2702→TS2694 code/anchor fix and deliberately does not over-specify that
//! namespace-name body. Oracle: typescript@7.0.2
//! (`NS.Nope` -> `TS2694: Namespace '"…/g/globals"' has no exported member
//! 'Nope'.`, anchored at `Nope`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::diagnostics::Diagnostic;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
}

/// `(code, message, anchored_source_slice)` per diagnostic — the slice is the
/// exact span the diagnostic points at, so anchor regressions are caught.
fn triples(files: &[(&str, &str)], entry: &str, entry_src: &str) -> Vec<(u32, String, String)> {
    check(files, entry)
        .into_iter()
        .map(|d| {
            let start = d.start as usize;
            let end = start + d.length as usize;
            (
                d.code,
                d.message_text,
                entry_src.get(start..end).unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// Assert the entry produces exactly one TS2694 (no TS2702, no TS2503) whose
/// message reports the missing `member` and whose anchor is that member token.
fn assert_single_missing_member_2694(got: &[(u32, String, String)], member: &str) {
    assert_eq!(
        got.len(),
        1,
        "expected exactly one diagnostic; got {got:#?}"
    );
    let (code, message, anchor) = &got[0];
    assert_eq!(
        *code, 2694,
        "expected TS2694, not e.g. TS2702/TS2503; got {got:#?}"
    );
    assert!(
        message.contains(&format!("has no exported member '{member}'")),
        "message must blame the missing member '{member}'; got {message:?}"
    );
    assert_eq!(
        anchor, member,
        "TS2694 must anchor at the missing member token"
    );
}

const GLOBALS: (&str, &str) = (
    "g/globals.ts",
    "export interface Foo { a: number }\nexport type Bar = { b: string };\n",
);
const REEXPORT: (&str, &str) = ("g/index.ts", "export * as NS from './globals';\n");

/// The #17197 repro: a missing member through the named-import namespace anchor
/// is TS2694 anchored at the member, not TS2702 anchored at `NS`.
#[test]
fn missing_member_through_named_import_of_export_star_as_is_ts2694() {
    let entry = "import { NS } from './g/index';\ntype T = NS.Nope;\n";
    let got = triples(
        &[GLOBALS, REEXPORT, ("consumer.ts", entry)],
        "consumer.ts",
        entry,
    );
    assert_single_missing_member_2694(&got, "Nope");
}

/// A different missing-member name — the fix keys on the anchor's namespace
/// meaning, never on the member spelling.
#[test]
fn missing_member_name_is_not_hardcoded() {
    let entry = "import { NS } from './g/index';\ntype T = NS.Absent;\n";
    let got = triples(
        &[GLOBALS, REEXPORT, ("consumer.ts", entry)],
        "consumer.ts",
        entry,
    );
    assert_single_missing_member_2694(&got, "Absent");
}

/// Renamed binder: the alias local name changes; the namespace-meaning gate
/// follows the resolved chain, not the identifier text.
#[test]
fn renamed_named_import_missing_member_is_ts2694() {
    let entry = "import { NS as Space } from './g/index';\ntype T = Space.Gone;\n";
    let got = triples(
        &[GLOBALS, REEXPORT, ("consumer.ts", entry)],
        "consumer.ts",
        entry,
    );
    assert_single_missing_member_2694(&got, "Gone");
}

/// The direct `import * as NS` form already reports TS2694 and must stay that
/// way — the named-import fix keeps parity with the direct namespace import.
#[test]
fn direct_namespace_import_missing_member_stays_ts2694() {
    let entry = "import * as NS from './g/globals';\ntype T = NS.Nope;\n";
    let got = triples(&[GLOBALS, ("consumer.ts", entry)], "consumer.ts", entry);
    assert_single_missing_member_2694(&got, "Nope");
}

/// Over-broadening guard: a genuine *type* used as a namespace (a named import
/// of a type alias — NOT a namespace-style alias) must still report TS2702.
/// The fix only grants namespace meaning to `import * as`-shaped aliases, so
/// this legitimate TS2702 is untouched.
#[test]
fn named_import_of_type_alias_used_as_namespace_stays_ts2702() {
    let entry = "import { Bar } from './g/globals';\ntype X = Bar.k;\n";
    let got = triples(&[GLOBALS, ("consumer.ts", entry)], "consumer.ts", entry);
    assert_eq!(
        got,
        vec![(
            2702,
            "'Bar' only refers to a type, but is being used as a namespace here.".to_string(),
            "Bar".to_string(),
        )],
        "a genuine type-used-as-namespace must remain TS2702"
    );
}

/// A present member still resolves cleanly (no diagnostics), so the fix did not
/// turn the anchor into a blanket accept-or-reject.
#[test]
fn present_member_through_named_import_still_resolves() {
    let entry = "import { NS } from './g/index';\ntype T = NS.Bar;\nlet v: T = { b: 'ok' };\nv;\n";
    let codes: Vec<u32> = check(&[GLOBALS, REEXPORT, ("consumer.ts", entry)], "consumer.ts")
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.is_empty(),
        "present member must resolve clean; got {codes:?}"
    );
}
