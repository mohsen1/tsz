//! A bare `x instanceof GenericCtor` (written with no type arguments) narrows an
//! unconstrained source (`unknown`/`any`) to the constructor's instance type
//! instantiated with `any` for every type parameter — tsc's `prototype`-derived
//! `Map<any, any>` shape — so member calls like `m.set(k, v)` accept any
//! argument.
//!
//! Outside a loop tsz already produced that shape: the flow path's fast
//! `node_types` lookup reads the `any`-filled `prototype` member off the
//! constructor type. But inside a loop body the back-edge re-narrowing pass runs
//! while the flow graph is still reaching its fixed point, so the constructor
//! *value* expression is untyped / typed `error`, the fast path misses, and the
//! symbol fallback resolved only the *bare generic* interface `Map<K, V>` with
//! its type parameters still free. Member access then rejected its arguments
//! against the free `K`/`V` — a spurious TS2345 that tsc never emits (#14945).
//!
//! These tests pin the structural rule (`any`-fill the bare generic instance at
//! the symbol fallback) and its guardrails:
//!   - non-generic globals (`Date`, N = 0) are untouched;
//!   - a union source keeps its concrete members (`Set<number>` still rejects a
//!     string argument), because the relation filters union members by
//!     subtype/instantiation independent of the instance type's arguments;
//!   - the rule covers the whole generic-global family (`Map`/`Set`/`WeakMap`)
//!     and user-defined generic classes alike.

use tsz_checker::context::CheckerOptions;

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags.iter().map(|(c, _)| *c).collect()
}

/// The exact witness from #14945: `value instanceof Map` inside a loop body must
/// narrow `value` to `Map<any, any>`, so `value.set(k, v)` with `k: string |
/// number` is accepted, matching tsc 5.9.3.
#[test]
fn map_instanceof_in_loop_narrows_to_any_filled_instance() {
    let diags = strict_diagnostics(
        r#"
function f(value: unknown) {
  const entries: Iterable<[string | number, unknown]> = [] as any;
  for (let [k, v] of entries) {
    if (value instanceof Map) {
      value.set(k, v);
    }
  }
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2345),
        "bare `instanceof Map` in a loop must narrow to Map<any, any>; got: {diags:?}"
    );
}

/// The no-loop control already worked through the fast path; lock it in so the
/// fix keeps both paths in agreement.
#[test]
fn map_instanceof_without_loop_stays_clean() {
    let diags = strict_diagnostics(
        r#"
function g(value: unknown) {
  if (value instanceof Map) {
    value.set(1, 2);
  }
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2345),
        "bare `instanceof Map` outside a loop must stay clean; got: {diags:?}"
    );
}

/// The rule is not Map-specific: `Set` in a loop must narrow to `Set<any>`.
#[test]
fn set_instanceof_in_loop_narrows_to_any_filled_instance() {
    let diags = strict_diagnostics(
        r#"
function h(value: unknown) {
  const xs: Iterable<number> = [] as any;
  for (const x of xs) {
    if (value instanceof Set) {
      value.add(x);
    }
  }
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2345),
        "bare `instanceof Set` in a loop must narrow to Set<any>; got: {diags:?}"
    );
}

/// A user-defined generic class behaves the same way: `value instanceof Box`
/// narrows `unknown` to `Box<any>` in a loop, accepting any argument.
#[test]
fn user_generic_class_instanceof_in_loop_narrows_to_any_filled_instance() {
    let diags = strict_diagnostics(
        r#"
class Box<T> { item!: T; put(x: T): void {} }
function b(value: unknown) {
  const xs: Iterable<number> = [] as any;
  for (const x of xs) {
    if (value instanceof Box) {
      value.put(x);
    }
  }
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2345),
        "bare `instanceof Box` in a loop must narrow to Box<any>; got: {diags:?}"
    );
}

/// GUARDRAIL: a non-generic global (`Date`, zero type parameters) is left
/// untouched by the `any`-fill — and was always clean here. The point is to pin
/// that the fill does not perturb the N = 0 case.
#[test]
fn non_generic_global_instanceof_in_loop_unchanged() {
    let diags = strict_diagnostics(
        r#"
function d(value: unknown) {
  const xs: Iterable<number> = [] as any;
  for (const _x of xs) {
    if (value instanceof Date) {
      value.getTime();
    }
  }
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2339) && !codes(&diags).contains(&2345),
        "non-generic `instanceof Date` must stay clean; got: {diags:?}"
    );
}

/// GUARDRAIL: a *union* source keeps its concrete member. `Set<number> | string`
/// narrowed by `instanceof Set` is `Set<number>`, so `value.add("x")` must STILL
/// report TS2345 — the `any`-fill only supplies the fallback instance type and
/// must not erase a concrete member's arguments. This holds both inside and
/// outside a loop.
#[test]
fn union_source_keeps_concrete_member_and_still_errors() {
    let diags = strict_diagnostics(
        r#"
function u(value: Set<number> | string) {
  if (value instanceof Set) {
    value.add("x");
  }
}
function uloop(value: Set<number> | string) {
  const xs: Iterable<number> = [] as any;
  for (const _x of xs) {
    if (value instanceof Set) {
      value.add("x");
    }
  }
}
"#,
    );
    let ts2345 = diags.iter().filter(|(c, _)| *c == 2345).count();
    assert!(
        ts2345 >= 2,
        "union source `Set<number> | string` must keep Set<number> and reject a string arg \
         in both the plain and loop forms; got: {diags:?}"
    );
}
