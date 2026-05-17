//! Regression tests for #7681 — false-positive TS2335 on `super` in decorator
//! expressions attached to members of an inner class.
//!
//! Structural rule: a `super` reference inside a decorator expression on a
//! class member is evaluated in the scope where the decorated class is
//! *defined*, not inside the class itself. When walking up from such a
//! `super` to find the enclosing class for TS2335, the inner class
//! (containing the decorated member) must be skipped, and validation must
//! resume at the next enclosing class.
//!
//! Conformance target:
//! `TypeScript/tests/cases/conformance/esDecorators/esDecorators-preservesThis.ts`.

use tsz_checker::test_utils::{
    check_source_code_messages, check_with_options_code_messages, has_diagnostic_code,
};
use tsz_common::checker_options::CheckerOptions;
use tsz_common::diagnostics::diagnostic_codes;

const TS2335: u32 = diagnostic_codes::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS;

#[test]
fn no_ts2335_for_super_decorator_on_inner_class_expression_member() {
    let source = r#"
declare class Base {
    deco<T>(this: Base, v: T, ctx: any): T;
}

class Outer extends Base {
    method() {
        const cls = class {
            @(super.deco)
            inner() { }
        };
        return cls;
    }
}
"#;
    let diags = check_source_code_messages(source);
    assert!(!has_diagnostic_code(&diags, TS2335), "{diags:?}");
}

#[test]
fn no_ts2335_for_super_decorator_on_inner_class_getter_setter_accessor() {
    let source = r#"
declare class Base {
    deco<T>(this: Base, v: T, ctx: any): T;
}

class Outer extends Base {
    method() {
        class Inner {
            @(super.deco)
            get prop() { return 1; }

            @(super.deco)
            set prop(_v: number) {}

            @(super.deco)
            accessor field = 0;
        }
    }
}
"#;
    let diags = check_source_code_messages(source);
    assert!(!has_diagnostic_code(&diags, TS2335), "{diags:?}");
}

#[test]
fn no_ts2335_for_super_decorator_in_nested_arrow_invocation() {
    // Arrows are transparent to super-scope; the decorator-skip rule must still
    // resolve through both the arrow and the inner-class boundary.
    let source = r#"
declare class Base {
    deco<T>(this: Base, v: T, ctx: any): T;
}

class Outer extends Base {
    method() {
        class Inner {
            @((() => super.deco)())
            inner() { }
        }
    }
}
"#;
    let diags = check_source_code_messages(source);
    assert!(!has_diagnostic_code(&diags, TS2335), "{diags:?}");
}

#[test]
fn ts2335_still_fires_for_super_in_non_derived_inner_class_method_body() {
    // Negative guard: the decorator-skip rule must not bleed into the method
    // body of an inner class. `super` in Inner.method's body belongs to Inner,
    // which is not derived, so TS2335 must fire.
    let source = r#"
declare class Base {
    f: number;
}

class Outer extends Base {
    method() {
        class Inner {
            method() {
                return super.f;
            }
        }
    }
}
"#;
    let diags = check_source_code_messages(source);
    assert!(has_diagnostic_code(&diags, TS2335), "{diags:?}");
}

#[test]
fn ts2335_fires_for_super_in_non_derived_outermost_class() {
    // Negative guard: if no enclosing class is derived, TS2335 must fire even
    // when the decorator-skip rule promotes lookup past the inner class.
    let source = r#"
class NotDerived {
    m() {
        class Inner {
            @(super.toString)
            inner() { }
        }
    }
}
"#;
    let diags = check_source_code_messages(source);
    assert!(has_diagnostic_code(&diags, TS2335), "{diags:?}");
}

#[test]
fn no_ts2335_for_super_decorator_under_experimental_decorators() {
    // Legacy (experimentalDecorators) decorator semantics also route through
    // `check_super_expression`; the same skip-rule must apply.
    let source = r#"
declare class Base {
    deco(target: any, key: string): void;
}

class Outer extends Base {
    method() {
        class Inner {
            @(super.deco)
            inner() { }
        }
    }
}
"#;
    let diags = check_with_options_code_messages(
        source,
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    );
    assert!(!has_diagnostic_code(&diags, TS2335), "{diags:?}");
}

#[test]
fn no_ts2335_for_full_es_decorators_preserves_this_repro() {
    // Faithful reproduction of the conformance test fixture; covers the
    // property-access, element-access, and double-parenthesized forms inline.
    let source = r#"
declare class DecoratorProvider {
    decorate<T>(this: DecoratorProvider, v: T, ctx: any): T;
}

declare const instance: DecoratorProvider;

class C {
    @instance.decorate
    method1() { }

    @(instance["decorate"])
    method2() { }

    @((instance.decorate))
    method3() { }
}

class D extends DecoratorProvider {
    m() {
        class C {
            @(super.decorate)
            method1() { }

            @(super["decorate"])
            method2() { }

            @((super.decorate))
            method3() { }
        }
    }
}
"#;
    let diags = check_source_code_messages(source);
    assert!(!has_diagnostic_code(&diags, TS2335), "{diags:?}");
}
