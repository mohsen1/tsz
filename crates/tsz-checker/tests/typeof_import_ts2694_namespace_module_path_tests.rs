//! The TS2694 namespace name for a TS-syntax `typeof import("mod").Member`
//! value query must render the module symbol's name — the *resolved file
//! path* with the extension removed — not the bare specifier stem.
//!
//! Structural rule: `tsc` binds a module's synthetic namespace symbol as
//! `"${removeFileExtension(fileName)}"` (`bindSourceFileAsExternalModule`),
//! so the namespace text in `Namespace 'X' has no exported member 'Y'` is the
//! resolved path. #17183 applied this to the type-position
//! `import("mod").Member` resolver and the JSDoc `@type`/`@typedef` path,
//! #17187 to the JSDoc `@param {typeof import(...).member}` walk; this file
//! pins the remaining sibling, the TS-syntax `typeof import("mod").Member`
//! walk (`resolve_typeof_import_query` in
//! `state/type_analysis/core_type_query.rs`). Oracle: `typescript@7.0.2`
//! (`tsc` renders `Namespace '"/abs/path/pkg/index"'` for
//! `typeof import('./pkg').Missing`; the conformance harness normalizes
//! diagnostic paths against the project root, so a same-directory import
//! still reads as the bare stem).
//!
//! The display for export-assignment modules (`export =` /
//! `module.exports =`) is a separate, pre-existing naming divergence — tsc
//! names the export= *target symbol* there (e.g. `Namespace 'shape'`), tsz
//! renders `"stem".export=` — deliberately left byte-for-byte untouched,
//! same as #17187's scope cut. Only modules *without* an export assignment
//! take the resolved-path rule.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn ts2694_messages(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .filter(|d| d.code == 2694)
    .map(|d| d.message_text)
    .collect()
}

fn ts2694_messages_cjs(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code == 2694)
    .map(|d| d.message_text)
    .collect()
}

