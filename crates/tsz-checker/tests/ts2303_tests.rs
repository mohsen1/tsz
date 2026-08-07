//! Tests for TS2303: Circular definition of import alias.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn get_diagnostics(source: &str, file_name: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            isolated_modules: true,
            ..Default::default()
        },
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Asserts the `TS2303` set for an `import self = require(...)` / `export = self`
/// cycle matches what `tsc` reports.
///
/// `tsc` treats the import declaration and the `export =` assignment as two
/// independent circular-alias sites, so every module participating in the cycle
/// is reported exactly twice — once at each. Modules outside the cycle are
/// reported not at all.
fn assert_self_alias_cycle_sites(
    files: &[(&str, &str)],
    diagnostics: &[(String, u32, u32, String)],
    cycle_members: &[&str],
) {
    let ts2303: Vec<_> = diagnostics
        .iter()
        .filter(|(_, code, _, _)| *code == 2303)
        .collect();

    for member in cycle_members {
        let source = files
            .iter()
            .find_map(|(name, source)| (name == member).then_some(*source))
            .unwrap_or_else(|| panic!("cycle member {member} is not in the fixture"));
        let import_site = u32::try_from(source.find("import self").expect("import site")).unwrap();
        let export_site =
            u32::try_from(source.find("export = self").expect("export site")).unwrap();

        let sites: Vec<u32> = ts2303
            .iter()
            .filter(|(file, _, _, message)| file == member && message.contains("'self'"))
            .map(|(_, _, start, _)| *start)
            .collect();

        assert!(
            sites.contains(&import_site),
            "Expected TS2303 on {member}'s `import self = require(...)` declaration (offset {import_site}). Actual TS2303: {ts2303:#?}"
        );
        assert!(
            sites.contains(&export_site),
            "Expected TS2303 on {member}'s `export = self;` assignment (offset {export_site}). Actual TS2303: {ts2303:#?}"
        );
        assert_eq!(
            sites.len(),
            2,
            "Expected exactly two TS2303 sites in {member}, with no duplicate at either. Actual TS2303: {ts2303:#?}"
        );
    }

    assert_eq!(
        ts2303.len(),
        cycle_members.len() * 2,
        "Only the {} cycle members may report TS2303, twice each. Actual diagnostics: {diagnostics:#?}",
        cycle_members.len()
    );
}

/// Checks a multi-file project, returning `(file, code, start, message)` per
/// diagnostic.
///
/// A bare `(file, code, message)` triple cannot tell two reports at the same
/// site apart from two reports at different sites, which is exactly the
/// distinction `tsc` draws for a circular import alias: it reports `TS2303`
/// once on the `import` declaration and once on the `export =` assignment.
fn get_project_diagnostics_positioned(files: &[(&str, &str)]) -> Vec<(String, u32, u32, String)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut diagnostics = Vec::new();

    for (file_idx, file_name) in file_names.iter().enumerate() {
        let mut checker = CheckerState::new(
            all_arenas[file_idx].as_ref(),
            all_binders[file_idx].as_ref(),
            &types,
            file_name.clone(),
            CheckerOptions {
                module: ModuleKind::CommonJS,
                no_lib: true,
                ..Default::default()
            },
        );
        checker.enable_source_file_test_pragmas();
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(file_idx);
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths.clone()));
        checker.ctx.set_resolved_modules(resolved_modules.clone());

        checker.check_source_file(roots[file_idx]);

        diagnostics.extend(
            checker
                .ctx
                .diagnostics
                .iter()
                .map(|d| (file_name.clone(), d.code, d.start, d.message_text.clone())),
        );
    }

    diagnostics
}

#[test]
fn export_as_namespace_is_not_circular_alias() {
    // `export as namespace X` creates an ALIAS-flagged symbol in the binder with
    // is_umd_export = true. This is an outbound UMD export, NOT an import alias.
    // The circular alias checker must skip these symbols.
    let source = r#"
export = React;
export as namespace React;

declare namespace React {
  type ReactNode = string | number | boolean | null | undefined;
  function createElement(): void;
}
"#;

    let diagnostics = get_diagnostics(source, "react-index.d.ts");
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2303),
        "Should not emit TS2303 for `export as namespace`. Got: {diagnostics:?}"
    );
}

