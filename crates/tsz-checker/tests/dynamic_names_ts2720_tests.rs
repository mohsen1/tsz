use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

#[test]
fn class_implements_public_dynamic_name_class_shape_no_ts2720() {
    let source = r#"
const c0 = "a";
const c1 = 1;
const s0 = Symbol();

declare class T1 {
    [c0]: number;
    [c1]: string;
    [s0]: boolean;
}
declare class T2 extends T1 {
}

const c4 = "a";
const c5 = 1;
const s2: typeof s0 = s0;

declare class T13 implements T2 {
    a: number;
    1: string;
    [s2]: boolean;
}
"#;
    let libs = load_lib_files(&["es2015.d.ts"]);
    let diagnostics = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2720),
        "Expected no TS2720 for public dynamic-name class shape, got: {diagnostics:#?}",
    );
}

#[test]
fn class_implements_property_type_mismatch_reports_member_ts2416() {
    let source = r#"
interface FileSystem {
    read: number;
}

class WorkerFS implements FileSystem {
    read: string;
}
"#;
    let libs = load_lib_files(&["es5.d.ts", "es2015.d.ts", "dom.d.ts"]);
    let diagnostics = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();
    assert!(
        codes.contains(&2416),
        "Expected TS2416 for incompatible implemented property, got: {diagnostics:#?}",
    );
    assert!(
        !codes.contains(&2420),
        "Expected member-level TS2416 without class-level TS2420, got: {diagnostics:#?}",
    );
}
