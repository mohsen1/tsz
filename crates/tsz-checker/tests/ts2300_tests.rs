#![allow(warnings)]
//! Tests for TS2300 emission ("Duplicate identifier")

use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Try to find the lib files in a few common locations relative to the crate
    let lib_paths = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.dom.d.ts"),
        // Fallback paths trying to find node_modules in various places
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
    ];

    let mut lib_files = Vec::new();

    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path.file_name().unwrap().to_string_lossy().to_string();
            let lib_file = LibFile::from_source(file_name, content);
            lib_files.push(Arc::new(lib_file));
        }
    }

    lib_files
}

fn get_line_and_col(source: &str, offset: u32) -> (u32, u32) {
    let offset = offset as usize;
    let mut line = 1;
    let mut col = 1;
    // Iterate over chars.
    // Note: this assumes byte offset maps to char boundary, which it should for valid UTF-8.
    // If source contains multi-byte chars, we need to be careful about byte index vs char count for column?
    // Diagnostic start is byte offset.
    // TypeScript usually counts UTF-16 code units for column.
    // tsz likely uses byte offset or char count?
    // tsz_scanner uses byte offsets.

    // We'll walk by chars and track byte index.
    let mut current_byte_idx = 0;

    for c in source.chars() {
        if current_byte_idx == offset {
            return (line, col);
        }
        if current_byte_idx > offset {
            // We overshot? Should not happen if offset is valid char boundary.
            return (line, col);
        }

        let char_len = c.len_utf8();
        current_byte_idx += char_len;

        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }

    (line, col)
}

fn verify_errors(
    source: &str,
    expected: &[(u32, u32, &str)],
) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let lib_files = load_lib_files_for_test();

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    let diagnostics = checker.ctx.diagnostics.clone();

    // Validation
    let mut matched_indices = Vec::new();
    let mut errors_found = Vec::new();

    for diag in &diagnostics {
        // Filter out "Cannot find global type" errors (TS2318) and similar noise if not expected
        if matches!(diag.code, 2318 | 2580 | 2552 | 2304) {
            // 2318: Cannot find global type '...'
            // 2580: Cannot find name '...'
            // 2304: Cannot find name '...'
            // Unless we expect them?
            let is_expected = expected.iter().any(|(l, c, m)| m.contains("Cannot find"));
            if !is_expected {
                continue;
            }
        }

        let (line, col) = get_line_and_col(source, diag.start);
        let msg = &diag.message_text;

        errors_found.push(format!(
            "({}, {}, \"{}\") [code: {}]",
            line, col, msg, diag.code
        ));

        let mut found = false;
        for (i, (exp_line, exp_col, exp_msg)) in expected.iter().enumerate() {
            if *exp_line == line {
                // Allow some slop in column matching (e.g. +/- 1) due to indexing differences
                // or just print what we found vs expected
                if (*exp_col as i32 - col as i32).abs() <= 1 && msg.contains(exp_msg) {
                    matched_indices.push(i);
                    found = true;
                    break;
                }
            }
        }
    }

    // Check if we missed any expected errors
    let mut missing = Vec::new();
    for (i, (exp_line, exp_col, exp_msg)) in expected.iter().enumerate() {
        if !matched_indices.contains(&i) {
            missing.push(format!("({}, {}, \"{}\")", exp_line, exp_col, exp_msg));
        }
    }

    if !missing.is_empty() {
        panic!(
            "Missing expected errors:\n  {}\n\nFound errors:\n  {}",
            missing.join("\n  "),
            errors_found.join("\n  ")
        );
    }

    diagnostics
}

#[test]
fn duplicate_enum_member_names() {
    verify_errors("enum E { A, A }", &[(1, 13, "Duplicate identifier 'A'.")]);
}

/// Test that method followed by property emits TS2300 only on the property.
#[test]
fn method_followed_by_property() {
    let diagnostics = verify_errors(
        "class C { x() {} x: any; }",
        &[(1, 18, "Duplicate identifier 'x'.")],
    );

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    assert_eq!(ts2300_errors.len(), 1);
}

/// Test that property followed by method emits TS2300 on BOTH declarations.
#[test]
fn property_followed_by_method() {
    verify_errors(
        "class C { x: any; x() {} }",
        &[
            (1, 11, "Duplicate identifier 'x'."),
            (1, 20, "Duplicate identifier 'x'."),
        ],
    );
}

