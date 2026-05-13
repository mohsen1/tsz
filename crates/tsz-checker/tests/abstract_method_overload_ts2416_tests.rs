use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

/// Structural rule: when a derived class implements abstract overload
/// signatures from a base class with matching overload declarations plus
/// an implementation signature, the externally-visible API is the
/// overload set (not the implementation signature). Per-AST-node TS2416
/// comparison must not flag the impl signature against a single base
/// overload.
///
/// Issue #6489: tsz used to compare the impl signature
/// `(x: string | number) => string | number` against the base's first
/// overload `(x: string) => string` and emit a false TS2416. tsc accepts
/// this code.
#[test]
fn abstract_method_overload_impl_signature_does_not_emit_ts2416() {
    let source = r#"
abstract class Base {
  abstract method(x: string): string;
  abstract method(x: number): number;
}

class Derived extends Base {
  method(x: string): string;
  method(x: number): number;
  method(x: string | number): string | number {
    return typeof x === "string" ? x.toUpperCase() : x * 2;
  }
}

const derived = new Derived();
const _s: string = derived.method("hello");
const _n: number = derived.method(42);
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(
        ts2416.is_empty(),
        "Expected no TS2416 for matched abstract overload implementation, got: {diags:#?}"
    );
}

/// Non-abstract concrete base with overload signatures. The same rule
/// applies — the derived class's externally-visible API is the overload
/// set, so a compatible impl signature must not produce TS2416.
#[test]
fn concrete_method_overload_impl_signature_does_not_emit_ts2416() {
    let source = r#"
class Base {
  foo(x: string): string;
  foo(x: number): number;
  foo(x: string | number): string | number { return x; }
}
class Sub extends Base {
  foo(x: string): string;
  foo(x: number): number;
  foo(x: string | number): string | number { return x; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(
        ts2416.is_empty(),
        "Expected no TS2416 for matched concrete overloaded method, got: {diags:#?}"
    );
}

/// Three overloads. The fix must generalize to arbitrary arity, not just
/// the two-overload shape from the reported repro.
#[test]
fn abstract_three_overload_impl_signature_does_not_emit_ts2416() {
    let source = r#"
abstract class Base {
  abstract op(x: string): string;
  abstract op(x: number): number;
  abstract op(x: boolean): boolean;
}
class Derived extends Base {
  op(x: string): string;
  op(x: number): number;
  op(x: boolean): boolean;
  op(x: string | number | boolean): string | number | boolean { return x; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(
        ts2416.is_empty(),
        "Expected no TS2416 for matched 3-overload method, got: {diags:#?}"
    );
}

/// Type-parameter spelling must not affect the structural fix. Renaming
/// the iteration variables and class name should not change behavior.
#[test]
fn abstract_method_overload_alternate_naming_does_not_emit_ts2416() {
    let source = r#"
abstract class XBase {
  abstract handle(value: string): string;
  abstract handle(value: number): number;
}
class XImpl extends XBase {
  handle(value: string): string;
  handle(value: number): number;
  handle(value: string | number): string | number { return value; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(
        ts2416.is_empty(),
        "Expected no TS2416 under alternate naming, got: {diags:#?}"
    );
}

/// Genuine mismatch must still produce TS2416: derived overload's return
/// type differs from base's. The combined check must flag this.
#[test]
fn overload_with_wrong_return_type_still_emits_ts2416() {
    let source = r#"
abstract class Base {
  abstract method(x: string): string;
}
class Derived extends Base {
  method(x: string): number;
  method(x: string): number { return 42; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(
        !ts2416.is_empty(),
        "Expected TS2416 for derived overload with wrong return type, got: {diags:#?}"
    );
}

/// Derived missing one of the base's abstract overloads must still
/// produce TS2416 because the combined derived type cannot satisfy the
/// missing base overload.
#[test]
fn overload_with_missing_overload_still_emits_ts2416() {
    let source = r#"
abstract class Base {
  abstract m(x: string): string;
  abstract m(x: number): number;
}
class Derived extends Base {
  m(x: string): string;
  m(x: string): string { return x; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(
        !ts2416.is_empty(),
        "Expected TS2416 when derived misses a required base overload, got: {diags:#?}"
    );
}

/// Static method overloads in classes must follow the same rule on the
/// static side. Static and instance overload sets are tracked
/// independently.
#[test]
fn static_method_overload_does_not_emit_ts2416() {
    let source = r#"
class Base {
  static foo(x: string): string;
  static foo(x: number): number;
  static foo(x: string | number): string | number { return x; }
}
class Sub extends Base {
  static foo(x: string): string;
  static foo(x: number): number;
  static foo(x: string | number): string | number { return x; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    let ts2417: Vec<_> = diags.iter().filter(|d| d.code == 2417).collect();
    assert!(
        ts2416.is_empty(),
        "Expected no TS2416 for matched static overloaded method, got: {diags:#?}"
    );
    assert!(
        ts2417.is_empty(),
        "Expected no TS2417 for matched static overloaded method, got: {diags:#?}"
    );
}

/// Single-declaration methods (no overloads on either side) must
/// continue to use the existing per-node compat check. A genuine
/// mismatch must still emit TS2416 to confirm the new overload path
/// did not regress the non-overloaded case.
#[test]
fn non_overloaded_method_incompatible_still_emits_ts2416() {
    let source = r#"
class Base { f(): string { return ""; } }
class Derived extends Base {
  f(): number { return 1; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(
        !ts2416.is_empty(),
        "Expected TS2416 for non-overloaded method with incompatible return, got: {diags:#?}"
    );
}

/// Deep inheritance: overload signatures declared on a grandparent
/// abstract class must be looked up through the chain summary, so that
/// `Derived extends Mid extends GrandBase` works the same as direct
/// inheritance.
#[test]
fn deep_chain_abstract_overload_does_not_emit_ts2416() {
    let source = r#"
abstract class GrandBase {
  abstract op(x: string): string;
  abstract op(x: number): number;
}
abstract class Mid extends GrandBase {}
class Derived extends Mid {
  op(x: string): string;
  op(x: number): number;
  op(x: string | number): string | number { return x; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(
        ts2416.is_empty(),
        "Expected no TS2416 for grandparent abstract overload, got: {diags:#?}"
    );
}
