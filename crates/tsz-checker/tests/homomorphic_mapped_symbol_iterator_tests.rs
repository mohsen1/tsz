//! Regression tests for issue #13619: a homomorphic mapped type over a
//! symbol-keyed lib interface (`Iterable<T>`, whose sole member is the
//! well-known `[Symbol.iterator]` method) must preserve that symbol-keyed
//! method so the result stays iterable.
//!
//! Root cause: when the mapped-type evaluator emitted a symbol-keyed output
//! property it looked the source method up by a synthetic `__unique_<id>`
//! atom instead of the canonical `"[Symbol.iterator]"` atom the lib interface
//! stores it under. The lookup missed, so `T[Symbol.iterator]` resolved to
//! `undefined` and the homomorphic result silently dropped the iterator
//! method — producing a false `TS2488` at a `for-of` over the result and a
//! false `TS7053`/widened `symbol` for `keyof`.
//!
//! The witnesses use the *real* embedded lib `Iterable` (a `Lazy(DefId)` from
//! the lib snapshot); an inline interface does not reproduce because its
//! symbol member is stored under a fresh local atom. Binder names are varied
//! to prove the fix is structural, not name-driven.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str, libs: &[Arc<LibFile>]) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    check_source_with_libs(source, "test.ts", options, libs)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

/// Simple identity homomorphic mapped type over the lib `Iterable<T>` must keep
/// `[Symbol.iterator]`, so a `for-of` over the result is valid (no TS2488) and
/// the symbol index access resolves (no TS7053).
#[test]
fn identity_mapped_over_iterable_preserves_symbol_iterator() {
    let libs = load_default_lib_files();
    let codes = strict_codes(
        r#"
type Mirror<Container> = { readonly [Slot in keyof Container]: Container[Slot] };
function walk<Payload>(seq: Mirror<Iterable<Payload>>) {
  for (const item of seq) {
  }
  const probe = seq[Symbol.iterator];
}
"#,
        &libs,
    );
    assert!(
        !codes.contains(&2488),
        "homomorphic mapped over Iterable should stay iterable (no TS2488); got {codes:?}"
    );
    assert!(
        !codes.contains(&7053),
        "the preserved [Symbol.iterator] key must be symbol-indexable (no TS7053); got {codes:?}"
    );
}

/// Concrete instantiation: `Mirror<Iterable<number>>` must also be iterable.
#[test]
fn identity_mapped_over_concrete_iterable_preserves_symbol_iterator() {
    let libs = load_default_lib_files();
    let codes = strict_codes(
        r#"
type Copy<Source> = { readonly [Field in keyof Source]: Source[Field] };
function scan(seq: Copy<Iterable<number>>) {
  for (const item of seq) {
    const keep: number = item;
  }
}
"#,
        &libs,
    );
    assert!(
        !codes.contains(&2488),
        "homomorphic mapped over concrete Iterable<number> should stay iterable; got {codes:?}"
    );
    assert!(
        !codes.contains(&7053),
        "the preserved [Symbol.iterator] key must be symbol-indexable; got {codes:?}"
    );
}

/// A mapped type whose template branches on `Slot extends typeof Symbol.iterator`
/// (the `DeepReadonlyObject` shape from ts-essentials) must still preserve the
/// symbol-keyed iterator method.
#[test]
fn symbol_iterator_conditional_template_mapped_preserves_iterator() {
    let libs = load_default_lib_files();
    let codes = strict_codes(
        r#"
type DeepCopyObject<Node> = {
  readonly [Slot in keyof Node]: Slot extends typeof Symbol.iterator
    ? Node[Slot] extends () => Iterator<infer Yielded, infer Ret, infer Sent>
      ? () => Iterator<Yielded, Ret, Sent>
      : Node[Slot]
    : Node[Slot];
};
function traverse<Payload>(seq: DeepCopyObject<Iterable<Payload>>) {
  for (const item of seq) {
  }
}
"#,
        &libs,
    );
    assert!(
        !codes.contains(&2488),
        "symbol-iterator-aware mapped over Iterable should stay iterable; got {codes:?}"
    );
    assert!(
        !codes.contains(&7053),
        "the preserved [Symbol.iterator] key must be symbol-indexable; got {codes:?}"
    );
}

/// Negative/leaf guard: a mapped type over an object that has NO
/// `[Symbol.iterator]` is correctly NOT iterable (TS2488 still fires). The fix
/// must not make every mapped object spuriously iterable.
#[test]
fn mapped_over_non_iterable_object_still_reports_ts2488() {
    let libs = load_default_lib_files();
    let codes = strict_codes(
        r#"
interface Plain { value: number; }
type Echo<Shape> = { readonly [Member in keyof Shape]: Shape[Member] };
function loop(notSeq: Echo<Plain>) {
  for (const item of notSeq) {
  }
}
"#,
        &libs,
    );
    assert!(
        codes.contains(&2488),
        "a mapped object without [Symbol.iterator] must NOT be iterable (expect TS2488); got {codes:?}"
    );
}
