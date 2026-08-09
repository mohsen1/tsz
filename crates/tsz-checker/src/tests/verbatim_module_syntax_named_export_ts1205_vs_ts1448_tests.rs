//! `export { X } from "./y"` under `isolatedModules`, where `X` is a pure
//! type reached through zero or more intermediate *plain* re-export hops.
//!
//! tsc reports **TS1205** ("Re-exporting a type... requires using 'export
//! type'") at every hop of a plain re-export chain, regardless of depth.
//! tsz's `check_verbatim_module_syntax_named_exports` used
//! `is_export_type_only_across_binders` — a general "does this ultimately
//! resolve to a non-value type" query — to decide between TS1205 and
//! **TS1448** ("resolves to a type-only declaration and must be
//! re-exported using a type-only re-export"), and picked TS1448 for
//! isolatedModules whenever that query returned true. That query answers
//! the wrong question: TS1448 is tsc's code specifically for a chain that
//! crosses an *explicit* `export type`/`import type` syntax boundary
//! (mirroring `getTypeOnlyAliasDeclarationEx`), not merely "resolves to a
//! type". A depth-1 hop (`is_import_specifier_type_only`, checked first as
//! `is_inherent_type`) correctly reports TS1205 because it can see the
//! immediate target's own flags; a depth->=2 hop's immediate target is a
//! re-export ALIAS symbol that never copies the original declaration's
//! `INTERFACE`/`TYPE_ALIAS` flags, so `is_inherent_type` returns false and
//! control fell through to the general chain query, wrongly picking TS1448.
//! tsz-org/tsz#17101.
//!
//! `check_multi_file` only type-checks its `entry` file, so each hop of a
//! chain is exercised by re-running the same file set with that hop as the
//! entry point — this mirrors how a real multi-file compilation checks
//! every source file independently.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const RE_EXPORTING_A_TYPE: u32 = 1205;
const RESOLVES_TO_A_TYPE_ONLY_DECLARATION: u32 = 1448;

fn check(files: &[(&str, &str)], entry: &str, isolated_modules: bool) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            isolated_modules,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// Single hop: `impl.ts` -> `a.ts` (plain `export { Foo } from`). Already
/// correct before this fix (`is_inherent_type` sees the direct declaration).
#[test]
fn single_hop_plain_reexport_of_interface_reports_ts1205() {
    let files = [
        ("/impl.ts", "export interface Foo {}\n"),
        ("/a.ts", "export { Foo } from \"./impl\";\n"),
    ];

    let diagnostics = check(&files, "/a.ts", true);

    assert_eq!(
        codes(&diagnostics),
        vec![RE_EXPORTING_A_TYPE],
        "expected TS1205 on the direct re-export hop, got: {diagnostics:?}"
    );
}

