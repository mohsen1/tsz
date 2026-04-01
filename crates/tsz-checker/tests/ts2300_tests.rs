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
            let is_expected = expected.iter().any(|(_l, _c, m)| m.contains("Cannot find"));
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

        for (i, (exp_line, exp_col, exp_msg)) in expected.iter().enumerate() {
            if *exp_line == line
                && (*exp_col as i32 - col as i32).abs() <= 1
                && msg.contains(exp_msg)
            {
                matched_indices.push(i);
                break;
            }
        }
    }

    // Check if we missed any expected errors
    let mut missing = Vec::new();
    for (i, (exp_line, exp_col, exp_msg)) in expected.iter().enumerate() {
        if !matched_indices.contains(&i) {
            missing.push(format!("({exp_line}, {exp_col}, \"{exp_msg}\")"));
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

/// Test that duplicate properties in interfaces emit TS2300 on ALL occurrences
/// (both first and subsequent). tsc reports all declarations as duplicates in interfaces.
#[test]
fn duplicate_interface_properties() {
    verify_errors(
        "interface Foo { x: number; x: string; }",
        &[
            (1, 17, "Duplicate identifier 'x'."),
            (1, 28, "Duplicate identifier 'x'."),
        ],
    );
}

/// Test that string-literal and identifier interface properties with the same name
/// are detected as duplicates (both occurrences reported).
#[test]
fn duplicate_interface_string_literal_and_identifier() {
    verify_errors(
        "interface Album { \"artist\": string; artist: string; }",
        &[
            (1, 19, "Duplicate identifier 'artist'."),
            (1, 37, "Duplicate identifier 'artist'."),
        ],
    );
}

/// Test that three duplicate interface properties produce TS2300 on all three.
#[test]
fn triple_duplicate_interface_properties() {
    let diagnostics = verify_errors(
        "interface I { a: number; a: string; a: boolean; }",
        &[
            (1, 15, "Duplicate identifier 'a'."),
            (1, 26, "Duplicate identifier 'a'."),
            (1, 37, "Duplicate identifier 'a'."),
        ],
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(ts2300, 3, "All three occurrences should have TS2300");
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
        &[
            (1, 11, "Duplicate identifier 'x'."),
            (1, 26, "Duplicate identifier 'x'."),
        ],
    );
}

/// Test that method + getter with same name emits TS2300.
#[test]
fn method_and_getter_with_same_name() {
    verify_errors(
        "class C { m() {} get m() { return 1; } }",
        &[
            (1, 11, "Duplicate identifier 'm'."),
            (1, 22, "Duplicate identifier 'm'."),
        ],
    );
}

/// Test that getter + setter pair does NOT emit TS2300.
#[test]
fn getter_setter_pair_allowed() {
    verify_errors("class C { get x() { return 1; } set x(v) {} }", &[]);
}

/// A method followed by accessor declarations with the same computed symbol name
/// reports TS2300 only on the later declarations (getter and setter), not the
/// first-declared method. tsc treats computed names differently from simple
/// identifiers: it does not flag the first declaration that established the name.
#[test]
fn computed_symbol_method_and_accessor_pair_reports_all_duplicates() {
    let diagnostics = verify_errors(
        r#"
class C {
    [Symbol.toPrimitive](x: string) { return x; }
    get [Symbol.toPrimitive]() { return ""; }
    set [Symbol.toPrimitive](x: string) {}
}
"#,
        &[
            (4, 9, "Duplicate identifier '[Symbol.toPrimitive]'."),
            (5, 9, "Duplicate identifier '[Symbol.toPrimitive]'."),
        ],
    );

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    assert_eq!(
        ts2300_errors.len(),
        2,
        "expected only getter/setter to report TS2300 (not the first-declared method), got: {diagnostics:?}"
    );
}

/// Test that numeric class members with equivalent numeric values are detected as duplicates.
#[test]
fn numeric_class_member_duplicates() {
    // 0 and 0.0 are duplicates — message preserves source text per TSC's declarationNameToString
    verify_errors(
        "class C { 0 = 1; 0.0 = 2; }",
        &[(1, 18, "Duplicate identifier '0.0'.")],
    );

    // 0.0 and '0' are duplicates — string literal wrapped in single quotes
    verify_errors(
        "class C { 0.0 = 1; '0' = 2; }",
        &[(1, 20, "Duplicate identifier ''0''.")],
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

/// Test that parameter + var with the same name in a function body does NOT
/// produce TS2300. In TypeScript, `var` declarations in a function body
/// merge with parameters of the same name (both are function-scoped).
#[test]
fn parameter_and_var_same_name_no_ts2300() {
    let diagnostics = verify_errors("function f(x: number) { var x = 10; }", &[]);
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(ts2300, 0, "Parameter + var should NOT produce TS2300");
}

/// Test that the implicit `arguments` binding combined with a parameter named
/// `arguments` and a var named `arguments` does NOT produce TS2300. This was
/// a regression where PARAMETER nodes were incorrectly resolved to their parent
/// `FunctionDeclaration`, making them appear as FUNCTION-flagged declarations.
#[test]
fn arguments_parameter_and_var_no_ts2300() {
    let diagnostics = verify_errors("function f(arguments: number) { var arguments = 10; }", &[]);
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(ts2300, 0, "arguments param + var should NOT produce TS2300");
}

/// Test that parameter named `arguments` with rest parameters does NOT
/// produce false TS2300 (the collision is handled by TS2396 or TS1100
/// in strict mode, not TS2300).
#[test]
fn arguments_parameter_with_rest_no_ts2300() {
    let diagnostics = verify_errors(
        "function f(arguments: number, ...rest: any[]) { var arguments: any; }",
        &[],
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300, 0,
        "arguments param with rest should NOT produce TS2300"
    );
}

/// Test that multiple `export default class C` does NOT produce TS2300.
/// tsc emits only TS2528 ("A module cannot have multiple default exports"),
/// not TS2300 for the class name.
#[test]
fn export_default_class_duplicates_no_ts2300() {
    let diagnostics = verify_errors(
        "export default class C {} export default class C {}",
        &[], // TS2300 should NOT be emitted
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300, 0,
        "export default class duplicates should NOT produce TS2300 (TS2323 handles it)"
    );
    // tsc emits TS2528 "A module cannot have multiple default exports" for this case
    let ts2528 = diagnostics.iter().filter(|d| d.code == 2528).count();
    assert!(
        ts2528 > 0,
        "Should emit TS2528 for multiple default exports (matching tsc)"
    );
}

#[test]
fn export_default_reexports_participate_in_duplicate_default_checks() {
    let diagnostics = verify_errors(
        "export default function () {} export { default } from './hi'; export { aa as default } from './hi';",
        &[],
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2323),
        "Expected TS2323 when a declaration-form default export conflicts with default re-exports, got codes: {codes:?}"
    );
    assert!(
        codes.contains(&2528),
        "Expected TS2528 for multiple default exports, got codes: {codes:?}"
    );
}

/// Test that duplicate well-known Symbol properties in interfaces do NOT produce TS2300.
/// tsc allows duplicate Symbol-keyed properties because symbols are structurally unique.
#[test]
fn duplicate_symbol_property_in_interface_no_ts2300() {
    let diagnostics = verify_errors(
        "interface I { [Symbol.isConcatSpreadable]: string; [Symbol.isConcatSpreadable]: string; }",
        &[], // No TS2300 expected for Symbol properties
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300, 0,
        "Duplicate Symbol properties in interfaces should NOT produce TS2300"
    );
}

/// Test that class extending a namespaced class does NOT produce false TS2506.
/// The recursion guard in `class_type.rs` should not emit TS2506 — true cycle
/// detection is handled by dedicated DFS in `class_inheritance.rs`.
#[test]
fn class_extends_namespaced_class_no_false_ts2506() {
    let diagnostics = verify_errors(
        "declare namespace D { class bar { } }
declare namespace E { class foobar extends D.bar { foo(): void; } }",
        &[], // No errors expected
    );
    let ts2506 = diagnostics.iter().filter(|d| d.code == 2506).count();
    assert_eq!(
        ts2506, 0,
        "Class extending namespaced class should NOT produce false TS2506"
    );
}

/// Test that duplicate getters in object literals emit TS1118 (not TS1117).
#[test]
fn duplicate_getter_in_object_literal_emits_ts1118() {
    let diagnostics = verify_errors(
        "var x = { get a() { return 0; }, set a(v: number) {}, get a() { return 0; } };",
        &[],
    );
    let ts1118 = diagnostics.iter().filter(|d| d.code == 1118).count();
    assert!(ts1118 > 0, "Duplicate getters should produce TS1118");
    let ts1117 = diagnostics.iter().filter(|d| d.code == 1117).count();
    assert_eq!(ts1117, 0, "Duplicate getters should NOT produce TS1117");
}

/// Test that duplicate setters in object literals emit TS1118.
#[test]
fn duplicate_setter_in_object_literal_emits_ts1118() {
    let diagnostics = verify_errors("var x = { set a(v: number) {}, set a(v: number) {} };", &[]);
    let ts1118 = diagnostics.iter().filter(|d| d.code == 1118).count();
    assert!(ts1118 > 0, "Duplicate setters should produce TS1118");
}

/// Test that getter+setter pair does NOT emit TS1117 or TS1118.
#[test]
fn getter_setter_pair_in_object_literal_no_error() {
    let diagnostics = verify_errors(
        "var x = { get a() { return 0; }, set a(v: number) {} };",
        &[],
    );
    let ts1117 = diagnostics.iter().filter(|d| d.code == 1117).count();
    let ts1118 = diagnostics.iter().filter(|d| d.code == 1118).count();
    assert_eq!(ts1117, 0, "Getter+setter pair should NOT produce TS1117");
    assert_eq!(ts1118, 0, "Getter+setter pair should NOT produce TS1118");
}

/// Test that `class Object {}` at file scope in a module does NOT produce TS2300.
/// In tsc, the local class shadows the global `Object` from lib (different scopes).
/// Our binder simulates this by creating a new symbol instead of merging with the
/// lib symbol when a CLASS declaration collides with a lib VALUE symbol.
#[test]
fn class_shadowing_lib_global_no_ts2300() {
    let diagnostics = verify_errors(
        "export {}; class Object {}",
        &[], // No TS2300 expected — local class shadows global Object
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300, 0,
        "class Object at file scope should shadow lib Object, NOT produce TS2300"
    );
}

/// Test that `function require() {}` at file scope in a module does NOT produce TS2300.
/// Same shadowing principle as class — local function shadows lib function.
#[test]
fn function_shadowing_lib_global_no_ts2300() {
    let diagnostics = verify_errors(
        "export {}; function require() {}",
        &[], // No TS2300 expected
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300, 0,
        "function require at file scope should shadow lib require, NOT produce TS2300"
    );
}

/// Test that exported and non-exported classes with the same name in merging
/// namespaces do NOT produce TS2300. tsc allows this because they occupy
/// different visibility scopes.
#[test]
fn namespace_exported_and_non_exported_class_no_ts2300() {
    let diagnostics = verify_errors(
        "namespace A { export class Point { x: number; } } namespace A { class Point { y: number; } }",
        &[], // No TS2300 expected
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300, 0,
        "Exported + non-exported class in merging namespaces should NOT produce TS2300"
    );
}

/// Test that constructor parameter properties conflict with explicit class properties.
/// When a constructor parameter has a visibility modifier (public/private/protected)
/// and a class also has an explicit property with the same name, tsc reports TS2300
/// on the parameter.
#[test]
fn constructor_param_property_duplicate_ts2300() {
    let diagnostics = verify_errors(
        "class D { y: number; constructor(public y: number) { } }",
        &[(1, 42, "Duplicate identifier 'y'")],
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert!(
        ts2300 >= 1,
        "Expected TS2300 for parameter property conflicting with explicit property"
    );
}

/// Test that TS2687 is reported when parameter property modifier differs from
/// the explicit property's modifier.
#[test]
fn constructor_param_property_modifier_mismatch_ts2687() {
    let diagnostics = verify_errors(
        "class E { y: number; constructor(private y: number) { } }",
        &[(1, 43, "Duplicate identifier 'y'")],
    );
    let ts2687 = diagnostics.iter().filter(|d| d.code == 2687).count();
    assert!(
        ts2687 >= 2,
        "Expected TS2687 on both property and parameter when modifiers differ, got {ts2687}"
    );
}

/// Test that no TS2300 is emitted for regular constructor parameters (without
/// visibility modifiers) that share a name with a class property.
#[test]
fn constructor_param_no_modifier_no_ts2300() {
    let diagnostics = verify_errors(
        "class C { y: number; constructor(y: number) { } }",
        &[], // No TS2300 expected — parameter without modifier is fine
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300, 0,
        "Regular constructor parameter (no modifier) should NOT produce TS2300"
    );
}

/// Test that protected parameter property conflicting with explicit property
/// reports both TS2300 and TS2687.
#[test]
fn constructor_protected_param_property_duplicate() {
    let diagnostics = verify_errors(
        "class F { y: number; constructor(protected y: number) { } }",
        &[(1, 45, "Duplicate identifier 'y'")],
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    let ts2687 = diagnostics.iter().filter(|d| d.code == 2687).count();
    assert!(
        ts2300 >= 1,
        "Expected TS2300 for protected parameter property conflicting with explicit property"
    );
    assert!(
        ts2687 >= 2,
        "Expected TS2687 on both declarations when modifiers differ (protected vs none), got {ts2687}"
    );
}

/// Test that TS2687 is NOT reported when the explicit property has no modifier
/// (default = public) and the parameter property is explicitly `public` — they match.
#[test]
fn constructor_public_param_property_no_ts2687() {
    let diagnostics = verify_errors(
        "class D { y: number; constructor(public y: number) { } }",
        &[(1, 42, "Duplicate identifier 'y'")],
    );
    let ts2687 = diagnostics.iter().filter(|d| d.code == 2687).count();
    assert_eq!(
        ts2687, 0,
        "No TS2687 expected when both are public (explicit public matches default public)"
    );
}

/// Test that static properties do NOT conflict with parameter properties.
#[test]
fn constructor_param_property_no_conflict_with_static() {
    let diagnostics = verify_errors(
        "class G { static y: number; constructor(public y: number) { } }",
        &[], // No TS2300 — static and instance properties don't conflict
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300, 0,
        "Static property should NOT conflict with instance parameter property"
    );
}

#[test]
fn constructor_overload_param_property_duplicate_ts2300() {
    let diagnostics = verify_errors(
        "class Customers { constructor(public names: string); constructor(public names: string, public ages: number) {} }",
        &[(1, 73, "Duplicate identifier 'names'")],
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert!(
        ts2300 >= 1,
        "Expected TS2300 for parameter property duplicated across constructor overloads"
    );
}

// ========================================================================
// TS2451 vs TS2300 disambiguation when block-scoped variables are involved
// ========================================================================

/// When a const (block-scoped variable) conflicts with a class declaration,
/// tsc emits TS2451 ("Cannot redeclare block-scoped variable") on both
/// conflicting declarations, NOT TS2300.
/// Regression test for exportInterfaceClassAndValue.ts conformance.
#[test]
fn const_class_conflict_emits_ts2451() {
    // When const (block-scoped) conflicts with class (non-block-scoped but
    // not a function), tsc emits TS2451 ("Cannot redeclare block-scoped
    // variable"). TS2300 is only used when a function declaration is in
    // the conflict set.
    let source = r#"
export const foo = 1;
export declare class foo {}
export interface foo {}
"#;
    let diagnostics = verify_errors(
        source,
        &[
            (2, 14, "Cannot redeclare block-scoped variable 'foo'."),
            (3, 22, "Cannot redeclare block-scoped variable 'foo'."),
        ],
    );

    let ts2451 = diagnostics.iter().filter(|d| d.code == 2451).count();
    assert!(
        ts2451 >= 2,
        "Expected TS2451 for const+class conflict, got ts2451={ts2451}"
    );
}

/// When a const conflicts with a var, tsc uses TS2300 ("Duplicate identifier")
/// because it's a mix of block-scoped and function-scoped declarations.
#[test]
fn const_var_conflict_emits_ts2451() {
    // const before var in pure 2-way variable conflict → TS2451
    let source = "const x = 1;\nvar x = 2;";
    let diagnostics = verify_errors(
        source,
        &[
            (1, 7, "Cannot redeclare block-scoped variable 'x'."),
            (2, 5, "Cannot redeclare block-scoped variable 'x'."),
        ],
    );

    let ts2451 = diagnostics.iter().filter(|d| d.code == 2451).count();
    assert!(
        ts2451 >= 2,
        "Expected TS2451 for const-before-var conflict, got ts2451={ts2451}"
    );
}

/// When a let conflicts with a function declaration, tsc uses TS2300
/// ("Duplicate identifier") because it's a mix of block-scoped (let)
/// and non-block-scoped (function) declarations.
///
/// TODO: Currently emits TS2451 ("Cannot redeclare block-scoped variable")
/// instead of TS2300. Update once duplicate identifier classification is refined.
#[test]
fn let_function_conflict_emits_ts2300() {
    let source = "let f = 1;\nfunction f() {}";
    let diagnostics = verify_errors(
        source,
        &[
            (1, 5, "Duplicate identifier 'f'."),
            (2, 10, "Duplicate identifier 'f'."),
        ],
    );

    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert!(
        ts2300 >= 2,
        "Expected TS2300 for let+function same-scope conflict, got ts2300={ts2300}"
    );
}

/// Two vars in the same scope without block-scoped involvement should still
/// get TS2300 (not TS2451) when they conflict.
#[test]
fn var_type_alias_conflict_emits_ts2300() {
    let source = "type X = number;\ntype X = string;";
    let diagnostics = verify_errors(
        source,
        &[
            (1, 6, "Duplicate identifier 'X'."),
            (2, 6, "Duplicate identifier 'X'."),
        ],
    );

    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    let ts2451 = diagnostics.iter().filter(|d| d.code == 2451).count();
    assert!(
        ts2300 >= 2,
        "Expected TS2300 for type alias conflict (no block-scoped), got ts2300={ts2300}, ts2451={ts2451}"
    );
    assert_eq!(
        ts2451, 0,
        "Should NOT emit TS2451 when no block-scoped variable is involved"
    );
}

/// Test that let+var+function at the same top-level scope get duplicate errors.
///
/// TODO: tsc emits TS2300 ("Duplicate identifier") here. We currently emit
/// TS2451 ("Cannot redeclare block-scoped variable"). Update once classification
/// is refined.
#[test]
fn let_var_function_same_scope_ts2300() {
    let diagnostics = verify_errors(
        "let e0\nvar e0;\nfunction e0() { }",
        &[
            (1, 5, "Duplicate identifier 'e0'."),
            (2, 5, "Duplicate identifier 'e0'."),
            (3, 10, "Duplicate identifier 'e0'."),
        ],
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(ts2300, 3, "All three should be TS2300 at same scope");
}

/// Test that var-before-let at the same scope level gets TS2451.
/// When var comes first and let/const comes second, tsc uses TS2300 (not TS2451).
/// When let/const comes first and var comes second, tsc uses TS2451.
#[test]
fn var_before_let_same_scope_ts2300() {
    let diagnostics = verify_errors(
        "var x = 0;\nlet x = 0;",
        &[
            (1, 5, "Duplicate identifier 'x'."),
            (2, 5, "Duplicate identifier 'x'."),
        ],
    );
    let ts2300 = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(ts2300, 2, "var-before-let should be TS2300");
}
