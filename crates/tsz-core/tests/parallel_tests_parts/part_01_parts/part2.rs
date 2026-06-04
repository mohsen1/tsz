#[test]
fn test_check_files_parallel_class_property_after_method_emits_ts2717() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
class C {
    a(): number { return 0; }
    a: number;
}
class K {
    b: number;
    b(): number { return 0; }
}
class D {
    c: number;
    c: string;
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");

    let ts2717_messages: Vec<&str> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2717)
        .map(|diag| diag.message_text.as_str())
        .collect();

    assert_eq!(
        ts2717_messages.len(),
        2,
        "Expected TS2717 for 'a' and 'c' only. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        ts2717_messages
            .iter()
            .any(|msg| msg.contains("Property 'a' must be of type '() => number'")),
        "Expected method-vs-property TS2717 for 'a'. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        ts2717_messages
            .iter()
            .any(|msg| msg.contains("Property 'c' must be of type 'number'")),
        "Expected property-vs-property TS2717 for 'c'. Diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_private_name_static_instance_conflicts_emit_ts2804() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
class A {
    #foo = "foo";
    static #foo() { }
}
class B {
    static get #bar() { return ""; }
    set #bar(value: string) { }
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");

    let ts2804_messages: Vec<&str> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2804)
        .map(|diag| diag.message_text.as_str())
        .collect();

    assert_eq!(
        ts2804_messages.len(),
        2,
        "Expected TS2804 on the later static/instance private-name conflicts only. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        ts2804_messages
            .iter()
            .all(|msg| msg
                .contains("Static and instance elements cannot share the same private name")),
        "Expected TS2804 static/instance private-name message. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        file.diagnostics.iter().all(|diag| diag.code != 2300),
        "Did not expect TS2300 for pure static/instance private-name conflicts. Diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_duplicate_private_accessors_report_all_occurrences() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
class A {
    get #foo() { return ""; }
    get #foo() { return ""; }
}
class B {
    static set #bar(value: string) { }
    static set #bar(value: string) { }
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");

    let ts2300_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300)
        .count();

    assert_eq!(
        ts2300_count, 4,
        "Expected TS2300 on both private getter declarations and both private setter declarations. Diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_private_accessor_before_field_reports_both_declarations() {
    // tsc reports TS2300 on the later field declaration when a private accessor
    // already established the same name.
    let source = r#"
function cases() {
    class A {
        get #foo() { return ""; }
        #foo = "foo";
    }
    class B {
        set #foo(value: string) { }
        #foo = "foo";
    }
    class C {
        static set #foo(value: string) { }
        static #foo = "foo";
    }
}
"#;
    let files = vec![("test.ts".to_string(), source.to_string())];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");

    let ts2300_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300)
        .count();

    assert_eq!(
        ts2300_count, 3,
        "Expected TS2300 on the later private field declarations. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        file.diagnostics.iter().all(|diag| diag.code != 2804),
        "Did not expect TS2804 for same-staticness private accessor/field conflicts. Diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_compile_large_program() {
    // Simulate a larger program with many files
    let files: Vec<_> = (0..50)
        .map(|i| {
            let source = format!("function fn{i}() {{ return {i}; }} const val{i} = fn{i}();");
            (format!("module{i}.ts"), source)
        })
        .collect();

    let program = compile_files(files);

    assert_eq!(program.files.len(), 50);
    // Should have at least 100 symbols (2 per file: fn + val)
    assert!(
        program.symbols.len() >= 100,
        "Expected at least 100 symbols, got {}",
        program.symbols.len()
    );

    // All function and value names should be in globals
    for i in 0..50 {
        let fn_name = format!("fn{i}");
        let val_name = format!("val{i}");
        assert!(program.globals.has(&fn_name), "Missing {fn_name}");
        assert!(program.globals.has(&val_name), "Missing {val_name}");
    }
}

#[test]
fn test_compile_with_exports() {
    // Test that export function/class/const are properly bound — and stay
    // module-scoped. Per tsc, the top-level exports of an external module are
    // NOT ambient globals: an unqualified reference to `add`/`Calculator`/`PI`
    // from a sibling file is a `TS2304` unless imported. They must therefore
    // never leak into `program.globals` (the same fallback that an unimported
    // package's `Symbol` export wrongly polluted in #12372), and remain
    // reachable only through their module's export table.
    let files = vec![
        (
            "a.ts".to_string(),
            "export function add(x: number, y: number) { return x + y; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export class Calculator { add(x: number, y: number) { return x + y; } }".to_string(),
        ),
        ("c.ts".to_string(), "export const PI = 3.14159;".to_string()),
    ];

    let program = compile_files(files);

    assert_eq!(program.files.len(), 3);
    // External-module exports must NOT seed the ambient global scope.
    for name in ["add", "Calculator", "PI"] {
        assert!(
            !program.globals.has(name),
            "external-module export '{name}' must not leak into program globals"
        );
    }
    // They remain reachable through their module's export table.
    let exported_somewhere =
        |name: &str| program.module_exports.values().any(|table| table.has(name));
    assert!(
        exported_somewhere("add"),
        "Exported function 'add' should be reachable as a module export"
    );
    assert!(
        exported_somewhere("Calculator"),
        "Exported class 'Calculator' should be reachable as a module export"
    );
    assert!(
        exported_somewhere("PI"),
        "Exported const 'PI' should be reachable as a module export"
    );
}