/// Two hops: `impl.ts` -> `a.ts` -> `b.ts`, both plain re-exports, no
/// `export type` anywhere. tsc reports TS1205 at *both* hops; tsz reported
/// TS1205 then TS1448 before this fix. Each hop is checked as its own entry
/// against the same three-file set.
#[test]
fn two_hop_plain_reexport_chain_of_interface_reports_ts1205_at_both_hops() {
    let files = [
        ("/impl.ts", "export interface Foo {}\n"),
        ("/a.ts", "export { Foo } from \"./impl\";\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
    ];

    let hop1 = check(&files, "/a.ts", true);
    assert_eq!(
        codes(&hop1),
        vec![RE_EXPORTING_A_TYPE],
        "hop 1 (/a.ts): expected TS1205, got: {hop1:?}"
    );

    let hop2 = check(&files, "/b.ts", true);
    assert_eq!(
        codes(&hop2),
        vec![RE_EXPORTING_A_TYPE],
        "hop 2 (/b.ts): expected TS1205 (not TS1448), got: {hop2:?}"
    );
}

/// Three hops: `impl.ts` -> `a.ts` -> `b.ts` -> `reexport.ts`. Every hop
/// after the first previously flipped to TS1448.
#[test]
fn three_hop_plain_reexport_chain_of_interface_reports_ts1205_at_every_hop() {
    let files = [
        ("/impl.ts", "export interface Foo {}\n"),
        ("/a.ts", "export { Foo } from \"./impl\";\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
        ("/reexport.ts", "export { Foo } from \"./b\";\n"),
    ];

    for hop in ["/a.ts", "/b.ts", "/reexport.ts"] {
        let diagnostics = check(&files, hop, true);
        assert_eq!(
            codes(&diagnostics),
            vec![RE_EXPORTING_A_TYPE],
            "{hop}: expected TS1205 (not TS1448) at every hop of a plain \
             re-export chain, got: {diagnostics:?}"
        );
    }
}

/// Control: the same two-hop shape but the target is a runtime value
/// (`class Foo`), not a type. No diagnostic should fire at either hop —
/// this distinguishes "wrong code" from "should report at all".
#[test]
fn two_hop_plain_reexport_chain_of_class_reports_nothing() {
    let files = [
        ("/impl.ts", "export class Foo {}\n"),
        ("/a.ts", "export { Foo } from \"./impl\";\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
    ];

    let hop1 = check(&files, "/a.ts", true);
    assert_eq!(
        codes(&hop1),
        Vec::<u32>::new(),
        "hop 1 (/a.ts): expected no diagnostic for a value re-export, got: {hop1:?}"
    );

    let hop2 = check(&files, "/b.ts", true);
    assert_eq!(
        codes(&hop2),
        Vec::<u32>::new(),
        "hop 2 (/b.ts): expected no diagnostic for a value re-export, got: {hop2:?}"
    );
}

/// Known pre-existing gap (NOT fixed by this change, tracked separately):
/// when hop 1 uses an EXPLICIT `export type { Foo }` over a value (`class
/// Foo`), hop 2's plain `export { Foo }` should report TS1448 per tsc
/// (oracle-verified against `typescript@7.0.2`) because it crosses that
/// explicit type-only boundary. tsz reports TS1205 instead, both before and
/// after this fix: `is_import_specifier_type_only`'s one-hop lookup reads
/// `sym.is_type_only` directly off hop 1's re-export ALIAS symbol and
/// treats "explicitly marked type-only" as "inherently a type" (the
/// `is_inherent_type` branch, which always picks TS1205), the same
/// conflation the `module_specifier_text.is_none()` sibling branch already
/// guards against via `is_local_symbol_imported_as_type_only` /
/// `is_local_symbol_from_type_only_reexport_chain`. This asserts today's
/// (wrong) output so a regression is visible; fixing it is separate
/// follow-up work.
#[test]
fn plain_reexport_after_an_explicit_export_type_hop_reports_ts1205_not_ts1448_pending_fix() {
    let files = [
        ("/impl.ts", "export class Foo {}\n"),
        ("/a.ts", "export type { Foo } from \"./impl\";\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
    ];

    let hop2 = check(&files, "/b.ts", true);
    assert_eq!(
        codes(&hop2),
        vec![RE_EXPORTING_A_TYPE],
        "tsz currently emits TS1205 here (tsc wants TS1448, tracked \
         separately) — got: {hop2:?}"
    );
}

/// One hop further out from the same explicit boundary, this fix's
/// `crosses_explicit_type_only_boundary` gate DOES work correctly: `c.ts`'s
/// immediate target (`b.ts`'s plain re-export) has no `is_type_only` flag
/// of its own, so `is_inherent_type` doesn't short-circuit before reaching
/// `is_export_type_only_syntax_across_binders`, which correctly walks
/// `c -> b -> a` and finds `a`'s explicit `export type` boundary.
/// Oracle-verified (`typescript@7.0.2`): both `b.ts` and `c.ts` report
/// TS1448 — only the `b.ts` hop (immediately adjacent to the pre-existing
/// `is_inherent_type` gap above) is wrong today.
#[test]
fn plain_reexport_two_hops_after_an_explicit_export_type_hop_reports_ts1448() {
    let files = [
        ("/impl.ts", "export class Foo {}\n"),
        ("/a.ts", "export type { Foo } from \"./impl\";\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
        ("/c.ts", "export { Foo } from \"./b\";\n"),
    ];

    let hop3 = check(&files, "/c.ts", true);
    assert_eq!(
        codes(&hop3),
        vec![RESOLVES_TO_A_TYPE_ONLY_DECLARATION],
        "expected TS1448 two hops past the explicit `export type` boundary, \
         got: {hop3:?}"
    );
}
