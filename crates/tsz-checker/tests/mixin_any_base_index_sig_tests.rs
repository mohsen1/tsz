//! When a class extends a type parameter whose constructor constraint returns
//! `any`, TypeScript adds `[x: string]: any` to the derived class's instance
//! type.  This lets callers index into the instance with any string key.
//!
//! Structural rule: "When `class C extends base` where `base: T` and the
//! constraint of `T` is a constructor returning `any`, the instance type of `C`
//! gains an implicit `[x: string]: any` string-index signature."
//!
//! Adjacent cases covered here to verify the rule is not spelling-specific:
//!   1. `TBase extends new (...args: any[]) => any` (non-abstract)
//!   2. `TParam extends abstract new (...args: any[]) => any` (abstract)
//!   3. Class body with explicit instance members alongside the index sig
//!   4. Renamed type parameter (`MyBase`, `X`) to prove no name-dependency

use tsz_checker::test_utils::check_source_codes;

fn assert_no_ts2339(source: &str) {
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 (property access valid via [x: string]: any), got codes: {codes:?}"
    );
}

#[test]
fn direct_any_base_does_not_synthesize_jsx_props_index_signature() {
    use tsz_checker::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;

    let diagnostics = tsz_checker::test_utils::check_source(
        r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    interface ElementAttributesProperty { props: {} }
}

declare var Base: any;
class Comp extends Base {}
let el = <Comp x={1} />;
"#,
        "test.tsx",
        CheckerOptions {
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.contains(&2607),
        "Expected TS2607 for JSX attributes on a class without props; got codes: {codes:?}"
    );
}

#[test]
fn non_abstract_ctor_returning_any_no_ts2339_on_dynamic_property() {
    assert_no_ts2339(
        r#"
function Mixin<TBase extends new (...args: any[]) => any>(base: TBase) {
    class Mixed extends base {
        method(): void {}
    }
    return Mixed;
}
class Base {}
const M = Mixin(Base);
const m = new M();
const _ = m.unknownProp;
"#,
    );
}

#[test]
fn direct_ctor_returning_any_no_ts2339_on_dynamic_property() {
    assert_no_ts2339(
        r#"
declare const Base: new (...args: any[]) => any;
class Derived extends Base {
    known(): void {}
}
const d = new Derived();
const _ = d.dynamic;
"#,
    );
}

#[test]
fn renamed_type_param_no_ts2339_on_dynamic_property() {
    assert_no_ts2339(
        r#"
function Mixin<TParam extends new (...args: any[]) => any>(base: TParam) {
    class Mixed extends base {
        greet(): string { return "hi"; }
    }
    return Mixed;
}
class Root {}
const M = Mixin(Root);
const m = new M();
const _ = m.dynamicKey;
"#,
    );
}

#[test]
fn abstract_ctor_returning_any_no_ts2339_on_dynamic_property() {
    assert_no_ts2339(
        r#"
function Mixin<X extends abstract new (...args: any[]) => any>(base: X) {
    abstract class Mixed extends base {
        method(): void {}
    }
    return Mixed;
}
abstract class AbstractBase {}
const M = Mixin(AbstractBase);
"#,
    );
}

#[test]
fn class_with_methods_and_any_base_no_ts2339() {
    assert_no_ts2339(
        r#"
function WithMethods<MyBase extends new (...args: any[]) => any>(base: MyBase) {
    class Enhanced extends base {
        foo(): number { return 1; }
        bar(): string { return ""; }
    }
    return Enhanced;
}
class Seed {}
const E = WithMethods(Seed);
const e = new E();
const _ = e.someDynamicProp;
"#,
    );
}
