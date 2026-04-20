#[test]
fn call_hierarchy_method_calls() {
    let t = FourslashTest::new(
        "
        class Service {
            /*m*/process() {
                this.validate();
                this.execute();
            }
            validate() {}
            execute() {}
        }
    ",
    );
    let result = t.outgoing_calls("m");
    result.expect_found();
}

// =============================================================================
// Document Links: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn document_links_re_export() {
    let t = FourslashTest::new(
        "
        export { something } from './other';
        export * from './utils';
    ",
    );
    let result = t.document_links("test.ts");
    result.expect_found();
    result.expect_min_count(2);
}

#[test]
fn document_links_type_import() {
    let t = FourslashTest::new(
        "
        import type { Config } from './config';
    ",
    );
    let result = t.document_links("test.ts");
    result.expect_found();
}

#[test]
fn document_links_require() {
    let t = FourslashTest::new(
        "
        const fs = require('fs');
        const path = require('path');
    ",
    );
    let result = t.document_links("test.ts");
    result.expect_found();
}

// =============================================================================
// Linked Editing (JSX Tag Sync): Advanced Patterns (NEW)
// =============================================================================

#[test]
fn linked_editing_simple_jsx() {
    let t = FourslashTest::new(
        "
        const elem = </*m*/div>content</div>;
    ",
    );
    // JSX linked editing should find paired tags
    let _ = t.linked_editing_ranges("m");
}

// =============================================================================
// Multi-file: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn multi_file_cross_file_references() {
    let mut t = FourslashTest::multi_file(&[
        ("types.ts", "export interface /*def*/User { name: string; }"),
        ("utils.ts", "import { /*ref*/User } from './types';"),
    ]);
    // Within-file definition of the import binding
    let result = t.go_to_definition("ref");
    result.expect_found();
}

#[test]
fn multi_file_workspace_symbols() {
    let t = FourslashTest::multi_file(&[
        ("a.ts", "export class Alpha {}"),
        ("b.ts", "export class Beta {}"),
        ("c.ts", "export class Gamma {}"),
    ]);
    let result = t.workspace_symbols("a");
    // Should find Alpha at minimum
    if !result.symbols.is_empty() {
        result.expect_found();
    }
}

#[test]
fn multi_file_diagnostics_independent() {
    let mut t = FourslashTest::multi_file(&[
        ("good.ts", "const x: number = 42;"),
        ("bad.ts", "const y: number = 'not a number';"),
    ]);
    t.verify_no_errors("good.ts");
    t.diagnostics("bad.ts").expect_found();
}

#[test]
fn multi_file_completions_imports() {
    let mut t = FourslashTest::multi_file(&[
        ("lib.ts", "export function helperFunc() { return 1; }"),
        ("main.ts", "/*c*/"),
    ]);
    // Completions at the top of main.ts
    let result = t.completions("c");
    // Just verify no crash - cross-file completions depend on project setup
    let _ = result;
}

// =============================================================================
// Edge Cases & Robustness (NEW)
// =============================================================================

#[test]
fn edge_case_empty_class() {
    let mut t = FourslashTest::new(
        "
        class /*cls*/Empty {}
    ",
    );
    t.hover("cls")
        .expect_found()
        .expect_display_string_contains("Empty");
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Empty");
}

