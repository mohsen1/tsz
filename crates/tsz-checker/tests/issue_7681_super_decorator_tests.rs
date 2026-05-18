//! Regression tests for #7681 — false-positive TS2335 on `super` in decorator
//! expressions attached to members of an inner class, plus the follow-up
//! TS1241/TS1270 false positives that surfaced once the structural TS2335
//! rule was fixed in PR #8026.
//!
//! Structural rules:
//!
//! 1. A `super` reference inside a decorator expression on a class member is
//!    evaluated in the scope where the decorated class is *defined*, not
//!    inside the class itself. When walking up from such a `super` to find
//!    the enclosing class for TS2335, the inner class (containing the
//!    decorated member) must be skipped, and validation must resume at the
//!    next enclosing class. — pinned by PR #8026.
//!
//! 2. When the decorator expression is a property or element access
//!    (modulo parentheses), the receiver's type is the implicit `this` of
//!    the synthetic decorator call used for signature validation. The call
//!    resolver must receive that receiver as `actual_this_type` so that
//!    declarations carrying an explicit `this: T` parameter type-check
//!    against the receiver instead of failing the call outright with
//!    TS1241 + TS1270. Bare-identifier decorator expressions still have
//!    no receiver and follow the prior no-`this`-binding behavior. — this
//!    file.
//!
//! Conformance target:
//! `TypeScript/tests/cases/conformance/esDecorators/esDecorators-preservesThis.ts`.

