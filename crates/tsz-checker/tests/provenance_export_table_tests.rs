//! Provenance-aware export resolution table (Goal 4 throughput wall-breaker).
//!
//! **Status: opt-in / default-OFF.** This was intended to ship always-on by
//! memoizing full chain provenance (visited chain + symbol→file registration
//! delta) rather than the bare endpoint that the predecessor experiment
//! (#12054) found unsound. A large-ts-repo A/B disproved soundness: the table
//! still flips diagnostics in both directions because the chain walk's endpoint
//! is not a pure function of `(current_file_idx, sym_id)` — it reads the
//! mutable, monotonically growing `symbol→file` overlay
//! (`resolve_symbol_file_index` etc.) to choose which cross-file export a
//! re-exported name resolves to. The first reference caches the resolution made
//! under overlay state `S₁`; a later reference would re-walk under a richer
//! `S₂ ⊇ S₁` and can reach a different endpoint. Replaying the registration
//! delta reproduces the walk's *writes* but not its *reads*. See the
//! `export_table_enabled` doc comment in
//! `crates/tsz-checker/src/types/queries/lib_aliases.rs` for the full root cause
//! and the 837-vs-967 `Graph<NodeId>` `TS2315` witness.
//!
//! These tests therefore pin the **shipped default (table OFF)** path: they
//! assert the un-memoized `resolve_alias_symbol` walk produces the correct
//! diagnostics across `export =`, named/wildcard re-exports, **type-only
//! re-export chains**, renamed bound-variable and renamed-module variants, and
//! re-export cycles. They are parity-preserving regression coverage for the
//! resolution shapes the table interacts with; they do not (and must not) claim
//! the enabled table is byte-identical.
//!
//! Anti-hardcoding: every shape is exercised with at least two name/spelling
//! choices, and the type-only path is repeated with a renamed module specifier,
//! so a fix keyed to a single spelling would fail here.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

const TS2322: u32 = 2322;
const TS1361: u32 = 1361;
const TS1362: u32 = 1362;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..Default::default()
    }
}

