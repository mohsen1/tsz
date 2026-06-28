use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn diagnostic_codes(source: &str, no_implicit_returns: bool) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_returns,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn terminal_value_return_suppresses_no_implicit_returns() {
    let codes = diagnostic_codes(
        r#"
function renamedTerminal(flag: boolean) {
    if (flag) {
        return 1;
    }
    return 2;
}
"#,
        true,
    );

    assert!(
        !codes.contains(&7030),
        "terminal value return should prove the function does not fall through; got {codes:?}"
    );
}

#[test]
fn nonterminal_partial_return_still_reports_no_implicit_returns() {
    let codes = diagnostic_codes(
        r#"
function renamedPartial(flag: boolean) {
    if (flag) {
        return 1;
    }
    flag;
}
"#,
        true,
    );

    assert!(
        codes.contains(&7030),
        "partial return without a terminal return/throw must still run fallthrough analysis; got {codes:?}"
    );
}

#[test]
fn terminal_throw_satisfies_declared_number_return_completeness() {
    let codes = diagnostic_codes(
        r#"
function renamedThrow(): number {
    throw 1;
}
"#,
        false,
    );

    assert!(
        !codes.iter().any(|code| matches!(code, 2355 | 2366)),
        "terminal throw should not emit return-completeness diagnostics; got {codes:?}"
    );
}

#[test]
fn terminal_bare_return_preserves_unknown_return_completeness() {
    let codes = diagnostic_codes(
        r#"
function renamedUnknown(flag: boolean): unknown {
    if (flag) {
        return 1;
    }
    return;
}
"#,
        false,
    );

    assert!(
        !codes.contains(&2355),
        "terminal bare return should not look like an empty falling-through unknown body; got {codes:?}"
    );
}

// Regression for #14741: a no-return body whose endpoint is unreachable via a
// tail call to a `never`-returning function must infer `never`, not `void`, in
// the contextually-typed / inferred path. Previously the throw-only syntax
// pre-scan short-circuited the reachability check, so these bodies inferred
// `void` and produced a spurious TS2322 against the contextual signature. All
// binder names are renamed so no fixture-name fast path can satisfy the case.

#[test]
fn tail_never_call_in_contextual_method_shorthand_infers_never() {
    let codes = diagnostic_codes(
        r#"
declare function bailOut(code: number): never

interface RenamedHandler {
    defineProperty: (target: object, key: string | symbol, desc: PropertyDescriptor) => boolean
}

const renamedHandler: RenamedHandler = {
    defineProperty() {
        bailOut(11)
    }
}
"#,
        false,
    );

    assert!(
        !codes.contains(&2322),
        "tail never-call in a contextually-typed method must infer never, not void; got {codes:?}"
    );
}

#[test]
fn tail_never_call_in_contextual_arrow_infers_never() {
    let codes = diagnostic_codes(
        r#"
declare function abortNow(code: number): never
const renamedArrow: () => boolean = () => { abortNow(1) }
"#,
        false,
    );

    assert!(
        !codes.contains(&2322),
        "tail never-call in a contextually-typed arrow must infer never, not void; got {codes:?}"
    );
}

#[test]
fn tail_namespace_never_call_in_contextual_arrow_infers_never() {
    let codes = diagnostic_codes(
        r#"
namespace RenamedDebug {
    export function fail(): never { throw new Error() }
}
const renamedNs: () => string = () => { RenamedDebug.fail() }
"#,
        false,
    );

    assert!(
        !codes.contains(&2322),
        "tail namespace-qualified never-call must infer never, not void; got {codes:?}"
    );
}

#[test]
fn tail_this_method_never_call_in_contextual_arrow_infers_never() {
    let codes = diagnostic_codes(
        r#"
interface RenamedBox { compute: () => number }
class RenamedComputer {
    panic(): never { throw new Error() }
    make(): RenamedBox {
        return { compute: () => { this.panic() } }
    }
}
"#,
        false,
    );

    assert!(
        !codes.contains(&2322),
        "tail this.method never-call must infer never, not void; got {codes:?}"
    );
}

#[test]
fn tail_assertion_false_call_in_contextual_arrow_infers_never() {
    let codes = diagnostic_codes(
        r#"
declare function renamedAssert(cond: boolean): asserts cond
const renamedAssertArrow: () => number = () => { renamedAssert(false) }
"#,
        false,
    );

    assert!(
        !codes.contains(&2322),
        "tail assertion call with a false condition terminates control flow; got {codes:?}"
    );
}

#[test]
fn tail_void_call_in_contextual_arrow_still_reports_ts2322() {
    // Negative control: a real `void`-returning tail call does NOT terminate
    // control flow, so the body still infers `void` and the contextual
    // `() => boolean` mismatch must still surface (parity with tsc). This guards
    // against over-applying the `never` inference.
    let codes = diagnostic_codes(
        r#"
declare function renamedEffect(): void
const renamedVoidArrow: () => boolean = () => { renamedEffect() }
"#,
        false,
    );

    assert!(
        codes.contains(&2322),
        "a real void tail body must still mismatch the contextual boolean return; got {codes:?}"
    );
}