/// Same-directory import: the resolved path equals the specifier stem, so the
/// namespace text is unchanged. Guards the common case against regression.
#[test]
fn typeof_same_directory_renders_specifier_stem() {
    let msgs = ts2694_messages(
        &[
            ("mod.ts", "export const local: number = 3;\n"),
            (
                "main.ts",
                "type M = typeof import('./mod').Absent;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"mod\"' has no exported member 'Absent'.".to_string()],
        "same-directory typeof import should render the bare module stem"
    );
}

/// Index resolution: `./pkg` resolves to `pkg/index.ts`. `tsc` renders the
/// resolved path `"pkg/index"`, which the written specifier `./pkg` cannot
/// express. This is the core divergence the fix repairs.
#[test]
fn typeof_index_resolution_renders_resolved_path() {
    let msgs = ts2694_messages(
        &[
            ("pkg/index.ts", "export const width: number = 1;\n"),
            (
                "main.ts",
                "type M = typeof import('./pkg').Missing;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"pkg/index\"' has no exported member 'Missing'.".to_string()],
        "index-resolved typeof import should render the resolved path, not the specifier stem"
    );
}

/// Subdirectory import: `./sub/mod` resolves to `sub/mod.ts`; resolved path
/// and specifier stem agree once normalized, but exercise the multi-segment
/// specifier shape.
#[test]
fn typeof_subdirectory_renders_resolved_path() {
    let msgs = ts2694_messages(
        &[
            ("sub/mod.ts", "export const depth: number = 2;\n"),
            (
                "main.ts",
                "type M = typeof import('./sub/mod').Gone;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"sub/mod\"' has no exported member 'Gone'.".to_string()],
    );
}

/// A member that exists but is type-only (an interface) is still TS2694 for a
/// value query, and the namespace text follows the same resolved-path rule.
#[test]
fn typeof_type_only_member_renders_resolved_path() {
    let msgs = ts2694_messages(
        &[
            (
                "pkg/index.ts",
                "export const width: number = 1;\nexport interface Iface { a: number }\n",
            ),
            (
                "main.ts",
                "type M = typeof import('./pkg').Iface;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"pkg/index\"' has no exported member 'Iface'.".to_string()],
        "a type-only member is not a value, and the miss renders the resolved path"
    );
}

/// Binder-name independence: renaming the files and the directory does not
/// change the rule — the namespace text follows whatever path the specifier
/// resolves to, never a hard-coded name.
#[test]
fn typeof_resolved_path_is_binder_name_independent() {
    let msgs = ts2694_messages(
        &[
            ("widgets/entry.ts", "export const gadget: number = 1;\n"),
            (
                "consumer.ts",
                "type Q = typeof import('./widgets/entry').Absent;\ndeclare const q: Q;\n",
            ),
        ],
        "consumer.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"widgets/entry\"' has no exported member 'Absent'.".to_string()],
    );
}

/// Positive control: a present value member resolves with no TS2694, index
/// resolution and all.
#[test]
fn typeof_present_member_resolves_without_ts2694() {
    let msgs = ts2694_messages(
        &[
            ("pkg/index.ts", "export const width: number = 1;\n"),
            (
                "main.ts",
                "type M = typeof import('./pkg').width;\ndeclare const m: M;\nconst n: number = m;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(msgs, Vec::<String>::new());
}

/// Negative boundary: an export-assignment module keeps the pre-existing
/// `.export=` display scheme untouched — specifier stem and suffix exactly as
/// before this fix. `tsc` names the export= *target symbol* there (`shape`),
/// a separately-tracked divergence this fix must not half-migrate: only
/// modules without an export assignment take the resolved-path rule.
#[test]
fn typeof_export_equals_module_keeps_preexisting_display_scheme() {
    let msgs = ts2694_messages_cjs(
        &[
            (
                "cepkg/index.ts",
                "const shape = { edge: 1 };\nexport = shape;\n",
            ),
            (
                "main.ts",
                "type M = typeof import('./cepkg').edge2;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"cepkg\".export=' has no exported member 'edge2'.".to_string()],
    );
}

/// Multi-segment miss at the FIRST segment: `tsc` blames the segment that
/// failed to resolve, at its own position — never a later one. Before this
/// fix the walk skipped ahead and reported
/// `Namespace '"mod".a.export=' has no exported member 'b'` anchored at `b`.
#[test]
fn typeof_multi_segment_first_miss_blames_failing_segment() {
    let source = "type J = typeof import('./mod').a.b;\ndeclare const j: J;\n";
    let diags: Vec<(String, u32)> = check_multi_file(
        &[
            ("mod.ts", "export const local: number = 3;\n"),
            ("main.ts", source),
        ],
        "main.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .filter(|d| d.code == 2694)
    .map(|d| (d.message_text, d.start))
    .collect();
    let expected_start = source.find(".a.").unwrap() as u32 + 1;
    assert_eq!(
        diags,
        vec![(
            "Namespace '\"mod\"' has no exported member 'a'.".to_string(),
            expected_start,
        )],
        "the first failing segment is blamed at its own position"
    );
}

/// Nested namespace member: the traversed segment is appended to the resolved
/// module path with no `.export=` — `Namespace '"ns".Outer'`, matching `tsc`.
#[test]
fn typeof_nested_namespace_member_appends_segment_without_export_eq() {
    let msgs = ts2694_messages(
        &[
            (
                "ns.ts",
                "export namespace Outer { export const inner: number = 4; }\n",
            ),
            (
                "main.ts",
                "type M = typeof import('./ns').Outer.nope;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"ns\".Outer' has no exported member 'nope'.".to_string()],
    );
}

/// Ambient module: no backing file, so the namespace keeps the written
/// specifier form, matching how `tsc` names an ambient module symbol.
#[test]
fn typeof_ambient_module_keeps_written_specifier() {
    let msgs = ts2694_messages(
        &[
            (
                "amb.d.ts",
                "declare module \"amb\" { export const glow: number; }\n",
            ),
            (
                "main.ts",
                "/// <reference path=\"./amb.d.ts\" />\ntype M = typeof import('amb').dim;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"amb\"' has no exported member 'dim'.".to_string()],
    );
}