#[test]
fn ambient_require_alias_reexport_is_not_a_circular_alias() {
    let source = r#"
declare module "events" {
  interface EventEmitterOptions {
    captureRejections?: boolean;
  }
  class EventEmitter {
    constructor(options?: EventEmitterOptions);
  }
  export = EventEmitter;
}
declare module "node:events" {
  import events = require("events");
  export = events;
}
"#;

    let diagnostics = get_diagnostics(source, "events.d.ts");
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2303),
        "Did not expect TS2303 for ambient import alias re-export, got: {diagnostics:?}"
    );
}

#[test]
fn ambient_require_alias_self_import_still_reports_ts2303() {
    // `declare module "moduleC" { import self = require("moduleC"); ... }` —
    // the require target equals the enclosing ambient module's specifier, so
    // the alias really is self-referential. tsc emits TS2303; we must too.
    let source = r#"
declare module "moduleC" {
    import self = require("moduleC");
    export = self;
}
"#;
    let diagnostics = get_diagnostics(source, "self.d.ts");
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2303),
        "Expected TS2303 for `import self = require(\"moduleC\")` inside `declare module \"moduleC\"`. Got: {diagnostics:?}"
    );
}

/// tsc reports `TS2303` once at EVERY alias declaration participating in a
/// cycle (the general rule `check_circular_import_aliases` in
/// `module_checker.rs` already documents), not once for the whole cycle. For
/// `declare global { namespace N {} } export = N; export as namespace N;`
/// that means two sites — the `export = N` assignment AND the
/// `export as namespace N` declaration that closes the loop back to the
/// global augmentation's `N` — confirmed against the pinned `typescript@7.0.2`
/// oracle (`a.d.ts(2,1)` and `a.d.ts(3,1)`, both `Circular definition of
/// import alias 'N'.`).
fn assert_global_augmentation_cycle_sites(source: &str, file_name: &str, ident: &str) {
    let diagnostics = get_diagnostics_positioned(source, file_name);
    let ts2303: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _, _)| *code == 2303)
        .collect();

    let export_equals_site = u32::try_from(
        source
            .find(&format!("export = {ident}"))
            .expect("export= site"),
    )
    .unwrap();
    let export_as_namespace_site = u32::try_from(
        source
            .find(&format!("export as namespace {ident}"))
            .expect("export as namespace site"),
    )
    .unwrap();

    assert!(
        ts2303
            .iter()
            .any(|(_, start, _)| *start == export_equals_site),
        "Expected TS2303 at the `export = {ident}` statement. Got: {ts2303:#?}"
    );
    assert!(
        ts2303
            .iter()
            .any(|(_, start, _)| *start == export_as_namespace_site),
        "Expected TS2303 at the `export as namespace {ident}` statement. Got: {ts2303:#?}"
    );
    assert_eq!(
        ts2303.len(),
        2,
        "Expected exactly the two cycle sites, no more. Got: {ts2303:#?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _, _)| *code != 2686),
        "Did not expect TS2686 for the export= cycle case. Got: {diagnostics:#?}"
    );
}

#[test]
fn export_equals_global_augmentation_namespace_cycle_reports_ts2303_not_ts2686() {
    let source = r#"
declare global { namespace N {} }
export = N;
export as namespace N;
"#;
    assert_global_augmentation_cycle_sites(source, "a.d.ts", "N");
}

#[test]
fn export_equals_global_augmentation_namespace_cycle_is_order_independent() {
    // Same cycle, `export as namespace` written before `export =` — the scan
    // is over all of the file's statements, not a fixed sequence, and a
    // renamed binder (`M`, not `N`) so the fix cannot be keyed off the name.
    let source = r#"
declare global { namespace M {} }
export as namespace M;
export = M;
"#;
    assert_global_augmentation_cycle_sites(source, "swapped.d.ts", "M");
}

