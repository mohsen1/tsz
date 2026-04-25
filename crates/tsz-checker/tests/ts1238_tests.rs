//! Tests for TS1238: Unable to resolve signature of class decorator when called as an expression.

#[test]
fn ts1238_class_used_as_decorator_emits_error() {
    // A class has construct signatures but no call signatures,
    // so using it as a decorator should emit TS1238.
    let codes = tsz_checker::test_utils::check_source_codes_experimental_decorators(
        r#"
class Decorate { }
@Decorate
class C { }
"#,
    );
    assert!(
        codes.contains(&1238),
        "Expected TS1238 when a class (no call signatures) is used as decorator, got: {codes:?}"
    );
}

#[test]
fn ts1238_function_decorator_no_error() {
    // A function declaration has a call signature, so no TS1238 should be emitted.
    let codes = tsz_checker::test_utils::check_source_codes_experimental_decorators(
        r#"
function decorate(target: any) { }
@decorate
class C { }
"#,
    );
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 for a function decorator, got: {codes:?}"
    );
}

#[test]
fn ts1238_declared_function_decorator_no_error() {
    // Declared function has a call signature — no TS1238.
    let codes = tsz_checker::test_utils::check_source_codes_experimental_decorators(
        r#"
declare function decorate(target: any): any;
@decorate
class C { }
"#,
    );
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 for a declared function decorator, got: {codes:?}"
    );
}

#[test]
fn ts1238_not_emitted_for_any_type() {
    // If the decorator expression has type `any`, no TS1238 — tsc allows it.
    let codes = tsz_checker::test_utils::check_source_codes_experimental_decorators(
        r#"
declare var dec: any;
@dec
class C { }
"#,
    );
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 for `any`-typed decorator, got: {codes:?}"
    );
}

#[test]
fn ts1238_not_emitted_for_any_decorator_on_class_with_static_this_members() {
    let codes = tsz_checker::test_utils::check_source_codes_experimental_decorators(
        r#"
declare const foo: any;

@foo
class C {
    static a = 1;
    static b = this.a + 1;
}

@foo
class D extends C {
    static c = 2;
    static d = this.c + 1;
    static e = super.a + this.c + 1;
    static f = () => this.c + 1;
    static ff = function () { this.c + 1 }
    static foo () {
        return this.c + 1;
    }
}
"#,
    );
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 for any-typed decorators around static-this members, got: {codes:?}"
    );
}

#[test]
fn ts1238_not_emitted_for_any_decorator_on_class_with_static_method_name_collision() {
    let codes = tsz_checker::test_utils::check_source_codes_experimental_decorators(
        r#"
declare const foo: any;

@foo
class D {
    static foo () {
        return 1;
    }
}
"#,
    );
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 when a decorated class has a same-named static method, got: {codes:?}"
    );
}

#[test]
fn ts1238_not_emitted_for_any_decorator_on_class_with_static_this_only() {
    let codes = tsz_checker::test_utils::check_source_codes_experimental_decorators(
        r#"
declare const foo: any;

@foo
class C {
    static a = 1;
    static b = this.a + 1;
}
"#,
    );
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 for any-typed decorators around static-this members, got: {codes:?}"
    );
}

#[test]
fn ts1238_not_emitted_without_experimental_decorators() {
    // Without experimentalDecorators, TS1238 should not be emitted.
    let codes =
        tsz_checker::test_utils::check_source_codes("class Decorate { }\n@Decorate\nclass C { }");
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 without experimentalDecorators, got: {codes:?}"
    );
}

#[test]
fn ts1238_generic_decorator_call_emits_error() {
    let codes = tsz_checker::test_utils::check_source_codes_experimental_decorators(
        r#"
interface I<T> {
    prototype: T,
    m: () => T
}
function dec<T>(c: I<T>) { }

@dec
class C {
    _brand: any;
    static m() {}
}
"#,
    );
    assert!(
        codes.contains(&1238),
        "Expected TS1238 for generic decorator with incompatible call, got: {codes:?}"
    );
}

// === ES decorators (experimental_decorators: false) =======================
//
// ES decorators call the decorator factory with `(value, context)`.
// A factory with zero parameters has no slot for `value`, so tsc flags
// it as TS1238 even though a structural call would succeed by ignoring
// the extra args. A factory requiring more than two parameters also
// cannot be satisfied. 1 or 2 required parameters are fine.

#[test]
fn ts1238_es_decorator_zero_arity_factory_emits_error() {
    // `() => {}` has no parameter to receive the class target.
    let codes = tsz_checker::test_utils::check_source_codes("@(() => {})\nclass C {}\n");
    assert!(
        codes.contains(&1238),
        "Expected TS1238 for zero-arity ES class decorator, got: {codes:?}"
    );
}

#[test]
fn ts1238_es_decorator_one_or_two_required_params_no_error() {
    for source in [
        "@((a: any) => {})\nclass C {}\n",
        "@((a: any, b: any) => {})\nclass C {}\n",
    ] {
        let codes = tsz_checker::test_utils::check_source_codes(source);
        assert!(
            !codes.contains(&1238),
            "Should not emit TS1238 for 1 or 2 required params, got: {codes:?} for {source}"
        );
    }
}

#[test]
fn ts1238_es_decorator_too_many_required_params_emits_error() {
    for source in [
        "@((a: any, b: any, c: any) => {})\nclass C {}\n",
        "@((a: any, b: any, c: any, ...d: any[]) => {})\nclass C {}\n",
    ] {
        let codes = tsz_checker::test_utils::check_source_codes(source);
        assert!(
            codes.contains(&1238),
            "Expected TS1238 for >2 required params, got: {codes:?} for {source}"
        );
    }
}
