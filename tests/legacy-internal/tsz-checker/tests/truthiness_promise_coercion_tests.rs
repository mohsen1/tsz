use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn codes_with_libs(src: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        src,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn promise_truthiness_coercion_emits_awaitable_diagnostics() {
    let c = codes_with_libs(
        r#"
declare const p: Promise<number>;
declare const p2: null | Promise<number>;
declare const obj: { p: Promise<unknown> };
declare function pf(): Promise<boolean>;

async function f() {
    if (p) {}
    if (!!p) {}
    if (p2) {}
    p ? f.arguments : f.arguments;
    !!p ? f.arguments : f.arguments;
    p2 ? f.arguments : f.arguments;
}

async function g() {
    if (p) {
        p;
    }
    if (p && p.then.length) {}
}

async function h() {
    if (obj.p) {}
    if (obj.p) {
        await obj.p;
    }
    if (obj.p && await obj.p) {}
}

async function i(): Promise<string> {
    if (pf()) {
        return "true";
    }
    if (pf()) {
        pf().then();
    }
    return "false";
}
"#,
    );

    let ts2801 = c.iter().filter(|&&code| code == 2801).count();
    assert_eq!(
        ts2801, 5,
        "expected the five tsc awaitable-truthiness diagnostics from truthinessPromiseCoercion, got {c:?}"
    );
}