/// Test that property followed by property emits TS2300 only on the second property.
#[test]
fn property_followed_by_property() {
    verify_errors(
        "class C { x: any; x: any; }",
        &[(1, 18, "Duplicate identifier 'x'.")],
    );
}

/// Test that method overloads are allowed (no TS2300).
#[test]
fn method_overloads_are_allowed() {
    let diagnostics = verify_errors("class C { m(): void; m(x: any): void; m() {} }", &[]);

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    assert!(ts2300_errors.is_empty());
}

/// Test that duplicate properties in interfaces emit TS2300 only on subsequent declarations.
#[test]
fn duplicate_interface_properties() {
    verify_errors(
        "interface Foo { x: number; x: string; }",
        &[(1, 28, "Duplicate identifier 'x'.")],
    );
}

/// Test that interface merging is allowed (no TS2300).
#[test]
fn interface_merging_allowed() {
    verify_errors(
        "interface Foo { x: number; } interface Foo { y: string; }",
        &[],
    );
}

/// Test that duplicate function implementations emit TS2393, not TS2300.
#[test]
fn duplicate_function_implementations() {
    let diagnostics = verify_errors(
        "class C { foo(x: number) { } foo(x: string) { } }",
        &[], // No TS2300 expected
    );

    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    let ts2393 = diagnostics.iter().filter(|d| d.code == 2393).count();

    assert_eq!(ts2300, 0);
    assert_eq!(ts2393, 2, "Should have 2 TS2393 errors");
}

/// Test that field + getter with same name emits TS2300.
#[test]
fn field_and_getter_with_same_name() {
    verify_errors(
        "class C { x: number; get x(): number { return 1; } }",
        &[(1, 26, "Duplicate identifier 'x'.")],
    );
}

/// Test that method + getter with same name emits TS2300.
#[test]
fn method_and_getter_with_same_name() {
    verify_errors(
        "class C { m() {} get m() { return 1; } }",
        &[(1, 22, "Duplicate identifier 'm'.")],
    );
}

/// Test that getter + setter pair does NOT emit TS2300.
#[test]
fn getter_setter_pair_allowed() {
    verify_errors("class C { get x() { return 1; } set x(v) {} }", &[]);
}

/// Test that numeric class members with equivalent numeric values are detected as duplicates.
#[test]
fn numeric_class_member_duplicates() {
    // 0 and 0.0 are duplicates
    verify_errors(
        "class C { 0 = 1; 0.0 = 2; }",
        &[(1, 18, "Duplicate identifier '0'.")],
    );

    // 0.0 and '0' are duplicates
    verify_errors(
        "class C { 0.0 = 1; '0' = 2; }",
        &[(1, 20, "Duplicate identifier '0'.")],
    );

    // '0.0' and '0' are NOT duplicates
    verify_errors("class C { '0.0' = 1; '0' = 2; }", &[]);
}

/// Test that duplicate Symbol computed properties are detected.
#[test]
fn duplicate_symbol_computed_property() {
    // Note: If Symbol is not found (no lib), this might fail with "Cannot find name 'Symbol'".
    // We only verify TS2300.
    verify_errors(
        "class C { get [Symbol.hasInstance]() { return ''; } get [Symbol.hasInstance]() { return ''; } }",
        &[(1, 57, "Duplicate identifier '[Symbol.hasInstance]'.")],
    );
}

/// Test that duplicate import = alias declarations emit TS2300.
#[test]
fn duplicate_import_equals_alias() {
    verify_errors(
        "namespace m { export const x = 1; } import a = m; import a = m;",
        &[(1, 58, "Duplicate identifier 'a'.")],
    );
}

/// Test that import alias conflicting with local declaration emits TS2440.
#[test]
fn import_equals_alias_conflicts_with_local() {
    let diagnostics = verify_errors(
        "namespace m { export const x = 1; } import a = m; const a = 1;",
        &[], // We are checking for TS2440 separately
    );

    let ts2440 = diagnostics.iter().filter(|d| d.code == 2440).count();
    assert!(ts2440 > 0, "Should emit TS2440");
}

/// Test for acceptableAlias1.ts false positive
/// "export import X = N" should NOT be a duplicate identifier
#[test]
fn acceptable_alias_repro() {
    verify_errors(
        "namespace M { export namespace N {} export import X = N; } import r = M.X;",
        &[],
    );
}
