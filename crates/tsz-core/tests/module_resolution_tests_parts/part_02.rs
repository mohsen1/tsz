#[test]
fn test_es_reexport_unresolved() {
    let source = r#"export { foo } from "./nonexistent";"#;
    let diags = check_with_resolved_modules(source, "barrel.ts", vec![], vec![]);
    assert!(
        has_module_not_found(&diags),
        "Re-export from unresolved module should emit TS2307 or TS2792, got: {diags:?}"
    );
}

#[test]
fn test_es_wildcard_reexport_resolved() {
    let source = r#"export * from "./utils";"#;
    let diags = check_with_resolved_modules(source, "barrel.ts", vec!["./utils"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Wildcard re-export from resolved module should not error, got: {diags:?}"
    );
}

#[test]
fn test_es_namespace_reexport_resolved() {
    let source = r#"export * as utils from "./utils";"#;
    let diags = check_with_resolved_modules(source, "barrel.ts", vec!["./utils"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Namespace re-export from resolved module should not error, got: {diags:?}"
    );
}

// =============================================================================
// Import Equals Declaration Tests (import x = require("..."))
// =============================================================================

#[test]
fn test_import_equals_require_resolved() {
    let source = r#"import utils = require("./utils");"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "import = require should resolve, got: {diags:?}"
    );
}

#[test]
fn test_import_equals_require_unresolved() {
    let source = r#"import utils = require("./nonexistent");"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![], vec![]);
    assert!(
        has_module_not_found(&diags),
        "Unresolved import = require should emit TS2307 or TS2792, got: {diags:?}"
    );
}

#[test]
fn test_import_equals_require_in_esm_emits_ts1202() {
    // When targeting ESM, import = require should emit TS1202
    let source = r#"import utils = require("./utils");"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: crate::common::ModuleKind::ES2015,
        module_explicitly_set: true,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "main.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&TS1202),
        "import = require in ESM should emit TS1202, got: {codes:?}"
    );
}

#[test]
fn test_import_equals_namespace_alias() {
    // import x = Namespace (not require)
    let source = r#"
namespace MyNamespace {
    export const value = 42;
}
import Alias = MyNamespace;
const x = Alias.value;
"#;
    let diags = check_single_file(source, "main.ts");
    // Should not have TS2503 (Cannot find namespace) since MyNamespace exists
    assert!(
        no_error_code(&diags, 2503),
        "Namespace import should resolve, got: {diags:?}"
    );
}

// =============================================================================
// Module Exports and Import Member Checking Tests
// =============================================================================

#[test]
fn test_import_nonexistent_member_from_module() {
    let source = r#"import { nonexistent } from "./utils";"#;
    let diags = check_with_module_exports(
        source,
        "main.ts",
        vec![("./utils", vec![("foo", 0), ("bar", 0)])],
        true,
    );
    assert!(
        has_error_code(&diags, TS2305),
        "Importing nonexistent member should emit TS2305, got: {diags:?}"
    );
}

#[test]
fn test_import_existing_member_from_module() {
    let source = r#"import { foo } from "./utils";"#;
    let diags = check_with_module_exports(
        source,
        "main.ts",
        vec![("./utils", vec![("foo", 0), ("bar", 0)])],
        true,
    );
    assert!(
        no_error_code(&diags, TS2305),
        "Importing existing member should not emit TS2305, got: {diags:?}"
    );
    assert!(
        no_error_code(&diags, TS2307),
        "Module with exports should not emit TS2307, got: {diags:?}"
    );
}

#[test]
fn test_import_renamed_member() {
    let source = r#"import { foo as myFoo } from "./utils";"#;
    let diags =
        check_with_module_exports(source, "main.ts", vec![("./utils", vec![("foo", 0)])], true);
    assert!(
        no_error_code(&diags, TS2305),
        "Renamed import of existing member should not error, got: {diags:?}"
    );
}

