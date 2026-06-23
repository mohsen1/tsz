//! Regression coverage for #14512: pushing the polymorphic `this` into a
//! `this[]` field must not draw a spurious `TS2345`.
//!
//! Structural rule: when a member is read through a receiver whose type is
//! itself a *compound* `this`-relative type (e.g. `this.children: this[]`
//! accessed inside the class body), the apparent type's own `this` is already
//! bound to the receiver by the solver, and any `this` remaining in the member
//! type (the array *element* `this`, surfaced by `Array<this>.push(...items:
//! this[])` / `pop(): this`) is the *same* polymorphic `this`. `tsz` previously
//! re-substituted that `this` with the this-bearing receiver, nesting it one
//! level too deep (`this[]` -> `this[][]`), so `push(c: this)` compared the
//! argument against `this[]` and `pop()` was typed `this[] | undefined`. `tsc`
//! 6.x accepts every positive case below and keeps `TS2345` only for the
//! genuine mismatches.
//!
//! These cases exercise the *real* `Array<T>` lib signatures (`push`/`pop`/
//! index), so they load `lib.es5.d.ts`. The fix is name-agnostic (it keys on
//! the structural `this`-relative shape of the receiver, not on identifiers),
//! so the cases below vary every binder.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_compiled_lib_files};

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn es5_lib() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&["lib.es5.d.ts"])
}

fn count(source: &str, code: u32) -> usize {
    let libs = es5_lib();
    check_source_with_libs(source, "test.ts", strict_options(), &libs)
        .into_iter()
        .filter(|d| d.code == code)
        .count()
}

fn ts2345(source: &str) -> usize {
    count(source, 2345)
}

fn ts2322(source: &str) -> usize {
    count(source, 2322)
}

/// The exact repro from #14512: pushing `c: this` into `this.children: this[]`.
#[test]
fn push_this_into_this_array_field_is_clean() {
    let source = r#"
class TreeNode {
  children: this[] = [];
  addChild(c: this): void {
    this.children.push(c);
  }
}
"#;
    assert_eq!(
        ts2345(source),
        0,
        "`push(c: this)` into a `this[]` field must be accepted"
    );
}

/// Same rule, fully renamed binders — the fix is structural, not name-keyed.
#[test]
fn push_this_into_this_array_field_renamed_binders() {
    let source = r#"
class Graph {
  neighbours: this[] = [];
  link(other: this): void {
    this.neighbours.push(other);
  }
}
"#;
    assert_eq!(ts2345(source), 0, "renamed binders must behave identically");
}

/// `pop(): this` read through a `this[]` receiver returns `this | undefined`,
/// not `this[] | undefined`; returning it from a `this | undefined` method must
/// not draw `TS2322`.
#[test]
fn pop_from_this_array_field_returns_element_this() {
    let source = r#"
class Stack {
  items: this[] = [];
  drop(): this | undefined {
    return this.items.pop();
  }
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "`pop()` on a `this[]` receiver must yield `this | undefined`"
    );
}

/// A getter returning `this | undefined` whose body reads an element of a
/// `this[]` field.
#[test]
fn getter_returning_this_reads_this_array_element() {
    let source = r#"
class Ring {
  buf: this[] = [];
  add(node: this): void {
    this.buf.push(node);
  }
  get head(): this | undefined {
    return this.buf[0];
  }
}
"#;
    assert_eq!(ts2345(source), 0, "indexed element of `this[]` is `this`");
}

/// `readonly this[]` behaves the same for element reads.
#[test]
fn readonly_this_array_field_element_is_this() {
    let source = r#"
class Frozen {
  readonly kids: readonly this[] = [];
  first(): this | undefined {
    return this.kids[0];
  }
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "`readonly this[]` element read is `this | undefined`"
    );
}

/// A generic receiver constrained to a class with `self(): this` is not a
/// `this`-rooted receiver expression. Its return `this` must bind to the
/// receiver type parameter, not remain raw polymorphic `this`.
#[test]
fn generic_constraint_receiver_still_binds_this_to_type_parameter() {
    let source = r#"
class A {
  self() {
    return this;
  }
}
function f<T extends A>(x: T) {
  x = x.self();
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "`x.self()` for `x: T extends A` must return `T`"
    );
}

/// Ordinary union receivers whose member signatures mention `this` must still
/// substitute `this` with the union receiver. Only `this`-rooted receiver
/// expressions get the compound-this preservation rule.
#[test]
fn union_receiver_call_return_still_binds_this_to_union() {
    let source = r#"
class Foo {
  doThing(): Promise<this> {
    return Promise.resolve(this);
  }
}
class Bar extends Foo {
  bar: number = 0;
}
class Baz extends Foo {
  baz: number = 0;
}
declare const a: Bar | Baz;
a.doThing().then((result: Bar | Baz) => {
  result;
});
"#;
    assert_eq!(
        ts2345(source),
        0,
        "`Promise<this>` from a union receiver must resolve to `Promise<Bar | Baz>`"
    );
}

/// `sort(): this` read through a `this[]` receiver must still return the
/// *array* `this[]` (the Array's own `this`), not collapse to the element
/// `this`. Assigning the result to a `this[]` annotation must not draw
/// `TS2322` — this guards the apparent type's own `this` binding (the case a
/// naive "never rebind on a this-receiver" fix would have broken).
#[test]
fn sort_on_this_array_field_returns_this_array() {
    let source = r#"
class Sorted {
  rows: this[] = [];
  ordered(): this[] {
    return this.rows.sort();
  }
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "`sort()` on a `this[]` receiver must yield `this[]`"
    );
}

/// A subclass overriding the method keeps the same polymorphic-`this` behavior.
#[test]
fn subclass_override_push_this_is_clean() {
    let source = r#"
class TreeNode {
  children: this[] = [];
  addChild(c: this): void {
    this.children.push(c);
  }
}
class Folder extends TreeNode {
  addChild(c: this): void {
    super.addChild(c);
    this.children.push(c);
  }
}
"#;
    assert_eq!(ts2345(source), 0, "subclass override stays clean");
}

/// Negative control: pushing an unrelated class instance must still error.
#[test]
fn push_unrelated_instance_into_this_array_still_errors() {
    let source = r#"
class Other {}
class TreeNode {
  children: this[] = [];
  bad(): void {
    this.children.push(new Other());
  }
}
"#;
    assert_eq!(
        ts2345(source),
        1,
        "an unrelated instance must not satisfy the `this` element"
    );
}

/// Negative control: pushing a primitive must still error against `this`.
#[test]
fn push_primitive_into_this_array_still_errors() {
    let source = r#"
class TreeNode {
  children: this[] = [];
  bad(): void {
    this.children.push(42);
  }
}
"#;
    assert_eq!(
        ts2345(source),
        1,
        "a `number` must not satisfy the `this` element"
    );
}
