#[test]
fn test_umd_export_vs_declare_global_const_emits_ts2451() {
    // `export as namespace React` in module.d.ts creates a UMD global binding.
    // `declare global { const React }` in global.d.ts creates a global const.
    // tsc expects TS2451 on both declarations.
    let files = vec![
        (
            "module.d.ts".to_string(),
            "export as namespace React;\nexport function foo(): string;\n".to_string(),
        ),
        (
            "global.d.ts".to_string(),
            "declare global {\n    const React: typeof import(\"./module\");\n}\nexport {};\n"
                .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::ESNext,
            target: tsz_common::common::ScriptTarget::ES2018,
            strict: true,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let module_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "module.d.ts")
        .expect("expected module.d.ts result");
    let global_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "global.d.ts")
        .expect("expected global.d.ts result");

    let module_ts2451 = module_file
        .diagnostics
        .iter()
        .filter(|d| d.code == 2451)
        .count();
    let global_ts2451 = global_file
        .diagnostics
        .iter()
        .filter(|d| d.code == 2451)
        .count();

    assert!(
        module_ts2451 > 0,
        "Expected TS2451 in module.d.ts for UMD export conflicting with declare global const. Diagnostics: {:#?}",
        module_file.diagnostics
    );
    assert!(
        global_ts2451 > 0,
        "Expected TS2451 in global.d.ts for declare global const conflicting with UMD export. Diagnostics: {:#?}",
        global_file.diagnostics
    );
}

// TODO: Implement TS2300 duplicate identifier detection for global augmentation conflicts.
#[test]
#[ignore]
fn test_check_files_parallel_global_augmentation_member_conflicts_emit_ts2300() {
    let files = vec![
        (
            "file1.ts".to_string(),
            r#"
declare global {
    interface TopLevel {
        duplicate1: () => string;
        duplicate2: () => string;
        duplicate3: () => string;
    }
}
export {}
"#
            .to_string(),
        ),
        (
            "file2.ts".to_string(),
            r#"
import "./file1";
declare global {
    interface TopLevel {
        duplicate1(): number;
        duplicate2(): number;
        duplicate3(): number;
    }
}
export {}
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file1 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file1.ts")
        .expect("expected file1.ts result");
    let file2 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file2.ts")
        .expect("expected file2.ts result");

    let file1_codes: Vec<u32> = file1
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300 || diag.code == 6200)
        .map(|diag| diag.code)
        .collect();
    let file2_codes: Vec<u32> = file2
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300 || diag.code == 6200)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        file1_codes,
        vec![2300, 2300, 2300],
        "Expected file1.ts to report per-member TS2300 diagnostics for global augmentation conflicts. Diagnostics: {:#?}",
        file1.diagnostics
    );
    assert_eq!(
        file2_codes,
        vec![2300, 2300, 2300],
        "Expected file2.ts to report per-member TS2300 diagnostics for global augmentation conflicts. Diagnostics: {:#?}",
        file2.diagnostics
    );
}

#[test]
fn test_check_files_parallel_module_augmentation_member_conflicts_aggregate_to_ts6200() {
    let files = vec![
        (
            "file1.ts".to_string(),
            r#"
declare module "someMod" {
    export interface TopLevel {
        duplicate1: () => string;
        duplicate2: () => string;
        duplicate3: () => string;
        duplicate4: () => string;
        duplicate5: () => string;
        duplicate6: () => string;
        duplicate7: () => string;
        duplicate8: () => string;
        duplicate9: () => string;
    }
}
"#
            .to_string(),
        ),
        (
            "file2.ts".to_string(),
            r#"
/// <reference path="./file1" />

declare module "someMod" {
    export interface TopLevel {
        duplicate1(): number;
        duplicate2(): number;
        duplicate3(): number;
        duplicate4(): number;
        duplicate5(): number;
        duplicate6(): number;
        duplicate7(): number;
        duplicate8(): number;
        duplicate9(): number;
    }
}
export {};
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file1 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file1.ts")
        .expect("expected file1.ts result");
    let file2 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file2.ts")
        .expect("expected file2.ts result");

    let file1_codes: Vec<u32> = file1
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300 || diag.code == 6200)
        .map(|diag| diag.code)
        .collect();
    let file2_codes: Vec<u32> = file2
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2300 || diag.code == 6200)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        file1_codes,
        vec![6200],
        "Expected file1.ts to aggregate large module augmentation conflicts into TS6200. Diagnostics: {:#?}",
        file1.diagnostics
    );
    assert_eq!(
        file2_codes,
        vec![6200],
        "Expected file2.ts to aggregate large module augmentation conflicts into TS6200. Diagnostics: {:#?}",
        file2.diagnostics
    );
}

