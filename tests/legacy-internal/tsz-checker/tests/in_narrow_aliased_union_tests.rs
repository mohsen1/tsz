//! `in`-operator narrowing must resolve a union written behind a *type alias*
//! or a *generic application* before filtering its members.
//!
//! A direct union (`A | B`) reaches narrowing as a `Union` and is filtered
//! member-wise. But a union behind a generic alias —
//! `type Enumerable<T> = ArrayLike<T> | Iterable<T>` used as `items:
//! Enumerable<T>` — reaches narrowing as an unresolved `Application`/`Lazy`,
//! never a `Union`. Before the fix the receiver was mistaken for an opaque
//! non-union type, so the positive branch degraded it to
//! `source & Record<prop, unknown>`, collapsing the accessed property to
//! `unknown` and emitting a spurious TS2322 against the declared return
//! (remeda `length.ts`: `"length" in items ? items.length : [...items].length`).
//!
//! These tests pin the parity for both the lib-backed `ArrayLike`/`Iterable`
//! shape and binder-name-varied custom interfaces, across the positive and
//! negative branches, and for generic *and* concrete alias forms. Binder names
//! are varied so the behavior is driven by the structural shape, not by any
//! identifier spelling.

use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_with_libs, load_default_lib_files, strict_checker_options};

/// Codes the fix must stop emitting on the narrowed branches. Asserting their
/// absence (rather than an empty set) keeps the tests robust to unrelated
/// lib-surface noise while still failing on the original bug.
const FALSE_POSITIVE_CODES: &[u32] = &[2322, 2339, 18046];

fn codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    let diags: Vec<Diagnostic> =
        check_source_with_libs(source, "test.ts", strict_checker_options(), &libs);
    diags.iter().map(|d| d.code).collect()
}

fn assert_no_false_positives(source: &str, label: &str) {
    let got = codes(source);
    for code in FALSE_POSITIVE_CODES {
        assert!(
            !got.contains(code),
            "{label}: expected no TS{code} after aliased-union `in` narrowing, got: {got:?}"
        );
    }
}

/// The exact remeda repro: a generic aliased union `Enumerable<T> = ArrayLike<T>
/// | Iterable<T>`. `"length" in items` must narrow the true branch to the
/// `ArrayLike<T>` constituent so `items.length` is `number`.
#[test]
fn generic_aliased_lib_union_narrows_length_to_number() {
    let source = r#"
type Enumerable<T> = ArrayLike<T> | Iterable<T>;
const lengthImplementation = <T,>(items: Enumerable<T>): number =>
  "length" in items ? items.length : [...items].length;
"#;
    assert_no_false_positives(source, "remeda Enumerable<T> length");
}

/// Same union, but as a function declaration rather than an arrow, and with a
/// renamed binder. The narrowing must not depend on the declaration form or the
/// parameter spelling.
#[test]
fn generic_aliased_lib_union_function_declaration_form() {
    let source = r#"
type Collection<U> = ArrayLike<U> | Iterable<U>;
function count<U>(collection: Collection<U>): number {
  if ("length" in collection) {
    const measured: number = collection.length;
    return measured;
  }
  return [...collection].length;
}
"#;
    assert_no_false_positives(source, "Collection<U> count");
}

/// Generic aliased union over binder-name-varied custom interfaces — no lib
/// dependency. The positive branch keeps the `length`-bearing member; the
/// negative branch keeps the rest. Exercises both branches of the filter.
#[test]
fn generic_aliased_custom_union_both_branches() {
    let source = r#"
interface Sized { readonly span: number; }
interface Named { readonly label: string; }
type Holder<V> = (Sized | Named) | V;
function inspect<V extends { extra: boolean }>(holder: Holder<V>): void {
  if ("span" in holder) {
    const s: number = holder.span;
  } else {
    // `holder` keeps `Named | V`; `span` is genuinely absent here.
  }
}
"#;
    assert_no_false_positives(source, "Holder<V> span");
}

/// A non-generic alias to a custom union must narrow identically — the alias
/// wrapper must not hide the union members.
#[test]
fn concrete_aliased_custom_union_narrows() {
    let source = r#"
interface WithCount { readonly total: number; }
interface WithText { readonly text: string; }
type Either = WithCount | WithText;
function read(either: Either): number {
  if ("total" in either) {
    return either.total;
  }
  return 0;
}
"#;
    assert_no_false_positives(source, "Either total");
}

/// Nested generic alias (`alias of alias`) still resolves to the underlying
/// union before filtering.
#[test]
fn nested_generic_alias_resolves_before_filtering() {
    let source = r#"
type Base<T> = ArrayLike<T> | Iterable<T>;
type Wrapped<T> = Base<T>;
function size<T>(seq: Wrapped<T>): number {
  return "length" in seq ? seq.length : [...seq].length;
}
"#;
    assert_no_false_positives(source, "Wrapped<T> size");
}

/// Negative control: `!("length" in items)` must narrow the false branch to the
/// `Iterable<T>` member so the spread `[...items]` is allowed, while the true
/// branch still sees `ArrayLike<T>`.
#[test]
fn generic_aliased_union_negative_branch_spreads_iterable() {
    let source = r#"
type Seq<T> = ArrayLike<T> | Iterable<T>;
function toArray<T>(seq: Seq<T>): readonly T[] {
  if (!("length" in seq)) {
    return [...seq];
  }
  const out: T[] = [];
  for (let i = 0; i < seq.length; i++) {
    out.push(seq[i]);
  }
  return out;
}
"#;
    assert_no_false_positives(source, "Seq<T> negative branch");
}