#[test]
fn export_equals_global_augmentation_namespace_cycle_renamed_binder() {
    let source = r#"
declare global { namespace Widget {} }
export = Widget;
export as namespace Widget;
"#;
    assert_global_augmentation_cycle_sites(source, "renamed.d.ts", "Widget");
}

#[test]
fn export_equals_without_matching_umd_export_is_not_circular() {
    // `export = N` alone, with no `export as namespace N` to close the loop,
    // is not a cycle — the global augmentation's `N` is a genuine value
    // target, not an alias back to itself.
    let source = r#"
declare global { namespace N {} }
export = N;
"#;
    let diagnostics = get_diagnostics(source, "no-umd.d.ts");
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2303),
        "Did not expect TS2303 without a matching `export as namespace`. Got: {diagnostics:?}"
    );
}

#[test]
fn recursive_export_assignment_self_import_reports_ts2303() {
    const FILES: &[(&str, &str)] = &[
        (
            "recursiveExportAssignmentAndFindAliasedType4_moduleC.ts",
            r#"import self = require("./recursiveExportAssignmentAndFindAliasedType4_moduleC");
export = self;"#,
        ),
        (
            "recursiveExportAssignmentAndFindAliasedType4_moduleB.ts",
            r#"class ClassB { }
export = ClassB;"#,
        ),
        (
            "recursiveExportAssignmentAndFindAliasedType4_moduleA.ts",
            r#"import moduleC = require("./recursiveExportAssignmentAndFindAliasedType4_moduleC");
import ClassB = require("./recursiveExportAssignmentAndFindAliasedType4_moduleB");
export var b: ClassB;"#,
        ),
    ];

    let diagnostics = get_project_diagnostics_positioned(FILES);

    assert_self_alias_cycle_sites(
        FILES,
        &diagnostics,
        &["recursiveExportAssignmentAndFindAliasedType4_moduleC.ts"],
    );
}

#[test]
fn recursive_export_assignment_two_file_cycle_reports_ts2303() {
    const FILES: &[(&str, &str)] = &[
        (
            "recursiveExportAssignmentAndFindAliasedType5_moduleC.ts",
            r#"import self = require("./recursiveExportAssignmentAndFindAliasedType5_moduleD");
export = self;"#,
        ),
        (
            "recursiveExportAssignmentAndFindAliasedType5_moduleD.ts",
            r#"import self = require("./recursiveExportAssignmentAndFindAliasedType5_moduleC");
export = self;"#,
        ),
        (
            "recursiveExportAssignmentAndFindAliasedType5_moduleB.ts",
            r#"class ClassB { }
export = ClassB;"#,
        ),
        (
            "recursiveExportAssignmentAndFindAliasedType5_moduleA.ts",
            r#"import moduleC = require("./recursiveExportAssignmentAndFindAliasedType5_moduleC");
import ClassB = require("./recursiveExportAssignmentAndFindAliasedType5_moduleB");
export var b: ClassB;"#,
        ),
    ];

    let diagnostics = get_project_diagnostics_positioned(FILES);

    assert_self_alias_cycle_sites(
        FILES,
        &diagnostics,
        &[
            "recursiveExportAssignmentAndFindAliasedType5_moduleC.ts",
            "recursiveExportAssignmentAndFindAliasedType5_moduleD.ts",
        ],
    );
}

#[test]
fn recursive_export_assignment_three_file_cycle_reports_ts2303() {
    const FILES: &[(&str, &str)] = &[
        (
            "recursiveExportAssignmentAndFindAliasedType6_moduleC.ts",
            r#"import self = require("./recursiveExportAssignmentAndFindAliasedType6_moduleD");
export = self;"#,
        ),
        (
            "recursiveExportAssignmentAndFindAliasedType6_moduleD.ts",
            r#"import self = require("./recursiveExportAssignmentAndFindAliasedType6_moduleE");
export = self;"#,
        ),
        (
            "recursiveExportAssignmentAndFindAliasedType6_moduleE.ts",
            r#"import self = require("./recursiveExportAssignmentAndFindAliasedType6_moduleC");
export = self;"#,
        ),
        (
            "recursiveExportAssignmentAndFindAliasedType6_moduleB.ts",
            r#"class ClassB { }
export = ClassB;"#,
        ),
        (
            "recursiveExportAssignmentAndFindAliasedType6_moduleA.ts",
            r#"import moduleC = require("./recursiveExportAssignmentAndFindAliasedType6_moduleC");
import ClassB = require("./recursiveExportAssignmentAndFindAliasedType6_moduleB");
export var b: ClassB;"#,
        ),
    ];

    let diagnostics = get_project_diagnostics_positioned(FILES);

    assert_self_alias_cycle_sites(
        FILES,
        &diagnostics,
        &[
            "recursiveExportAssignmentAndFindAliasedType6_moduleC.ts",
            "recursiveExportAssignmentAndFindAliasedType6_moduleD.ts",
            "recursiveExportAssignmentAndFindAliasedType6_moduleE.ts",
        ],
    );
}