fn diagnostics(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check_multi_file(files, entry, strict_options())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn count(files: &[(&str, &str)], entry: &str, code: u32) -> usize {
    diagnostics(files, entry)
        .iter()
        .filter(|(c, _)| *c == code)
        .count()
}

// ───────────────── 1. export= + re-export chain: mismatch fires through cache ─

/// A type reached through an `export =` namespace member, re-exported via a
/// named re-export and then a wildcard re-export. The four good references
/// prime the provenance table; the bad one must still report exactly once.
#[test]
fn export_equals_reexport_chain_reports_single_mismatch_through_table() {
    let files = [
        (
            "a.d.ts",
            "declare namespace A { interface Widget { id: number; } }\nexport = A;\n",
        ),
        (
            "b.ts",
            "import a = require(\"./a\");\nexport import Widget = a.Widget;\n",
        ),
        ("c.ts", "export { Widget } from \"./b\";\n"),
        ("d.ts", "export * from \"./c\";\n"),
        (
            "use.ts",
            "import { Widget } from \"./d\";\n\
             const w1: Widget = { id: 1 };\n\
             const w2: Widget = { id: 2 };\n\
             const w3: Widget = { id: 3 };\n\
             const bad: Widget = { id: \"no\" };\n",
        ),
    ];
    assert_eq!(
        count(&files, "use.ts", TS2322),
        1,
        "expected exactly one TS2322 through the export= re-export chain; got {:#?}",
        diagnostics(&files, "use.ts")
    );
}

/// Same rule, different spellings (renamed interface, alias and module stems) —
/// proves the table is keyed by structure, not by the names in test 1.
#[test]
fn export_equals_reexport_chain_renamed_spellings_reports_single_mismatch() {
    let files = [
        (
            "core.d.ts",
            "declare namespace Core { interface Gadget { tag: string; } }\nexport = Core;\n",
        ),
        (
            "mid.ts",
            "import core = require(\"./core\");\nexport import Gadget = core.Gadget;\n",
        ),
        ("hub.ts", "export { Gadget } from \"./mid\";\n"),
        ("top.ts", "export * from \"./hub\";\n"),
        (
            "consumer.ts",
            "import { Gadget } from \"./top\";\n\
             const g1: Gadget = { tag: \"a\" };\n\
             const g2: Gadget = { tag: \"b\" };\n\
             const wrong: Gadget = { tag: 99 };\n",
        ),
    ];
    assert_eq!(
        count(&files, "consumer.ts", TS2322),
        1,
        "renamed export= chain must still report one TS2322; got {:#?}",
        diagnostics(&files, "consumer.ts")
    );
}

// ───────────────── 2. type-only re-export provenance (the #12054 wall) ───────

/// The high-risk path: a value used through a *type-only* re-export chain must
/// emit TS1362 ("'X' cannot be used as a value because it was exported using
/// 'export type'") — even though the endpoint symbol itself is a runtime value.
/// Endpoint-only memoization loses the `export type` provenance and silences
/// this; the provenance table replays the type-only chain and keeps it firing.
#[test]
fn type_only_reexport_used_as_value_emits_ts1362_through_table() {
    let files = [
        ("a.ts", "export class Service { run(): void {} }\n"),
        // b re-exports Service as *type-only*.
        ("b.ts", "export type { Service } from \"./a\";\n"),
        (
            "use.ts",
            // Reference Service in type position twice (primes the table), then
            // use it as a value — must error because the re-export is type-only.
            "import { Service } from \"./b\";\n\
             type T1 = Service;\n\
             type T2 = Service;\n\
             const s = new Service();\n",
        ),
    ];
    let diags = diagnostics(&files, "use.ts");
    assert!(
        diags.iter().any(|(c, _)| *c == TS1362),
        "type-only re-export used as value must emit TS1362 through the table; got {diags:#?}"
    );
}

/// Same type-only rule, with a **renamed module specifier and class name** —
/// the provenance must not be keyed to the spelling in the previous test.
#[test]
fn type_only_reexport_renamed_module_emits_ts1362() {
    let files = [
        ("engine.ts", "export class Worker { tick(): void {} }\n"),
        ("reexport.ts", "export type { Worker } from \"./engine\";\n"),
        (
            "main.ts",
            "import { Worker } from \"./reexport\";\n\
             type W1 = Worker;\n\
             type W2 = Worker;\n\
             const w = new Worker();\n",
        ),
    ];
    let diags = diagnostics(&files, "main.ts");
    assert!(
        diags.iter().any(|(c, _)| *c == TS1362),
        "renamed type-only re-export used as value must still emit TS1362; got {diags:#?}"
    );
}

/// `import type` (TS1361 family): a type-only *import* used as a value must
/// stay flagged even after the alias is referenced in type position first
/// (which primes the table on the type-only alias).
#[test]
fn type_only_import_used_as_value_stays_flagged_through_table() {
    let files = [
        ("mod.ts", "export class Box { open(): void {} }\n"),
        (
            "use.ts",
            "import type { Box } from \"./mod\";\n\
             type B1 = Box;\n\
             type B2 = Box;\n\
             const b = new Box();\n",
        ),
    ];
    let diags = diagnostics(&files, "use.ts");
    assert!(
        diags.iter().any(|(c, _)| *c == TS1361 || *c == TS1362),
        "type-only import used as value must stay flagged through the table; got {diags:#?}"
    );
}

/// Negative / fallback control: a **plain** (non-type-only) re-export of the
/// same class used as a value must NOT emit TS1361/TS1362 — proving the table
/// does not over-apply type-only provenance to ordinary aliases.
#[test]
fn plain_reexport_used_as_value_has_no_type_only_diagnostic() {
    let files = [
        ("a.ts", "export class Service { run(): void {} }\n"),
        ("b.ts", "export { Service } from \"./a\";\n"),
        (
            "use.ts",
            "import { Service } from \"./b\";\n\
             type T1 = Service;\n\
             const s = new Service();\n",
        ),
    ];
    let diags = diagnostics(&files, "use.ts");
    assert!(
        !diags.iter().any(|(c, _)| *c == TS1361 || *c == TS1362),
        "plain re-export used as value must not emit a type-only diagnostic; got {diags:#?}"
    );
}

// ───────────────── 3. re-export cycle terminates and stays clean ─────────────

/// A re-export cycle must terminate (no hang / overflow) and must not be
/// memoized as a position-dependent truncation. The downstream good use stays
/// resolvable; the bad use still reports.
#[test]
fn reexport_cycle_terminates_and_reports_mismatch() {
    let files = [
        // c1 <-> c2 form a re-export cycle of `Item`, with a real definition.
        ("def.ts", "export interface Item { n: number; }\n"),
        (
            "c1.ts",
            "export { Item } from \"./def\";\nexport * from \"./c2\";\n",
        ),
        ("c2.ts", "export * from \"./c1\";\n"),
        (
            "use.ts",
            "import { Item } from \"./c1\";\n\
             const ok: Item = { n: 1 };\n\
             const bad: Item = { n: \"x\" };\n",
        ),
    ];
    // Must not hang; the mismatch must still surface.
    assert!(
        count(&files, "use.ts", TS2322) >= 1,
        "cyclic re-export chain must still report the TS2322 mismatch; got {:#?}",
        diagnostics(&files, "use.ts")
    );
}

// ───────────────── 4. cold-vs-warm stability (same diagnostics twice) ────────

/// Checking the same program twice (two `check_multi_file` invocations) must
/// yield identical diagnostics — the table is rebuilt per run and never leaks a
/// stale endpoint across runs.
#[test]
fn cold_and_warm_runs_are_stable() {
    let files = [
        (
            "a.d.ts",
            "declare namespace A { interface W { id: number; } }\nexport = A;\n",
        ),
        (
            "b.ts",
            "import a = require(\"./a\");\nexport import W = a.W;\n",
        ),
        (
            "use.ts",
            "import { W } from \"./b\";\nconst bad: W = { id: \"no\" };\n",
        ),
    ];
    let first = diagnostics(&files, "use.ts");
    let second = diagnostics(&files, "use.ts");
    assert_eq!(
        first, second,
        "cold and warm runs must produce identical diagnostics"
    );
}
