//! `InstanceType<typeof Class>` for a class with a polymorphic-`this` member.
//!
//! Regression coverage for the Kysely "instance vs constructor" false-positive
//! family (issue #10663): a fluent class whose members reference `this`
//! (`clone(): this`, `eq(o: this)`, `self: this`) made
//! `InstanceType<typeof Class>` stop relating to the class instance type, so
//! every use of such a class through `InstanceType<typeof X>` raised a spurious
//! `TS2322`/`TS2345`.
//!
//! Structural rule: when a conditional's check type already *binds* the `this`
//! it contains to a concrete instance (the construct-signature return of a
//! `typeof Class`), the conditional must be evaluated, not deferred. Only a
//! *free* contextual `this` (the check type is `this`, `this[]`, `keyof this`,
//! `A | this`, …) defers. Owner: solver conditional evaluation
//! (`evaluate_rules/conditional.rs`).

/// Minimal lib surface so `class`/conditional/`infer` resolve without pulling
/// the real standard library into the test.
fn check(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(&format!(
        r#"
interface Array<T> {{}}
interface Boolean {{}}
interface CallableFunction {{}}
interface Function {{}}
interface IArguments {{}}
interface NewableFunction {{}}
interface Number {{}}
interface Object {{}}
interface RegExp {{}}
interface String {{}}

type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;
type ReturnType<T extends (...args: any) => any> =
    T extends (...args: any) => infer R ? R : any;

{source}
"#
    ))
}

fn instance_constructor_family_codes(diagnostics: &[(u32, String)]) -> Vec<&(u32, String)> {
    diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322 || *code == 2345)
        .collect()
}

/// Canonical witness: a `this`-returning method makes `InstanceType<typeof B>`
/// no longer relate to `B` in either direction. tsc is clean.
#[test]
fn instance_type_of_return_this_class_relates_both_directions() {
    let diagnostics = check(
        r#"
class Builder {
  clone(): this { return this; }
}
declare const inst: Builder;
const a: InstanceType<typeof Builder> = inst;
const b: Builder = (null as any as InstanceType<typeof Builder>);
"#,
    );
    let errs = instance_constructor_family_codes(&diagnostics);
    assert!(
        errs.is_empty(),
        "InstanceType<typeof Builder> must relate to Builder when Builder has a `this`-returning member.\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}

/// `this` in parameter position (contravariant) triggers the same root.
#[test]
fn instance_type_of_param_this_class_no_spurious_error() {
    let diagnostics = check(
        r#"
class Node2 {
  equals(other: this): boolean { return other === this; }
}
declare const n: Node2;
const a: InstanceType<typeof Node2> = n;
const b: Node2 = (null as any as InstanceType<typeof Node2>);
"#,
    );
    let errs = instance_constructor_family_codes(&diagnostics);
    assert!(
        errs.is_empty(),
        "InstanceType<typeof Node2> must relate to Node2 with a `this`-typed parameter.\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}

/// `this` in property position triggers the same root.
#[test]
fn instance_type_of_property_this_class_no_spurious_error() {
    let diagnostics = check(
        r#"
class Cell {
  neighbor!: this;
}
declare const c: Cell;
const a: InstanceType<typeof Cell> = c;
const b: Cell = (null as any as InstanceType<typeof Cell>);
"#,
    );
    let errs = instance_constructor_family_codes(&diagnostics);
    assert!(
        errs.is_empty(),
        "InstanceType<typeof Cell> must relate to Cell with a `this`-typed property.\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}

/// Kysely-shaped: a fluent base class with a static `create()` returning
/// `InstanceType<typeof this>` and a subclass adding more `this` methods.
#[test]
fn static_create_instance_type_of_this_on_fluent_subclass() {
    let diagnostics = check(
        r#"
class AlterColumnBuilder {
  alter(): this { return this; }
  static create(): InstanceType<typeof AlterColumnBuilder> {
    return new this();
  }
}
class WhereBuilder extends AlterColumnBuilder {
  where(): this { return this; }
}
const a = AlterColumnBuilder.create();
const b: AlterColumnBuilder = a;
"#,
    );
    let errs = instance_constructor_family_codes(&diagnostics);
    assert!(
        errs.is_empty(),
        "static create(): InstanceType<typeof this> with `new this()` must type-check on a fluent class.\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}

/// `ReturnType` over a function returning a `this`-member class is the same
/// shape (call-signature infer) and must also resolve.
#[test]
fn return_type_of_function_returning_this_class() {
    let diagnostics = check(
        r#"
class Link {
  next(): this { return this; }
}
declare function makeLink(): Link;
type R = ReturnType<typeof makeLink>;
const a: Link = (null as any as R);
const b: R = (null as any as Link);
"#,
    );
    let errs = instance_constructor_family_codes(&diagnostics);
    assert!(
        errs.is_empty(),
        "ReturnType<typeof makeLink> must relate to Link with a `this`-returning member.\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}

/// A genuinely *free* contextual `this` in a conditional check must still defer
/// and then resolve per the concrete instance — `C` selects the false branch,
/// the `D` subclass selects the true branch. Guards that the narrowed deferral
/// did not over-reach into the free-`this` case.
#[test]
fn free_this_conditional_still_resolves_per_instance() {
    let diagnostics = check(
        r#"
interface HasTag { tag: number }
class Plain {
  describe(): this extends HasTag ? "tagged" : "plain" {
    return ("plain" as unknown) as any;
  }
}
class Tagged extends Plain {
  tag = 1;
}
const p = new Plain();
const t = new Tagged();
const pk: "plain" = p.describe();
const tk: "tagged" = t.describe();
"#,
    );
    let errs = instance_constructor_family_codes(&diagnostics);
    assert!(
        errs.is_empty(),
        "Free `this extends HasTag ? ... : ...` must resolve per concrete instance (Plain -> false branch, Tagged -> true branch).\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}

/// Negative control: a genuine mismatch through `InstanceType<typeof X>` must
/// still report `TS2322`, so the fix did not blanket-suppress the relation.
#[test]
fn instance_type_of_this_class_still_reports_real_mismatch() {
    let diagnostics = check(
        r#"
class Widget {
  refine(): this { return this; }
  size: number = 0;
}
declare const w: InstanceType<typeof Widget>;
const bad: string = w.size;
"#,
    );
    let count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();
    assert_eq!(
        count, 1,
        "A real `number`->`string` mismatch read off InstanceType<typeof Widget> must still raise exactly one TS2322.\nAll: {diagnostics:#?}"
    );
}
