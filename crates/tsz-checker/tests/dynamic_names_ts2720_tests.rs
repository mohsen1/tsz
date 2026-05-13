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
