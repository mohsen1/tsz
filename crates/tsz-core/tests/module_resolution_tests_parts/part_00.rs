#[test]
fn module_specifier_candidates_does_not_fan_out_dot_chain_variants() {
    // Contract of the new compatibility shim: at most two entries (canonical
    // plus raw input if different). The old implementation manufactured dot
    // and trailing-slash variants ("." vs "./") on the lookup side so the
    // caller could iterate all spellings. The new design registers BOTH
    // spellings directly in the resolution map (see
    // `index_import_dot_aliases_resolve_in_module_maps`), so no per-lookup
    // fan-out is required.
    use crate::checker::module_resolution::module_specifier_candidates;

    for raw in [".", "./", "..", "../", "../.."] {
        let candidates = module_specifier_candidates(raw);
        assert!(
            candidates.len() <= 2,
            "candidates must not fan out dot-chain variants, got {candidates:?} for {raw:?}",
        );
        assert!(
            candidates.contains(&raw.to_string()),
            "canonical dot-chain specifier must appear in candidates",
        );
    }
}

#[test]
fn index_import_dot_aliases_resolve_in_module_maps() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    let files = vec![
        "/tmp/test/a.ts".to_string(),
        "/tmp/test/a/index.ts".to_string(),
        "/tmp/test/a/test.ts".to_string(),
        "/tmp/test/a/b/test.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(
        paths.get(&(2, ".".to_string())),
        Some(&1),
        "Expected '.' to resolve to a/index.ts from a/test.ts"
    );
    assert_eq!(
        paths.get(&(2, "./".to_string())),
        Some(&1),
        "Expected './' to resolve to a/index.ts from a/test.ts"
    );
    assert_eq!(
        paths.get(&(3, "..".to_string())),
        Some(&1),
        "Expected '..' to resolve to a/index.ts from a/b/test.ts"
    );
    assert_eq!(
        paths.get(&(3, "../".to_string())),
        Some(&1),
        "Expected '../' to resolve to a/index.ts from a/b/test.ts"
    );

    assert!(modules.contains("."));
    assert!(modules.contains("./"));
    assert!(modules.contains(".."));
    assert!(modules.contains("../"));
}

/// Helper to simply parse, bind, and check a file with no cross-file context.
fn check_single_file(source: &str, file_name: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors in {}: {:?}",
        file_name,
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub fn has_error_code(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

fn no_error_code(diagnostics: &[(u32, String)], code: u32) -> bool {
    !has_error_code(diagnostics, code)
}

// TS2307: Cannot find module
const TS2307: u32 = 2307;
// TS2580: Cannot find name ... install @types/node
const TS2580: u32 = 2580;
// TS2591: Cannot find name ... install @types/node and add node to types
const TS2591: u32 = 2591;
// TS2305: Module has no exported member
const TS2305: u32 = 2305;
// TS1202: Import assignment cannot be used when targeting ECMAScript modules
const TS1202: u32 = 1202;
// TS2792: Cannot find module ... did you mean to set moduleResolution
#[allow(dead_code)]
const TS2792: u32 = 2792;
// TS2882: Cannot find module or type declarations for side-effect import
const TS2882: u32 = 2882;

/// Check if diagnostics contain a module-not-found error (either TS2307 or TS2792).
#[allow(dead_code)]
fn has_module_not_found(diagnostics: &[(u32, String)]) -> bool {
    has_error_code(diagnostics, TS2307) || has_error_code(diagnostics, TS2792)
}

// =============================================================================
// ES Import Declaration Tests
// =============================================================================

#[test]
fn test_es_named_import_resolved_module() {
    let source = r#"import { foo } from "./utils";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Should not emit TS2307 for resolved module, got: {diags:?}"
    );
}

#[test]
fn test_es_named_import_unresolved_module() {
    let source = r#"import { foo } from "./nonexistent";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![], vec![]);
    assert!(
        has_module_not_found(&diags),
        "Should emit TS2307 or TS2792 for unresolved module, got: {diags:?}"
    );
}

#[test]
fn test_ts_import_of_node_builtin_uses_ts2580() {
    let diags = check_with_module_not_found_errors(
        r#"import { parse } from "url";
export const thing = () => parse();
"#,
        "usage.ts",
        vec![],
        vec!["url"],
        CheckerOptions {
            module: crate::common::ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error_code(&diags, TS2580),
        "TypeScript import of unresolved Node builtin should emit TS2580, got: {diags:?}"
    );
    assert!(
        no_error_code(&diags, TS2591),
        "TypeScript import of unresolved Node builtin should not emit TS2591, got: {diags:?}"
    );
}

#[test]
fn test_require_like_import_of_node_builtin_uses_ts2591() {
    let diags = check_with_module_not_found_errors(
        r#"import fs = require("fs");
void fs;
"#,
        "test.ts",
        vec![],
        vec!["fs"],
        CheckerOptions {
            module: crate::common::ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error_code(&diags, TS2591),
        "Require-like unresolved Node builtin should emit TS2591, got: {diags:?}"
    );
}

#[test]
fn test_import_type_of_node_builtin_uses_ts2591() {
    let diags = check_with_module_not_found_errors(
        r#"export type Builtin = import("module").Module;"#,
        "types.ts",
        vec![],
        vec!["module"],
        CheckerOptions {
            module: crate::common::ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error_code(&diags, TS2591),
        "Import-type unresolved Node builtin should emit TS2591, got: {diags:?}"
    );
    assert!(
        no_error_code(&diags, TS2580),
        "Import-type unresolved Node builtin should not emit TS2580, got: {diags:?}"
    );
}

#[test]
fn test_dynamic_import_of_node_builtin_uses_ts2591_with_no_types_and_symbols() {
    let diags = check_with_module_not_found_errors(
        r#"import("node:path");"#,
        "no.ts",
        vec![],
        vec!["node:path"],
        CheckerOptions {
            module: crate::common::ModuleKind::Preserve,
            no_types_and_symbols: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error_code(&diags, TS2591),
        "Dynamic import of unresolved Node builtin should emit TS2591, got: {diags:?}"
    );
    assert!(
        no_error_code(&diags, TS2307),
        "Dynamic import of unresolved Node builtin should not emit TS2307, got: {diags:?}"
    );
}

#[test]
fn test_no_types_and_symbols_keeps_ts2591_for_node_builtin_imports() {
    let diags = check_with_module_not_found_errors(
        r#"import { parse } from "url";
export const thing = () => parse();
"#,
        "usage.ts",
        vec![],
        vec!["url"],
        CheckerOptions {
            module: crate::common::ModuleKind::CommonJS,
            no_types_and_symbols: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error_code(&diags, TS2591),
        "noTypesAndSymbols should keep TS2591 for unresolved Node builtins, got: {diags:?}"
    );
}

// TS2591: noTypesAndSymbols + Node module resolution should emit TS2591
// for known Node built-ins (not suppress them). This exercises the
// is_node_builtin guard in check_import_declaration.
#[test]
fn test_no_types_and_symbols_with_node_module_kind_emits_ts2591() {
    let diags = check_with_module_not_found_errors(
        r#"import { parse } from "url";
export const thing = parse();
"#,
        "usage.ts",
        vec![],
        vec!["url"],
        CheckerOptions {
            module: crate::common::ModuleKind::NodeNext,
            no_types_and_symbols: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error_code(&diags, TS2591),
        "noTypesAndSymbols + NodeNext should emit TS2591 for Node builtins, got: {diags:?}"
    );
    assert!(
        no_error_code(&diags, TS2307),
        "noTypesAndSymbols + NodeNext should not emit TS2307 for Node builtins, got: {diags:?}"
    );
}

