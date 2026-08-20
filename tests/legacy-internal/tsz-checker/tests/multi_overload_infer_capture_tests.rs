//! Tests for multi-overload `infer` capture in conditional types (extends
//! #13794's intersection-of-callable `ReturnType` reduction).
//!
//! When a conditional's `extends` pattern is a callable type with two or more
//! call signatures that each carry `infer` holes — e.g.
//! ```ignore
//! S extends {
//!     (...args: infer A1): infer R1
//!     (...args: infer A2): infer R2
//! } ? ... : never
//! ```
//! tsc binds each `infer` by pairing pattern signature `i` with the source
//! signature at the corresponding position. tsz previously only handled a
//! single-signature pattern and otherwise fell through to a plain subtype check
//! (which cannot bind `infer` holes), so the conditional wrongly collapsed to
//! its false branch (`never`). That `never` collapse is the root of the zustand
//! `devtools.ts(217)` / `persist.ts(337)` residual where
//! `StoreDevtools<StoreApi<T>>['setState']` failed to resolve to its overloaded
//! function type.
//!
//! Binder names are varied across cases so no fix can hardcode an identifier.

use crate::test_utils::check_source_diagnostics;

/// The minimal `StoreDevtools` shape: a two-overload `setState` callable matched
/// against a two-signature infer pattern, indexed back to `setState`. tsc
/// resolves it to the concrete overload set; the assignment of a wrongly-typed
/// value must therefore report TS2322 against that function type, not collapse
/// the conditional to `never`.
#[test]
fn two_overload_infer_capture_resolves_to_function_type() {
    let diags = check_source_diagnostics(
        r#"
type Action = string | { type: string };
type StoreDevtools<S> = S extends {
  setState: {
    (...args: infer Sa1): infer Sr1;
    (...args: infer Sa2): infer Sr2;
  };
}
  ? {
      setState(...args: [...args: Sa1, action?: Action]): Sr1;
      setState(...args: [...args: Sa2, action?: Action]): Sr2;
    }
  : never;

interface StoreApi<T> {
  setState: {
    (partial: T | Partial<T>, replace?: false): void;
    (state: T, replace: true): void;
  };
}
type NamedSet<T> = StoreDevtools<StoreApi<T>>["setState"];

function makeSetter<Model>() {
  // Contextual typing must flow the resolved overload signatures into the
  // arrow params; if the conditional collapsed to `never`, the params would
  // implicitly be `any` (TS7006).
  const ns: NamedSet<Model> = (partial, replace) => {
    void partial;
    void replace;
  };
  return ns;
}
"#,
    );

    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert!(
        ts7006.is_empty(),
        "Expected no implicit-any (TS7006) params; the multi-overload infer \
         pattern should resolve NamedSet<Model> to a concrete overload set, got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Renamed-binder variant: identical structure, completely different identifiers
/// for the alias, type parameters, and infer holes. Locks out any name-based
/// fast path.
#[test]
fn two_overload_infer_capture_renamed_binders() {
    let diags = check_source_diagnostics(
        r#"
type Tag = number | { kind: number };
type Wrap<Quux> = Quux extends {
  apply: {
    (...zs: infer ParamsA): infer RetA;
    (...zs: infer ParamsB): infer RetB;
  };
}
  ? {
      apply(...zs: [...zs: ParamsA, tag?: Tag]): RetA;
      apply(...zs: [...zs: ParamsB, tag?: Tag]): RetB;
    }
  : "FELL_THROUGH";

interface Holder<Elem> {
  apply: {
    (head: Elem, flag?: false): Elem;
    (head: Elem, flag: true): number;
  };
}
type Applied<Elem> = Wrap<Holder<Elem>>["apply"];

function build<Elem>() {
  const fn: Applied<Elem> = (head, flag) => {
    void flag;
    return head;
  };
  return fn;
}
"#,
    );

    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert!(
        ts7006.is_empty(),
        "Renamed-binder multi-overload infer capture should still resolve to a \
         concrete overload set (no implicit-any params), got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Return-only multi-overload capture: each pattern signature infers only its
/// return type. The captured returns must reach the true branch instead of the
/// conditional collapsing to its false branch.
#[test]
fn two_overload_return_only_infer_capture() {
    let diags = check_source_diagnostics(
        r#"
type Returns<F> = F extends {
  (a: string): infer R1;
  (a: number): infer R2;
}
  ? [R1, R2]
  : never;

interface Both {
  (a: string): boolean;
  (a: number): symbol;
}

declare const captured: Returns<Both>;
// Returns<Both> must reduce to [boolean, symbol], not `never`.
const ok: [boolean, symbol] = captured;
void ok;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Return-only two-overload infer capture should reduce to [R1, R2] \
         ([boolean, symbol]) rather than collapsing to never, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Negative / no-regression case: when the source does NOT structurally provide
/// matching overloads, the conditional must still take the false branch. A
/// non-callable source can never satisfy a call-signature pattern, so the false
/// branch (`"NO"`) is selected and assigning it to the true-branch type errors.
#[test]
fn multi_overload_pattern_false_branch_when_source_not_callable() {
    let diags = check_source_diagnostics(
        r#"
type Cap<F> = F extends {
  (a: string): infer R1;
  (a: number): infer R2;
}
  ? { r1: R1; r2: R2 }
  : "NO";

// A plain object with no call signatures cannot match the overload pattern.
type Result = Cap<{ a: number }>;
// Result must be the false branch "NO"; assigning to a wrong shape errors.
const bad: { r1: unknown; r2: unknown } = (null as unknown as Result);
void bad;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Non-callable source must take the false branch (\"NO\"); assigning it \
         to the true-branch shape should report TS2322, but none was found",
    );
}