// ---------------------------------------------------------------------------
// Ambient-module alias cycles
//
// `declare module "m" { import self = require("m"); export = self; }` is the
// same cycle as the file-level `recursiveExportAssignmentAndFindAliasedType4-6`
// shapes above, written inside an ambient module body instead of at file top
// level. `tsc` reports TS2303 identically in both: once at the `import` and
// once at the `export =`. These pin the ambient half, whose `export =` site
// lives in the module block rather than in `SourceFile.statements`.
//
// Offsets are pinned rather than counted: a count-only assertion cannot tell
// "two sites" from "one site reported twice", which is exactly the distinction
// at issue here.
// ---------------------------------------------------------------------------

/// Single-file diagnostics carrying the start offset of each report.
fn get_diagnostics_positioned(source: &str, file_name: &str) -> Vec<(u32, u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            isolated_modules: true,
            ..Default::default()
        },
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

/// Start offsets of every TS2303 in `source`, sorted.
///
/// Sorting is deliberate: the emission order of the import site and its
/// companion `export =` site is a checker-traversal artifact, not a parity
/// claim, so only the set of anchors is asserted on.
fn ts2303_offsets(source: &str, file_name: &str) -> Vec<u32> {
    let mut offsets: Vec<u32> = get_diagnostics_positioned(source, file_name)
        .into_iter()
        .filter(|(code, _, _)| *code == 2303)
        .map(|(_, start, _)| start)
        .collect();
    offsets.sort_unstable();
    offsets
}

/// Byte offset of the `nth` (0-based) occurrence of `needle` in `source`.
fn nth_offset(source: &str, needle: &str, nth: usize) -> u32 {
    let mut from = 0usize;
    for _ in 0..nth {
        let at = source[from..]
            .find(needle)
            .unwrap_or_else(|| panic!("occurrence {nth} of {needle:?} not found"));
        from += at + needle.len();
    }
    let at = source[from..]
        .find(needle)
        .unwrap_or_else(|| panic!("occurrence {nth} of {needle:?} not found"));
    u32::try_from(from + at).unwrap()
}

#[test]
fn ambient_module_self_alias_cycle_reports_ts2303_at_import_and_export_equals() {
    // `recursiveExportAssignmentAndFindAliasedType1`. tsc 7.0.2:
    //   def.d.ts(2,5) TS2303  <- import self = require("moduleC");
    //   def.d.ts(3,5) TS2303  <- export = self;
    const SOURCE: &str = r#"declare module "moduleC" {
    import self = require("moduleC");
    export = self;
}
"#;

    assert_eq!(
        ts2303_offsets(SOURCE, "def.d.ts"),
        vec![
            nth_offset(SOURCE, "import self", 0),
            nth_offset(SOURCE, "export = self", 0),
        ],
        "Expected TS2303 at both the import and the `export =` inside the ambient module"
    );
}

