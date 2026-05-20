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

fn get_project_diagnostics(files: &[(&str, &str)]) -> Vec<(String, u32, String)> {
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
                .map(|d| (file_name.clone(), d.code, d.message_text.clone())),
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

#[test]
fn export_equals_global_augmentation_namespace_cycle_reports_ts2303_not_ts2686() {
    let source = r#"
declare global { namespace N {} }
export = N;
export as namespace N;
"#;

    let diagnostics = get_diagnostics(source, "a.d.ts");
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2303),
        "Expected TS2303 for export= cycle through global augmentation namespace. Got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2686),
        "Did not expect TS2686 for the export= cycle case. Got: {diagnostics:?}"
    );
}

#[test]
fn recursive_export_assignment_self_import_reports_ts2303() {
    let diagnostics = get_project_diagnostics(&[
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
    ]);

    let ts2303: Vec<_> = diagnostics
        .iter()
        .filter(|(_, code, _)| *code == 2303)
        .collect();
    assert_eq!(
        ts2303.len(),
        1,
        "Expected one TS2303 for recursive export assignment self-import. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2303.iter().any(|(file, _, message)| {
            file == "recursiveExportAssignmentAndFindAliasedType4_moduleC.ts"
                && message.contains("'self'")
        }),
        "Expected TS2303 on moduleC's `self` alias. Actual TS2303 diagnostics: {ts2303:#?}"
    );
}

#[test]
fn recursive_export_assignment_two_file_cycle_reports_ts2303() {
    let diagnostics = get_project_diagnostics(&[
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
    ]);

    let ts2303: Vec<_> = diagnostics
        .iter()
        .filter(|(_, code, _)| *code == 2303)
        .collect();
    assert!(
        ts2303.iter().any(|(file, _, message)| {
            file == "recursiveExportAssignmentAndFindAliasedType5_moduleD.ts"
                && message.contains("'self'")
        }),
        "Expected TS2303 on moduleD's `self` alias. Actual TS2303 diagnostics: {ts2303:#?}"
    );
}

#[test]
fn recursive_export_assignment_three_file_cycle_reports_ts2303() {
    let diagnostics = get_project_diagnostics(&[
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
    ]);

    let ts2303: Vec<_> = diagnostics
        .iter()
        .filter(|(_, code, _)| *code == 2303)
        .collect();
    assert!(
        ts2303.iter().any(|(file, _, message)| {
            file == "recursiveExportAssignmentAndFindAliasedType6_moduleE.ts"
                && message.contains("'self'")
        }),
        "Expected TS2303 on moduleE's `self` alias. Actual TS2303 diagnostics: {ts2303:#?}"
    );
}
