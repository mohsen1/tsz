use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check_codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn await_using_directly_in_static_block_emits_ts18054() {
    let codes = check_codes(
        r#"
class C {
    static {
        await using resource = getResource();
    }
}
declare function getResource(): { [Symbol.dispose](): void };
"#,
    );

    assert!(
        codes.contains(&18054),
        "expected TS18054 for await using directly inside a class static block, got {codes:?}"
    );
    assert!(
        !codes.contains(&2852),
        "TS2852 should not accompany TS18054 for the same statement, got {codes:?}"
    );
}

#[test]
fn await_using_in_static_block_under_outer_async_function_still_emits_ts18054() {
    // Oracle-confirmed (typescript@7.0.2): a static block is never itself async,
    // so an enclosing async function does NOT suppress TS18054. This is the
    // regression case: `enclosing_function_allows_await_using` alone would
    // incorrectly find the outer async function and report nothing.
    let codes = check_codes(
        r#"
async function outer() {
    class C {
        static {
            await using resource = getResource();
        }
    }
}
declare function getResource(): { [Symbol.dispose](): void };
"#,
    );

    assert!(
        codes.contains(&18054),
        "expected TS18054 even under an outer async function, got {codes:?}"
    );
}

#[test]
fn await_using_in_nested_async_function_inside_static_block_does_not_emit_ts18054() {
    let codes = check_codes(
        r#"
class C {
    static {
        async function inner() {
            await using resource = getResource();
        }
    }
}
declare function getResource(): { [Symbol.dispose](): void };
"#,
    );

    assert!(
        !codes.contains(&18054),
        "did not expect TS18054 for await using inside a nested async function within a static block, got {codes:?}"
    );
    assert!(
        !codes.contains(&2852),
        "did not expect TS2852 either — the nested function is async, got {codes:?}"
    );
}

#[test]
fn await_using_in_nested_non_async_function_inside_static_block_emits_ts2852_not_ts18054() {
    // The static-block detector stops at the first function boundary, so a
    // nested *non*-async function routes to the ordinary TS2852 check instead
    // of TS18054 — the diagnostic families are mutually exclusive per node.
    let codes = check_codes(
        r#"
class C {
    static {
        function inner() {
            await using resource = getResource();
        }
    }
}
declare function getResource(): { [Symbol.dispose](): void };
"#,
    );

    assert!(
        codes.contains(&2852),
        "expected TS2852 for await using inside a nested non-async function, got {codes:?}"
    );
    assert!(
        !codes.contains(&18054),
        "did not expect TS18054 once a function boundary intervenes, got {codes:?}"
    );
}

#[test]
fn plain_using_in_static_block_does_not_emit_ts18054() {
    // TS18054 is specific to `await using`; a plain `using` declaration never
    // requires an async context, so it is unaffected by this check.
    let codes = check_codes(
        r#"
class C {
    static {
        using resource = getResource();
    }
}
declare function getResource(): { [Symbol.dispose](): void };
"#,
    );

    assert!(
        !codes.contains(&18054),
        "did not expect TS18054 for a plain (non-await) using inside a static block, got {codes:?}"
    );
}