#[test]
fn ambient_module_two_module_alias_cycle_reports_ts2303_at_every_site() {
    // `recursiveExportAssignmentAndFindAliasedType2`: a two-module cycle whose
    // members each contribute an import site and an `export =` site (4 total).
    const SOURCE: &str = r#"declare module "moduleC" {
    import self = require("moduleD");
    export = self;
}
declare module "moduleD" {
    import self = require("moduleC");
    export = self;
}
"#;

    assert_eq!(
        ts2303_offsets(SOURCE, "def.d.ts"),
        vec![
            nth_offset(SOURCE, "import self", 0),
            nth_offset(SOURCE, "export = self", 0),
            nth_offset(SOURCE, "import self", 1),
            nth_offset(SOURCE, "export = self", 1),
        ],
        "Expected TS2303 at all four alias sites of the two-module ambient cycle"
    );
}

#[test]
fn ambient_module_alias_cycle_export_site_does_not_depend_on_the_binder_name() {
    // Same cycle as above with the alias renamed: the `export =` companion is
    // found through the enclosing module's export table, never by name.
    const SOURCE: &str = r#"declare module "cyc" {
    import zzz = require("cyc");
    export = zzz;
}
"#;

    assert_eq!(
        ts2303_offsets(SOURCE, "renamed.d.ts"),
        vec![
            nth_offset(SOURCE, "import zzz", 0),
            nth_offset(SOURCE, "export = zzz", 0),
        ],
        "Renaming the alias must not change which sites report TS2303"
    );
}

#[test]
fn ambient_module_export_equals_outside_the_cycle_stays_clean() {
    // Negative control, and the reason the companion scan is scoped to the
    // container that declares the alias: two more ambient modules in the SAME
    // file each carry their own `export =`, and neither is circular. A scan
    // that walked the file rather than the cyclic alias's own module body
    // would light all three up.
    const SOURCE: &str = r#"declare module "cyc" {
    import zzz = require("cyc");
    export = zzz;
}
declare module "sane" {
    class Real { }
    export = Real;
}
declare module "borrows" {
    import ok = require("sane");
    export = ok;
}
"#;

    assert_eq!(
        ts2303_offsets(SOURCE, "mixed.d.ts"),
        vec![
            nth_offset(SOURCE, "import zzz", 0),
            nth_offset(SOURCE, "export = zzz", 0),
        ],
        "Only the circular ambient module may report TS2303; its non-circular siblings' `export =` must stay clean"
    );
}

// =============================================================================
// Local `export { X as Y }` companion sites (no `from` clause)
// =============================================================================
//
// A local export specifier does not get its own alias symbol in this binder:
// `bind_export_declaration` publishes the exported name onto the *local* symbol
// it names, so a cyclic import alias and every local re-export of it share one
// `SymbolId`. tsc gives each specifier its own alias symbol, puts it on the same
// cycle, and reports TS2303 at every member. All expectations below are pinned
// against tsc 7.0.2.

/// Sorted `(file, offset, message)` triples for every TS2303 in a project.
///
/// Position and message are both pinned: a set that discards either cannot tell
/// "two members of the cycle" from "one member reported twice", and the exported
/// name is exactly what distinguishes the import site from its companion.
fn ts2303_sites(files: &[(&str, &str)]) -> Vec<(String, u32, String)> {
    let mut sites: Vec<(String, u32, String)> = get_project_diagnostics_positioned(files)
        .into_iter()
        .filter(|(_, code, _, _)| *code == 2303)
        .map(|(file, _, start, message)| (file, start, message))
        .collect();
    sites.sort();
    sites
}

fn circular_alias_message(name: &str) -> String {
    format!("Circular definition of import alias '{name}'.")
}

fn site(files: &[(&str, &str)], file: &str, needle: &str, alias: &str) -> (String, u32, String) {
    let source = files
        .iter()
        .find_map(|(name, source)| (*name == file).then_some(*source))
        .unwrap_or_else(|| panic!("{file} is not in the fixture"));
    (
        file.to_string(),
        nth_offset(source, needle, 0),
        circular_alias_message(alias),
    )
}