use tsz_checker::test_utils::{
    check_source_code_messages, check_source_with_libs_code_messages,
    check_with_options_code_messages, has_diagnostic_code, load_default_lib_files,
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
fn no_ts2335_for_full_es_decorators_preserves_this_repro_with_libs() {
    // Reproduce the conformance fixture under full TypeScript-lib resolution,
    // matching the runtime conditions where the regression actually fires.
    // The conformance fixture (esDecorators-preservesThis.ts) types the
    // decorator's `ctx` parameter as `DecoratorContext`, which is defined in
    // `decorators.d.ts` and requires lib resolution to bind.
    let source = r#"
declare class DecoratorProvider {
    decorate<T>(this: DecoratorProvider, v: T, ctx: DecoratorContext): T;
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
    let mut libs = load_default_lib_files();
    let decorator_libs = tsz_checker::test_utils::load_lib_files(&["decorators.d.ts"]);
    libs.extend(decorator_libs);
    assert!(!libs.is_empty(), "expected at least one lib file");
    let opts = CheckerOptions {
        target: tsz_common::common::ScriptTarget::ES2022,
        ..CheckerOptions::default()
    };
    let diags = check_source_with_libs_code_messages(source, "test.ts", opts, &libs);
    // The conformance fixture expects zero diagnostics (tsc accepts it cleanly).
    // The historical TS2335 regression was pinned by PR #8026's structural rule;
    // the remaining failure surfaces as TS1241/TS1270 because the decorator
    // call signature check passes `actual_this_type = None`, so `this: T`
    // method-style decorators fail to type-check against their property-access
    // receiver. Assert that none of the regression codes fire.
    let forbidden = [
        diagnostic_codes::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS,
        diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
        diagnostic_codes::DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    ];
    let offenders: Vec<_> = diags
        .iter()
        .filter(|(c, _)| forbidden.contains(c))
        .collect();
    assert!(offenders.is_empty(), "{offenders:?}");
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

// -----------------------------------------------------------------------------
// Adjacent-case test matrix for the `actual_this_type` receiver binding
// (structural rule #2 above). Per §26 of CLAUDE.md, each rule needs at least
// three adjacent shapes that vary the names, the access form, and the
// generic-vs-concrete and method-vs-arrow axes.
// -----------------------------------------------------------------------------

const TS1241: u32 =
    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION;
const TS1270: u32 = diagnostic_codes::DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;

fn assert_no_decorator_call_signature_diagnostics(diags: &[(u32, String)]) {
    let forbidden = [TS2335, TS1241, TS1270];
    let offenders: Vec<_> = diags
        .iter()
        .filter(|(c, _)| forbidden.contains(c))
        .collect();
    assert!(offenders.is_empty(), "{offenders:?}");
}

#[test]
fn es_method_decorator_with_this_param_through_property_access_is_accepted() {
    // Property-access form: `@instance.decorate` where `decorate` declares
    // `this: Provider`. The receiver `instance` provides the `this` binding.
    let source = r#"
declare class Provider {
    decorate<V>(this: Provider, v: V, ctx: any): V;
}
declare const instance: Provider;
class C {
    @instance.decorate
    method() { }
}
"#;
    let diags = check_source_code_messages(source);
    assert_no_decorator_call_signature_diagnostics(&diags);
}

#[test]
fn es_method_decorator_with_this_param_through_element_access_is_accepted() {
    // Element-access form: `@instance["decorate"]`. Same receiver binding rule.
    let source = r#"
declare class Provider {
    decorate<V>(this: Provider, v: V, ctx: any): V;
}
declare const instance: Provider;
class C {
    @(instance["decorate"])
    method() { }
}
"#;
    let diags = check_source_code_messages(source);
    assert_no_decorator_call_signature_diagnostics(&diags);
}

#[test]
fn es_method_decorator_with_this_param_through_nested_parens_is_accepted() {
    // Parenthesized form: `@((instance.decorate))`. Parens must be skipped to
    // recover the property-access receiver.
    let source = r#"
declare class Provider {
    decorate<V>(this: Provider, v: V, ctx: any): V;
}
declare const instance: Provider;
class C {
    @((instance.decorate))
    method() { }
}
"#;
    let diags = check_source_code_messages(source);
    assert_no_decorator_call_signature_diagnostics(&diags);
}

#[test]
fn es_method_decorator_with_this_param_renamed_value_param_is_accepted() {
    // Same rule with the value parameter renamed (`v` → `target`) and the
    // type-parameter renamed (`V` → `R`). Proves the fix is structural — not
    // keyed on identifier spelling (§25).
    let source = r#"
declare class Provider {
    decorate<R>(this: Provider, target: R, ctx: any): R;
}
declare const provider: Provider;
class C {
    @provider.decorate
    method() { }
}
"#;
    let diags = check_source_code_messages(source);
    assert_no_decorator_call_signature_diagnostics(&diags);
}

#[test]
fn es_method_decorator_through_super_on_derived_class_is_accepted() {
    // The super-receiver branch: `@super.decorate` inside an inner class
    // declared in a method of a derived class. The `this` parameter must
    // bind to the outer-class super type. Combines both structural rules.
    let source = r#"
declare class Provider {
    decorate<V>(this: Provider, v: V, ctx: any): V;
}
class D extends Provider {
    m() {
        class C {
            @(super.decorate)
            method() { }
        }
    }
}
"#;
    let diags = check_source_code_messages(source);
    assert_no_decorator_call_signature_diagnostics(&diags);
}

#[test]
fn legacy_method_decorator_with_this_param_through_property_access_is_accepted() {
    // The legacy (`experimentalDecorators`) path also routes through the same
    // call-signature helper; the receiver binding must apply equally.
    let source = r#"
declare class Provider {
    decorate(this: Provider, target: any, key: string): void;
}
declare const instance: Provider;
class C {
    @instance.decorate
    method() { }
}
"#;
    let diags = check_with_options_code_messages(
        source,
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    );
    assert_no_decorator_call_signature_diagnostics(&diags);
}

#[test]
fn legacy_property_decorator_with_this_param_through_property_access_is_accepted() {
    // Legacy property decorators run through
    // `check_legacy_property_decorator_call_signature`. Same rule.
    let source = r#"
declare class Provider {
    decorate(this: Provider, target: any, key: string): void;
}
declare const instance: Provider;
class C {
    @instance.decorate
    field: number = 0;
}
"#;
    let diags = check_with_options_code_messages(
        source,
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    );
    assert_no_decorator_call_signature_diagnostics(&diags);
}

#[test]
fn legacy_parameter_decorator_with_this_param_through_property_access_is_accepted() {
    // Legacy parameter decorators run through
    // `check_parameter_decorator_call_signature`. Same rule.
    let source = r#"
declare class Provider {
    decorate(this: Provider, target: any, key: string | undefined, idx: number): void;
}
declare const instance: Provider;
class C {
    method(@instance.decorate v: number) { }
}
"#;
    let diags = check_with_options_code_messages(
        source,
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    );
    assert_no_decorator_call_signature_diagnostics(&diags);
}

#[test]
fn es_decorator_bare_identifier_with_this_param_still_emits_ts1241() {
    // Negative guard: a bare identifier `@factory` has no syntactic receiver,
    // so the call resolver still validates against the explicit `this: T`
    // without a receiver binding. TS1241 must still fire — otherwise the
    // helper would over-permit decorators that cannot satisfy their `this`.
    let source = r#"
declare const factory: <V>(this: { provider: string }, v: V, ctx: any) => V;
class C {
    @factory
    method() { }
}
"#;
    let diags = check_source_code_messages(source);
    let has_ts1241 = diags.iter().any(|(c, _)| *c == TS1241);
    assert!(
        has_ts1241,
        "Bare identifier decorator with explicit `this` should still emit TS1241: {diags:?}"
    );
}

#[test]
fn es_method_decorator_without_this_param_through_property_access_unchanged() {
    // Baseline: when the decorator method has no explicit `this` parameter,
    // the receiver binding is a no-op and existing behavior is preserved.
    let source = r#"
declare class Provider {
    decorate<V>(v: V, ctx: any): V;
}
declare const instance: Provider;
class C {
    @instance.decorate
    method() { }
}
"#;
    let diags = check_source_code_messages(source);
    assert_no_decorator_call_signature_diagnostics(&diags);
}
