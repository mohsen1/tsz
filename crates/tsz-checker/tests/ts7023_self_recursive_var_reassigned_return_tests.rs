//! Regression tests for #16987: a self-recursive function whose own binding is a
//! genuinely circular *variable* (e.g. reassigned inside its own body) must infer
//! return type `any` — matching `tsc`'s circular implicit-`any` (TS7023)
//! resolution — rather than the degenerate `void` / `never` that return
//! aggregation produces once the direct self-call return is dropped.
//!
//! The clean, no-base-case recursion in a *named function declaration*
//! (`function f(n){ return f(n); }`) must continue to infer `never` (tsc parity,
//! existing behavior). Both shapes are exercised so the discriminator cannot
//! regress one to fix the other.
//!
//! Binder names are varied across cases (anti-hardcoding): the logic keys off the
//! structural shape (variable-bound circular self-recursion vs. named-declaration
//! clean recursion), never a specific identifier.

use tsz_checker::test_utils::{
    check_js_source_code_messages_with_options, check_js_source_codes_with_options,
    non_strict_checker_options, strict_checker_options,
};

fn js_codes_non_strict(src: &str) -> Vec<u32> {
    let mut codes =
        check_js_source_codes_with_options(src, "test.js", non_strict_checker_options());
    codes.sort_unstable();
    codes
}

fn js_codes_strict(src: &str) -> Vec<u32> {
    let mut codes = check_js_source_codes_with_options(src, "test.js", strict_checker_options());
    codes.sort_unstable();
    codes
}

// The reported witness (#16987): `fn2` is reassigned inside its own body, so its
// resolution is circular and tsc infers `any` for the recursive return. That
// makes `fn2(1)` and therefore `d` `any`, so `d.redefined()` is clean. Before the
// fix tsz threaded `void` through and emitted a spurious
// `TS2339: Property 'redefined' does not exist on type 'void'`.
const REASSIGNED_WITNESS: &str = r#"
var fn2 = function(name) {
  fn2 = compose(this, 0, 1)
  return fn2(name)

  function compose(child, level, find) {
    if (child === find) {
      return level
    }
    return compose(child, level + 1, find)
  }
}

var d = fn2(1);
d.redefined();
"#;

#[test]
fn var_reassigned_self_recursion_no_spurious_ts2339_non_strict() {
    // Under `@strict: false` the only diagnostic tsz used to emit was the
    // spurious TS2339; with the fix the file is clean.
    let codes = js_codes_non_strict(REASSIGNED_WITNESS);
    assert!(
        !codes.contains(&2339),
        "spurious TS2339 on `d.redefined()` — fn2's circular return type must be `any`, not `void`; got {codes:?}"
    );
}

#[test]
fn var_reassigned_self_recursion_detects_circularity_and_no_ts2339_strict() {
    // Under strict, the circularity detector fires TS7023 (matching tsc) and,
    // once the return type is `any`, the spurious TS2339 is gone.
    let codes = js_codes_strict(REASSIGNED_WITNESS);
    assert!(
        codes.contains(&7023),
        "expected TS7023 (implicit circular return) on fn2; got {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "spurious TS2339 on `d.redefined()` — fn2's circular return type must be `any`, not `void`; got {codes:?}"
    );
}

// A minimal variable-bound arrow self-recursion (no trailing declaration, no
// reassignment) is still a genuinely circular *variable*, so tsc infers `any`
// too. tsz distinctively emits TS2339 on property access of BOTH `void` and
// `never` but never on `any`, so the absence of TS2339 on `r.anything()` proves
// the return type is specifically `any`. This guards the broader fix, not just
// the exact reported fixture.
#[test]
fn var_bound_arrow_self_recursion_infers_any_strict() {
    let src = r#"
const loop = () => loop();
const r = loop();
r.anything();
"#;
    let codes = js_codes_strict(src);
    assert!(
        !codes.contains(&2339),
        "variable-bound circular self-recursion must infer `any` (no TS2339 on property access); got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Adjacent case: clean no-base-case recursion in a named function declaration
// must stay `never` (no regression). A named declaration is not a resolving
// variable, so it is never recorded as a circular return site and keeps the
// `never` path — and, being non-circular, emits no TS7023. This is exactly the
// behavior #16987 requires must not regress while the reassigned case moves to
// `any`.
// ---------------------------------------------------------------------------

#[test]
fn clean_no_base_case_named_recursion_stays_never_not_void() {
    // `function rec(n) { return rec(n); }` never terminates: tsz infers `never`
    // (unchanged by this fix — a named declaration is not a resolving variable,
    // so the circular-`any` override never applies). Property access on the
    // result reports TS2339 anchored on `never` — crucially NOT `void`, which is
    // the exact regression the reassigned-case fix guards against, and NOT `any`.
    let src = r#"
function rec(n) { return rec(n); }
var x = rec(1);
x.whatever();
"#;
    let messages =
        check_js_source_code_messages_with_options(src, "test.js", strict_checker_options());
    let ts2339 = messages.iter().find(|(code, _)| *code == 2339);
    let (_, msg) = ts2339.unwrap_or_else(|| {
        panic!(
            "expected TS2339 on `x.whatever()` for a `never`-returning recursion; got {messages:?}"
        )
    });
    assert!(
        msg.contains("'never'"),
        "named-declaration recursion must stay `never`, not `void`/`any`; got message: {msg:?}"
    );
    let codes: Vec<u32> = messages.iter().map(|(c, _)| *c).collect();
    assert!(
        !codes.contains(&7023),
        "clean (named-declaration) recursion is not a circular variable; TS7023 must not fire; got {codes:?}"
    );
}

#[test]
fn recursion_with_base_case_infers_base_type_not_void() {
    // A genuine base case decides the inferred return type; the direct self-call
    // return is dropped from aggregation (tsc parity). The base returns a number,
    // so `y` is `number` and `y.toFixed()` is clean.
    let src = r#"
function rec2(n) { if (n > 0) return 1; return rec2(n); }
var y = rec2(1);
y.toFixed();
"#;
    let codes = js_codes_strict(src);
    assert!(
        !codes.contains(&2339),
        "base-case recursion must infer the base type (`number`), not `void`; got {codes:?}"
    );
    assert!(
        !codes.contains(&7023),
        "recursion with a real base case is not circular; TS7023 must not fire; got {codes:?}"
    );
}