#[test]
fn local_export_specifier_on_an_import_alias_cycle_reports_its_own_site() {
    // `conformance/externalModules/typeOnly/circular3.ts`. tsc 7.0.2:
    //   a.ts(1,15) TS2303 'A'   <- import type { A } from './b';
    //   a.ts(2,15) TS2303 'B'   <- export type { A as B };
    //   b.ts(1,15) TS2303 'B'   <- import type { B } from './a';
    //   b.ts(2,15) TS2303 'A'   <- export type { B as A };
    const FILES: &[(&str, &str)] = &[
        (
            "a.ts",
            "import type { A } from './b';\nexport type { A as B };\n",
        ),
        (
            "b.ts",
            "import type { B } from './a';\nexport type { B as A };\n",
        ),
    ];

    assert_eq!(
        ts2303_sites(FILES),
        vec![
            site(FILES, "a.ts", "A } from", "A"),
            site(FILES, "a.ts", "A as B", "B"),
            site(FILES, "b.ts", "B } from", "B"),
            site(FILES, "b.ts", "B as A", "A"),
        ],
        "each of the four alias declarations on the cycle reports at its own anchor"
    );
}

#[test]
fn local_export_specifier_cycle_is_unchanged_by_renamed_binders() {
    // Same shape as above with every binder renamed. The rule is structural, so
    // the site set must be identical modulo the names.
    const FILES: &[(&str, &str)] = &[
        (
            "a.ts",
            "import type { Zeta } from './b';\nexport type { Zeta as Omega };\n",
        ),
        (
            "b.ts",
            "import type { Omega } from './a';\nexport type { Omega as Zeta };\n",
        ),
    ];

    assert_eq!(
        ts2303_sites(FILES),
        vec![
            site(FILES, "a.ts", "Zeta } from", "Zeta"),
            site(FILES, "a.ts", "Zeta as Omega", "Omega"),
            site(FILES, "b.ts", "Omega } from", "Omega"),
            site(FILES, "b.ts", "Omega as Zeta", "Zeta"),
        ],
        "renaming every binder must not change which sites report"
    );
}

#[test]
fn local_export_specifier_without_a_rename_reports_the_exported_name() {
    // `export type { A };` — property_name is absent, so the local name and the
    // exported name coincide and both reports read 'A'.
    const FILES: &[(&str, &str)] = &[
        (
            "a.ts",
            "import type { A } from './b';\nexport type { A };\n",
        ),
        (
            "b.ts",
            "import type { A } from './a';\nexport type { A };\n",
        ),
    ];

    assert_eq!(
        ts2303_sites(FILES),
        vec![
            site(FILES, "a.ts", "A } from", "A"),
            site(FILES, "a.ts", "A };", "A"),
            site(FILES, "b.ts", "A } from", "A"),
            site(FILES, "b.ts", "A };", "A"),
        ],
        "a bare specifier still gets its own site"
    );
}

#[test]
fn value_form_local_export_specifier_cycle_reports_its_own_site() {
    // The rule is about the alias graph, not about type-only syntax: the plain
    // `import`/`export` form reports the same four sites.
    const FILES: &[(&str, &str)] = &[
        ("a.ts", "import { A } from './b';\nexport { A as B };\n"),
        ("b.ts", "import { B } from './a';\nexport { B as A };\n"),
    ];

    assert_eq!(
        ts2303_sites(FILES),
        vec![
            site(FILES, "a.ts", "A } from", "A"),
            site(FILES, "a.ts", "A as B", "B"),
            site(FILES, "b.ts", "B } from", "B"),
            site(FILES, "b.ts", "B as A", "A"),
        ],
        "the value form is on the same alias cycle as the type-only form"
    );
}

#[test]
fn a_local_export_specifier_off_the_cycle_is_not_reported() {
    // `export type { A as B, A as C };` — the cycle re-enters a.ts under 'B'
    // only. 'C' aliases the same local but is a branch off the cycle: tsc
    // resolves it through an already-resolved alias and never revisits it.
    const FILES: &[(&str, &str)] = &[
        (
            "a.ts",
            "import type { A } from './b';\nexport type { A as B, A as C };\n",
        ),
        (
            "b.ts",
            "import type { B } from './a';\nexport type { B as A };\n",
        ),
    ];

    let sites = ts2303_sites(FILES);
    assert_eq!(
        sites,
        vec![
            site(FILES, "a.ts", "A } from", "A"),
            site(FILES, "a.ts", "A as B", "B"),
            site(FILES, "b.ts", "B } from", "B"),
            site(FILES, "b.ts", "B as A", "A"),
        ],
        "only the specifier the cycle re-enters under is a member of it"
    );
    assert!(
        !sites.iter().any(|(_, _, message)| message.contains("'C'")),
        "the second specifier for the same local aliases a resolved symbol, not a cyclic one: {sites:#?}"
    );
}

