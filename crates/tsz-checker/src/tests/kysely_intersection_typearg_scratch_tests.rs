//! Scratch repro for #16025's smallest witness: contextual inference of a
//! generic, argument-less call's type parameter from an intersection member
//! (`TB & string`) against the substituted contextual type.

use tsz_common::options::checker::CheckerOptions;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str) -> Vec<u32> {
    crate::test_utils::check_source(source, "test.ts", opts())
        .iter()
        .map(|diag| diag.code)
        .collect()
}

fn messages(source: &str) -> Vec<String> {
    crate::test_utils::check_source(source, "test.ts", opts())
        .iter()
        .map(|diag| format!("TS{}: {}", diag.code, diag.message_text))
        .collect()
}

#[test]
fn scratch_function_module_shape() {
    let src = r#"
interface FunctionModule<DB, TB extends keyof DB> {
    agg: TB & string;
    plain: TB;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
interface Container<DB> {
    fn: FunctionModule<DB, keyof DB>;
}
class Impl<DB> implements Container<DB> {
    get fn(): FunctionModule<DB, keyof DB> {
        return createFunctionModule();
    }
}
"#;
    let diags = messages(src);
    assert!(diags.is_empty(), "PROBE messages: {diags:#?}");
}