#[test]
fn test_check_files_parallel_cross_file_enum_conflicts_emit_ts2567() {
    let files = vec![
        (
            "file1.ts".to_string(),
            r#"
enum D {
    bar
}
class E {}
"#
            .to_string(),
        ),
        (
            "file2.ts".to_string(),
            r#"
function D() {
    return 0;
}
enum E {
    bar
}
"#
            .to_string(),
        ),
    ];

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

    let file1 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file1.ts")
        .expect("expected file1.ts result");
    let file2 = result
        .file_results
        .iter()
        .find(|file| file.file_name == "file2.ts")
        .expect("expected file2.ts result");

    let file1_codes: Vec<u32> = file1
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();
    let file2_codes: Vec<u32> = file2
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        file1_codes,
        vec![2567, 2567],
        "Expected file1.ts to report TS2567 for cross-file enum conflicts. Diagnostics: {:#?}",
        file1.diagnostics
    );
    assert_eq!(
        file2_codes,
        vec![2567, 2567],
        "Expected file2.ts to report TS2567 for cross-file enum conflicts. Diagnostics: {:#?}",
        file2.diagnostics
    );
}

// TODO: Implement TS2567 detection for re-exported class/enum merge conflicts.
#[test]
#[ignore]
fn test_check_files_parallel_module_augmentation_reexported_enum_class_merge_emits_ts2567() {
    let files = vec![
        (
            "file.ts".to_string(),
            r#"
export class Foo {
    member: string;
}
"#
            .to_string(),
        ),
        (
            "reexport.ts".to_string(),
            r#"
export * from "./file";
"#
            .to_string(),
        ),
        (
            "augment.ts".to_string(),
            r#"
import * as ns from "./reexport";

declare module "./reexport" {
    export enum Foo {
        A, B, C
    }
}

declare const f: ns.Foo;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|entry| entry.file_name == "file.ts")
        .expect("expected file.ts result");
    let augment = result
        .file_results
        .iter()
        .find(|entry| entry.file_name == "augment.ts")
        .expect("expected augment.ts result");
    let reexport = result
        .file_results
        .iter()
        .find(|entry| entry.file_name == "reexport.ts")
        .expect("expected reexport.ts result");

    let file_codes: Vec<u32> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();
    let augment_codes: Vec<u32> = augment
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();
    let reexport_codes: Vec<u32> = reexport
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2567)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        file_codes,
        vec![2567],
        "Expected file.ts to report TS2567 for a re-exported class/enum merge conflict. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert_eq!(
        augment_codes,
        vec![2567],
        "Expected augment.ts to report TS2567 for a module augmentation enum/class merge conflict. Diagnostics: {:#?}",
        augment.diagnostics
    );
    assert!(
        reexport_codes.is_empty(),
        "Did not expect TS2567 in reexport.ts. Diagnostics: {:#?}",
        reexport.diagnostics
    );
}

#[test]
fn test_check_files_parallel_var_and_duplicate_functions_keep_ts2300() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
var foo: string;
function foo(): number { }
function foo(): number { }
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
    let ts2393_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2393)
        .count();
    let ts2355_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2355)
        .count();

    assert_eq!(
        ts2300_count, 3,
        "Expected TS2300 on the var and both function declarations. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert_eq!(
        ts2393_count, 2,
        "Expected TS2393 on both function implementations. Diagnostics: {:#?}",
        file.diagnostics
    );
    assert_eq!(
        ts2355_count, 2,
        "Expected TS2355 on both function implementations. Diagnostics: {:#?}",
        file.diagnostics
    );
}

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
#[ignore] // TODO: Private name static/instance conflicts TS2804 needs parallel tracking
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
#[ignore] // TODO: Private accessor before field declarations reporting needs parallel handling
fn test_check_files_parallel_private_accessor_before_field_reports_both_declarations() {
    // tsc reports TS2300 on BOTH declarations when a private accessor and
    // private field share the same name, so we expect 6 total (2 per class).
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
        ts2300_count, 6,
        "Expected TS2300 on both accessor and field declarations (2 per class × 3 classes). Diagnostics: {:#?}",
        file.diagnostics
    );
    assert!(
        file.diagnostics.iter().all(|diag| diag.code != 2804),
        "Did not expect TS2804 for same-staticness private accessor/field conflicts. Diagnostics: {:#?}",
        file.diagnostics
    );
}