#[test]
fn a_local_export_specifier_over_a_resolvable_alias_stays_clean() {
    // Same syntax, no cycle: `A` resolves to a real declaration, so neither the
    // import nor its companion specifier may report.
    const FILES: &[(&str, &str)] = &[
        (
            "a.ts",
            "import type { A } from './b';\nexport type { A as B };\n",
        ),
        ("b.ts", "export type A = string;\n"),
    ];

    assert_eq!(
        ts2303_sites(FILES),
        Vec::new(),
        "a resolvable alias is not circular at either site"
    );
}

#[test]
fn a_local_export_specifier_over_a_plain_local_stays_clean() {
    // No alias at all — the specifier names a type declared in the same file.
    const FILES: &[(&str, &str)] = &[("a.ts", "type A = string;\nexport type { A as B };\n")];

    assert_eq!(
        ts2303_sites(FILES),
        Vec::new(),
        "a specifier over a non-alias local is never circular"
    );
}

#[test]
fn a_healthy_re_export_beside_a_cyclic_one_stays_clean() {
    // Two specifiers in the same file, one on the cycle and one not. Only the
    // cyclic one reports, which is what keeps the scan from being file-scoped.
    const FILES: &[(&str, &str)] = &[
        (
            "a.ts",
            "import type { A } from './b';\nimport type { Ok } from './c';\nexport type { A as B };\nexport type { Ok as Fine };\n",
        ),
        (
            "b.ts",
            "import type { B } from './a';\nexport type { B as A };\n",
        ),
        ("c.ts", "export type Ok = string;\n"),
    ];

    assert_eq!(
        ts2303_sites(FILES),
        vec![
            site(FILES, "a.ts", "A } from", "A"),
            site(FILES, "a.ts", "A as B", "B"),
            site(FILES, "b.ts", "B } from", "B"),
            site(FILES, "b.ts", "B as A", "A"),
        ],
        "the resolvable re-export in the same file must stay clean"
    );
}

#[test]
fn a_three_file_local_export_specifier_cycle_reports_every_member() {
    // The cycle is longer than one hop out and back, so the walk has to carry
    // the re-entry name across more than a single file.
    const FILES: &[(&str, &str)] = &[
        (
            "a.ts",
            "import type { A } from './b';\nexport type { A as B };\n",
        ),
        (
            "b.ts",
            "import type { B } from './c';\nexport type { B as A };\n",
        ),
        (
            "c.ts",
            "import type { B } from './a';\nexport type { B };\n",
        ),
    ];

    assert_eq!(
        ts2303_sites(FILES),
        vec![
            site(FILES, "a.ts", "A } from", "A"),
            site(FILES, "a.ts", "A as B", "B"),
            site(FILES, "b.ts", "B } from", "B"),
            site(FILES, "b.ts", "B as A", "A"),
            site(FILES, "c.ts", "B } from", "B"),
            site(FILES, "c.ts", "B };", "B"),
        ],
        "every alias declaration around the three-file cycle reports once"
    );
}

#[test]
fn an_export_specifier_with_a_from_clause_keeps_its_single_site() {
    // `conformance/externalModules/typeOnly/circular1.ts`. A specifier that
    // carries `from` sets ALIAS + import_module and is already its own entry in
    // the collection scan, so the companion scan must leave it alone — two
    // sites, not four.
    const FILES: &[(&str, &str)] = &[
        ("a.ts", "export type { A } from './b';\n"),
        ("b.ts", "export type { A } from './a';\n"),
    ];

    assert_eq!(
        ts2303_sites(FILES),
        vec![
            site(FILES, "a.ts", "A } from", "A"),
            site(FILES, "b.ts", "A } from", "A"),
        ],
        "a re-export specifier must not be double-counted by the companion scan"
    );
}
