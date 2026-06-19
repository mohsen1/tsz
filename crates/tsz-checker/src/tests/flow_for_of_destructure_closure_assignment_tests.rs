//! Definite-assignment (TS2454) parity for `for...of` / `for...in` loop
//! bindings captured by a closure.
//!
//! Structural rule: a `for...in` / `for...of` loop binding — a simple
//! identifier (`for (const x of …)`) or a destructuring pattern
//! (`for (const [k, v] of …)`, `for (const { x } of …)`) — is assigned by the
//! loop on every iteration before the body runs, so it is definitely assigned
//! throughout the body. A reference inside a deferred closure declared in the
//! body can only execute after that assignment, so `tsc` never reports TS2454
//! for it. tsz previously exempted the simple-binding shape but reported a
//! false TS2454 for the *destructured* loop binding captured by a closure.
//!
//! The decision is purely structural (the binding's AST position is a loop
//! header), never keyed on the binding's name — the renamed-binder cases prove
//! it is not name-driven.

use crate::test_utils::check_source_strict_codes as check_strict;

fn count_2454(source: &str) -> usize {
    check_strict(source).iter().filter(|&&c| c == 2454).count()
}

// ---------------------------------------------------------------------------
// False positives that the fix removes (clean under tsc).
// ---------------------------------------------------------------------------

#[test]
fn for_of_array_destructure_captured_by_closure_is_clean() {
    // The witnessed bug: array destructuring in a `for...of`, a bound variable
    // captured by a nested arrow. tsc is clean.
    assert_eq!(
        count_2454(
            r#"
function b() {
  const out: any = {};
  for (const [key, refKey] of [["data", "_data"]] as const) {
    out[key] = () => refKey;
  }
}
"#,
        ),
        0,
        "array-destructured for-of binding captured by a closure must not report TS2454",
    );
}

#[test]
fn for_of_object_destructure_captured_by_closure_is_clean() {
    assert_eq!(
        count_2454(
            r#"
function h() {
  const o: any = {};
  for (const { x } of [{ x: 1 }] as const) {
    o.f = () => x;
  }
}
"#,
        ),
        0,
        "object-destructured for-of binding captured by a closure must not report TS2454",
    );
}

#[test]
fn for_of_nested_destructure_captured_by_closure_is_clean() {
    // Nested array/object pattern — every leaf binding is loop-assigned.
    assert_eq!(
        count_2454(
            r#"
function n() {
  const sink: any = {};
  for (const [{ a }, [b]] of [[{ a: 1 }, [2]]] as const) {
    sink.f = () => a + b;
  }
}
"#,
        ),
        0,
        "nested-destructured for-of bindings captured by a closure must not report TS2454",
    );
}

#[test]
fn for_of_let_destructure_captured_by_closure_is_clean() {
    assert_eq!(
        count_2454(
            r#"
function l() {
  const out: any = {};
  for (let [p, q] of [[1, 2]] as const) {
    out[p] = () => q;
  }
}
"#,
        ),
        0,
        "let-declared destructured for-of binding captured by a closure must not report TS2454",
    );
}

#[test]
fn for_of_var_destructure_captured_by_closure_is_clean() {
    assert_eq!(
        count_2454(
            r#"
function v() {
  const out: any = {};
  for (var [p, q] of [[1, 2]] as const) {
    out[p] = () => q;
  }
}
"#,
        ),
        0,
        "var-declared destructured for-of binding captured by a closure must not report TS2454",
    );
}

#[test]
fn for_in_destructure_captured_by_closure_is_clean() {
    assert_eq!(
        count_2454(
            r#"
function fin() {
  const sink: any = {};
  const obj: Record<string, number> = {};
  for (const [c0] in obj) {
    sink.f = () => c0;
  }
}
"#,
        ),
        0,
        "destructured for-in binding captured by a closure must not report TS2454",
    );
}

#[test]
fn for_of_destructure_in_defineproperty_getter_is_clean() {
    // The ofetch canary witness shape: the closure is a getter inside an object
    // literal passed to `Object.defineProperty`.
    assert_eq!(
        count_2454(
            r#"
function ofetchLike(e: any, ctx: any) {
  for (const [key, refKey] of [["data", "_data"]] as const) {
    Object.defineProperty(e, key, {
      get() {
        return ctx.response && ctx.response[refKey];
      },
    });
  }
}
"#,
        ),
        0,
        "for-of destructured binding read inside a defineProperty getter must not report TS2454",
    );
}

#[test]
fn for_of_destructure_captured_by_closure_is_name_agnostic() {
    // Anti-hardcoding: identical structure, every binder renamed. The exemption
    // is structural (loop-header binding), not keyed on `key`/`refKey`/etc.
    assert_eq!(
        count_2454(
            r#"
function alpha() {
  const bravo: any = {};
  for (const [charlie, delta] of [["echo", "foxtrot"]] as const) {
    bravo[charlie] = () => delta;
  }
}
"#,
        ),
        0,
        "renamed destructured for-of binding captured by a closure must not report TS2454",
    );
}

// ---------------------------------------------------------------------------
// Negative controls that must stay clean (already correct before the fix).
// ---------------------------------------------------------------------------

#[test]
fn for_of_destructure_direct_use_is_clean() {
    // Direct use in the body (no closure) was already clean; the fix must not
    // disturb it.
    assert_eq!(
        count_2454(
            r#"
function a() {
  for (const [k, v] of [["a", "b"]] as const) {
    console.log(k, v);
  }
}
"#,
        ),
        0,
        "direct use of a destructured for-of binding must remain clean",
    );
}

#[test]
fn for_of_simple_binding_captured_by_closure_is_clean() {
    // Simple (non-destructured) for-of binding captured by a closure — already
    // exempt; this guards against a regression in that path.
    assert_eq!(
        count_2454(
            r#"
function d() {
  const o: any = {};
  for (const x of [1, 2] as const) {
    o.f = () => x;
  }
}
"#,
        ),
        0,
        "simple for-of binding captured by a closure must remain clean",
    );
}

#[test]
fn plain_destructure_captured_by_closure_is_clean() {
    // Plain (non-for-of) destructuring with an initializer, captured by a
    // closure — already clean.
    assert_eq!(
        count_2454(
            r#"
function e() {
  const [a, b] = [1, 2] as const;
  const f = () => b;
  f();
}
"#,
        ),
        0,
        "plain destructuring captured by a closure must remain clean",
    );
}

// ---------------------------------------------------------------------------
// Over-suppression guards: genuine TS2454 must still fire.
// ---------------------------------------------------------------------------

#[test]
fn plain_let_used_before_assignment_still_reports() {
    // A plain (non-loop) annotated `let` read before assignment in the same
    // scope still reports TS2454 — the fix must not silence real diagnostics.
    assert_eq!(
        count_2454(
            r#"
function g() {
  let x: number;
  const y: number = x;
  return y;
}
"#,
        ),
        1,
        "a genuine use-before-assignment must still report exactly one TS2454",
    );
}

#[test]
fn for_of_binding_used_before_the_loop_still_reports() {
    // A binding read *before* its for-of loop (a separate annotated `let`) is a
    // genuine use-before-assignment; the loop-binding exemption must not bleed
    // into unrelated references.
    assert_eq!(
        count_2454(
            r#"
function p() {
  let z: number;
  const w: number = z;
  for (const [z2] of [[1]] as const) {
    void z2;
  }
  return w;
}
"#,
        ),
        1,
        "a real use-before-assignment outside the loop body must still report TS2454",
    );
}
