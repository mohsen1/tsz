#[test]
fn test_es_default_import_resolved() {
    let source = r#"import utils from "./utils";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Default import should resolve, got: {diags:?}"
    );
}

#[test]
fn test_es_namespace_import_resolved() {
    let source = r#"import * as utils from "./utils";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Namespace import should resolve, got: {diags:?}"
    );
}

#[test]
fn test_import_equals_require_uses_export_equals_constructable_target() {
    let source = r#"
        import A = require("M");
        var c = new A();
    "#;
    let module_source = r#"
        namespace C {
            export var f: number;
        }
        class C {
            foo(): void;
        }
        export = C;
    "#;

    let diags = check_with_module_sources(source, "main.ts", vec![("M", module_source)]);
    assert!(
        no_error_code(&diags, 2351),
        "import=require should resolve to constructable export= target, got: {diags:?}"
    );
}

#[test]
fn test_es_side_effect_import_resolved() {
    let source = r#"import "./polyfill";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./polyfill"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Side-effect import should resolve, got: {diags:?}"
    );
}

#[test]
fn test_es_side_effect_import_unresolved() {
    // Side-effect imports are only checked when noUncheckedSideEffectImports is true.
    // With the default (false, matching tsc), side-effect imports are silently accepted.
    let source = r#"import "./nonexistent";"#;
    let diags = check_with_resolved_modules_opts(
        source,
        "main.ts",
        vec![],
        vec![],
        CheckerOptions {
            no_unchecked_side_effect_imports: true,
            ..CheckerOptions::default()
        },
    );
    // TS2882: Cannot find module or type declarations for side-effect import.
    assert!(
        has_error_code(&diags, TS2882),
        "Unresolved side-effect import should emit TS2882 when noUncheckedSideEffectImports is true, got: {diags:?}"
    );
}

#[test]
fn test_es_side_effect_import_unresolved_default_emits_error() {
    // With default options (noUncheckedSideEffectImports: true in tsc 6.0),
    // unresolved side-effect imports should produce TS2882.
    let source = r#"import "./nonexistent";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![], vec![]);
    assert!(
        has_error_code(&diags, TS2882),
        "Side-effect imports should emit TS2882 by default (tsc 6.0), got: {diags:?}"
    );
}

#[test]
fn test_es_type_only_import() {
    let source = r#"import type { Foo } from "./types";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./types"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Type-only import should resolve, got: {diags:?}"
    );
}

#[test]
fn test_es_import_multiple_specifiers() {
    let source = r#"
import { a } from "./mod-a";
import { b } from "./mod-b";
import { c } from "./mod-c";
"#;
    let diags = check_with_resolved_modules(
        source,
        "main.ts",
        vec!["./mod-a", "./mod-b", "./mod-c"],
        vec![],
    );
    assert!(
        no_error_code(&diags, TS2307),
        "Multiple resolved imports should not error, got: {diags:?}"
    );
}

#[test]
fn test_es_import_partial_resolution() {
    let source = r#"
import { a } from "./exists";
import { b } from "./missing";
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./exists"], vec![]);
    assert!(
        has_module_not_found(&diags),
        "Unresolved import should produce TS2307 or TS2792, got: {diags:?}"
    );
    // Verify the error is about the missing module specifically
    let module_errors: Vec<_> = diags
        .iter()
        .filter(|(c, _)| *c == TS2307 || *c == TS2792)
        .collect();
    assert_eq!(
        module_errors.len(),
        1,
        "Only one module-not-found error should be emitted, got: {module_errors:?}"
    );
    assert!(
        module_errors[0].1.contains("./missing"),
        "Module-not-found message should reference './missing', got: {}",
        module_errors[0].1
    );
}

// =============================================================================
// ES Re-export Tests
// =============================================================================

#[test]
fn test_es_reexport_resolved() {
    let source = r#"export { foo } from "./utils";"#;
    let diags = check_with_resolved_modules(source, "barrel.ts", vec!["./utils"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Re-export from resolved module should not error, got: {diags:?}"
    );
}

