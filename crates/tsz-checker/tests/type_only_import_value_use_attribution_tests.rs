//! Cross-file attribution of "type-only binding used as a value" diagnostics.
//!
//! When a binding that is type-only is used in a value position, tsc reports
//! one of two distinct diagnostics depending on *why* it is type-only:
//!
//! * **TS1361** — "'X' cannot be used as a value because it was imported using
//!   'import type'." The type-only-ness comes from a local
//!   `import type { X }` / `import { type X }` at the use site's file.
//! * **TS1362** — "'X' cannot be used as a value because it was exported using
//!   'export type'." The binding is a plain value import whose type-only-ness
//!   is introduced *upstream* by an `export type { X }` / `export { type X }`
//!   re-export.
//!
//! Regression: tsz used to resolve the arena for walking an import alias's
//! *own* declaration via the alias's import-*target* file (the file the alias
//! resolves to) instead of the alias's *declaring* file. The `import type`
//! marker therefore could not be found on the local clause, so every cross-file
//! `import type` use-as-value was misattributed to TS1362 ("export type"), even
//! when no `export type` existed anywhere in the program. The fix walks each
//! declaration in the arena that actually owns it
//! (`BinderState::arena_for_declaration_or`).
//!
//! The rule is name-agnostic, so each case varies the binder spellings to prove
//! the attribution is not keyed on any particular identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, diagnostic_code_messages, diagnostic_codes, load_lib_files,
};

fn check(files: &[(&str, &str)], entry: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_multi_file_with_libs(
        files,
        entry,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
}

fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    diagnostic_codes(&check(files, entry))
}

fn messages(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(check(files, entry))
}

/// `import type { X }` of a name exported as *both* a value and a type, then
/// used as a value, is attributed to the local `import type` -> TS1361.
#[test]
fn import_type_dual_namespace_source_attributes_ts1361() {
    let c = codes(
        &[
            (
                "shared.ts",
                "export const Widget = { id: 1 };\nexport type Widget = { id: number };\n",
            ),
            (
                "consumer.ts",
                "import type { Widget } from \"./shared\";\nexport type WidgetMap = Widget & { tag: \"w\" };\nconst value = Widget;\n",
            ),
        ],
        "consumer.ts",
    );
    assert!(
        c.contains(&1361) && !c.contains(&1362),
        "Expected TS1361 (imported using 'import type'), not TS1362: {c:?}"
    );
}

/// The source exports the name as a *value only*; the importing file's
/// `import type` is what makes the local binding type-only. There is no
/// `export type` anywhere, so the attribution must be TS1361, never TS1362.
#[test]
fn import_type_value_only_source_attributes_ts1361() {
    let m = messages(
        &[
            ("shared.ts", "export const Gadget = { id: 1 };\n"),
            (
                "consumer.ts",
                "import type { Gadget } from \"./shared\";\nconst value = Gadget;\n",
            ),
        ],
        "consumer.ts",
    );
    assert!(
        m.iter()
            .any(|(code, msg)| *code == 1361 && msg.contains("imported using 'import type'")),
        "Expected TS1361 mentioning 'import type' (no 'export type' exists): {m:?}"
    );
    assert!(
        !m.iter().any(|(code, _)| *code == 1362),
        "Must not report TS1362 when no 'export type' exists anywhere: {m:?}"
    );
}

/// Per-specifier `import { type X }` form is also a local `import type` marker.
#[test]
fn per_specifier_import_type_attributes_ts1361() {
    let c = codes(
        &[
            (
                "shared.ts",
                "export const Sprocket = { id: 1 };\nexport type Sprocket = { id: number };\n",
            ),
            (
                "consumer.ts",
                "import { type Sprocket } from \"./shared\";\nconst value = Sprocket;\n",
            ),
        ],
        "consumer.ts",
    );
    assert!(
        c.contains(&1361) && !c.contains(&1362),
        "Expected TS1361 for per-specifier `import {{ type X }}`: {c:?}"
    );
}

/// A plain value import whose type-only-ness is introduced upstream by an
/// `export type { X } from` re-export is attributed to TS1362.
#[test]
fn export_type_reexport_then_plain_import_attributes_ts1362() {
    let m = messages(
        &[
            (
                "origin.ts",
                "export const Cog = { id: 1 };\nexport type Cog = { id: number };\n",
            ),
            ("barrel.ts", "export type { Cog } from \"./origin\";\n"),
            (
                "consumer.ts",
                "import { Cog } from \"./barrel\";\nconst value = Cog;\n",
            ),
        ],
        "consumer.ts",
    );
    assert!(
        m.iter()
            .any(|(code, msg)| *code == 1362 && msg.contains("exported using 'export type'")),
        "Expected TS1362 (exported using 'export type') for upstream re-export: {m:?}"
    );
    assert!(
        !m.iter().any(|(code, _)| *code == 1361),
        "Must not report TS1361 when the type-only origin is an 'export type': {m:?}"
    );
}

/// Inline `export { type X } from` re-export is likewise attributed to TS1362.
#[test]
fn inline_export_type_reexport_attributes_ts1362() {
    let c = codes(
        &[
            (
                "origin.ts",
                "export const Lever = { id: 1 };\nexport type Lever = { id: number };\n",
            ),
            ("barrel.ts", "export { type Lever } from \"./origin\";\n"),
            (
                "consumer.ts",
                "import { Lever } from \"./barrel\";\nconst value = Lever;\n",
            ),
        ],
        "consumer.ts",
    );
    assert!(
        c.contains(&1362) && !c.contains(&1361),
        "Expected TS1362 for inline `export {{ type X }}` re-export: {c:?}"
    );
}

/// A plain value import is usable as a value even if the same file *also*
/// re-exports it type-only via `export type { X }` — the local value binding is
/// unaffected, so no diagnostic fires.
#[test]
fn plain_import_with_local_export_type_reexport_is_value_usable() {
    let c = codes(
        &[
            ("shared.ts", "export const Pulley = { id: 1 };\n"),
            (
                "consumer.ts",
                "import { Pulley } from \"./shared\";\nexport type { Pulley };\nconst value = Pulley;\n",
            ),
        ],
        "consumer.ts",
    );
    assert!(
        !c.contains(&1361) && !c.contains(&1362) && !c.contains(&2749),
        "A plain value import remains value-usable despite a local `export type` \
         re-export: {c:?}"
    );
}
