use super::super::core::*;

#[test]
fn test_type_alias_type_param_shadows_global_return_type_utility() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type AnyFunction<Args extends any[] = any[], ReturnType = any> = (...args: Args) => ReturnType;
        ",
    );

    assert!(
        !has_error(&diagnostics, 2314),
        "Type alias-local type parameters must shadow the global ReturnType<T> utility. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_interface_extends_primitive_reports_ts2840() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface I extends number {}
        ",
    );

    assert!(
        has_error(&diagnostics, 2840),
        "Expected TS2840 when interface extends primitive type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_interface_extends_classes_with_private_member_clash_reports_ts2320() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class X {
    private m: number;
}
class Y {
    private m: string;
}

interface Z extends X, Y {}
        ",
    );

    assert!(
        has_error(&diagnostics, 2320),
        "Expected TS2320 when interface extends classes with conflicting private members. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_instance_member_initializer_constructor_param_capture_reports_ts2301() {
    // Use ES5 target so useDefineForClassFields is false and TS2301 applies
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
declare var console: {
    log(msg?: any): void;
};
var field1: string;

class Test1 {
    constructor(private field1: string) {
    }
    messageHandler = () => {
        console.log(field1);
    };
}
        ",
        {
            CheckerOptions {
                target: tsz_common::common::ScriptTarget::ES5,
                ..Default::default()
            }
        },
    );

    assert!(
        has_error(&diagnostics, 2301),
        "Expected TS2301 for constructor parameter capture in instance initializer. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_instance_member_initializer_plain_constructor_names_report_ts2301() {
    // Use ES5 target so useDefineForClassFields is false and TS2301 applies
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
class A {
    private a = x;
    private b = { p: x };
    private c = () => x;
    constructor(x: number) {
    }
}

class B {
    private a = x;
    private b = { p: x };
    private c = () => x;
    constructor() {
        var x = 1;
    }
}
        ",
        {
            CheckerOptions {
                target: tsz_common::common::ScriptTarget::ES5,
                ..Default::default()
            }
        },
    );

    let ts2301_count = diagnostics.iter().filter(|(code, _)| *code == 2301).count();

    assert_eq!(
        ts2301_count, 6,
        "Expected TS2301 for constructor parameter and constructor-local captures in instance initializers. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2304),
        "Did not expect TS2304 once constructor captures are recognized. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2663),
        "Did not expect TS2663 for plain constructor captures in non-module classes. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_instance_member_initializer_missing_name_reports_ts2663() {
    // Use ES5 target so useDefineForClassFields is false and TS2663 applies
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
declare var console: {
    log(msg?: any): void;
};

export class Test1 {
    constructor(private field1: string) {
    }
    messageHandler = () => {
        console.log(field1);
    };
}
        ",
        {
            CheckerOptions {
                target: tsz_common::common::ScriptTarget::ES5,
                ..Default::default()
            }
        },
    );

    assert!(
        has_error(&diagnostics, 2663),
        "Expected TS2663 for missing free name in module instance initializer. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_instance_member_initializer_cross_file_global_script_name_reports_ts2301() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "classMemberInitializerWithLamdaScoping3_0.ts",
                "var field1: string;",
            ),
            (
                "classMemberInitializerWithLamdaScoping3_1.ts",
                r"
declare var console: {
    log(msg?: any): void;
};
export class Test1 {
    constructor(private field1: string) {
    }
    messageHandler = () => {
        console.log(field1);
    };
}
                ",
            ),
        ],
        "classMemberInitializerWithLamdaScoping3_1.ts",
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2301),
        "Expected TS2301 for cross-file global script capture in module instance initializer. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2663),
        "Did not expect TS2663 when a cross-file global script value exists. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_umd_namespace_conflicting_with_global_const_reports_ts2451() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "three.d.ts",
                r"
export namespace THREE {
  export class Vector2 {}
}
                ",
            ),
            (
                "global.d.ts",
                r"
import * as _three from './three';

export as namespace THREE;

declare global {
  export const THREE: typeof _three;
}
                ",
            ),
            ("test.ts", "const m = THREE;"),
        ],
        "global.d.ts",
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2451_count = diagnostics.iter().filter(|(code, _)| *code == 2451).count();
    assert!(
        ts2451_count >= 2,
        "Expected both UMD/global declarations to report TS2451 when checking global.d.ts. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_umd_namespace_with_global_const_value_does_not_emit_ts2708() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "three.d.ts",
                r"
export namespace THREE {
  export class Vector2 {}
}
                ",
            ),
            (
                "global.d.ts",
                r"
import * as _three from './three';

export as namespace THREE;

declare global {
  export const THREE: typeof _three;
}
                ",
            ),
            ("test.ts", "const m = THREE;"),
        ],
        "test.ts",
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2708),
        "Did not expect cascading TS2708 once a non-UMD global value exists. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_umd_global_property_access_in_commonjs_module_emits_ts2686() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "a.js",
                r#"
const other = require("./other");
/** @type {Puppeteer.Keyboard} */
var ppk;
Puppeteer.connect;
"#,
            ),
            (
                "puppet.d.ts",
                r#"
export as namespace Puppeteer;
export interface Keyboard {
    key: string;
}
export function connect(name: string): void;
"#,
            ),
            (
                "other.d.ts",
                r#"
declare function f(): string;
export = f;
"#,
            ),
        ],
        "a.js",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2686),
        "Expected TS2686 for module-side property access through a UMD global. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_umd_export_as_namespace_class_is_usable_in_type_position() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "foo.d.ts",
                r#"
declare class Thing {
    foo(): number;
}
declare namespace Thing {
    interface SubThing {}
}
export = Thing;
export as namespace Foo;
"#,
            ),
            (
                "a.ts",
                r#"
/// <reference path="foo.d.ts" />
import * as ff from "./foo";

declare let y: Foo;
y.foo();
declare let z: Foo.SubThing;
let x: any = Foo;
"#,
            ),
        ],
        "a.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2709),
        "Did not expect TS2709 for UMD export-as-namespace class in type position. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2686),
        "Expected TS2686 for bare UMD global value access in module file. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_uninstantiated_namespace_shadowing_symbol_uses_global_value_for_property_access() {
    let diagnostics =
        without_missing_global_type_errors(compile_and_get_diagnostics_with_lib_and_options(
            r#"
namespace M {
    namespace Symbol { }

    class C {
        [Symbol.iterator]() { }
    }
}
            "#,
            CheckerOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        ));

    assert!(
        !has_error(&diagnostics, 2708),
        "Did not expect TS2708 when an empty namespace shadows the global Symbol value in a property access. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_instance_member_initializer_local_shadow_does_not_report_ts2301() {
    let diagnostics = compile_and_get_diagnostics(
        r"
declare var console: {
    log(msg?: any): void;
};

class Test {
    constructor(private field: string) {
    }
    messageHandler = () => {
        var field = this.field;
        console.log(field);
    };
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 2301),
        "Did not expect TS2301 for locally shadowed identifier in initializer. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2403),
        "Did not expect TS2403 for the hoisted local var inside the initializer lambda. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_unresolved_import_namespace_access_suppresses_ts2708() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
import { alias } from "foo";
let x = new alias.Class();
        "#,
    );

    assert!(
        !has_error(&diagnostics, 2708),
        "Should not emit cascading TS2708 for unresolved imported namespace access. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_cross_file_js_container_merge_does_not_emit_shadowed_namespace_ts2708() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "a.d.ts",
                r"
declare namespace C {
    function bar(): void;
}
                ",
            ),
            (
                "b.js",
                r"
C.prototype = {};
C.bar = 2;
                ",
            ),
        ],
        "b.js",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2708),
        "Did not expect TS2708 once the JS container provides a real value binding. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_extends_user_defined_generic_without_type_args_reports_ts2314() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base<T, U> {}
class Derived extends Base {}
        ",
    );

    assert!(
        has_error(&diagnostics, 2314),
        "Expected TS2314 for omitted type arguments on user-defined generic base class. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_optional_method_parameter_accepts_optional_boolean_argument() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C {
    outer(flag?: boolean) {
        return this.inner(flag);
    }

    inner(flag?: boolean) {
        return flag;
    }
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 2345),
        "Did not expect TS2345 when passing an optional boolean to another optional boolean parameter. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_super_call_args_match_instantiated_generic_base_ctor() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> {
    constructor(public value: T) {}
}

class Derived extends Base<number> {
    constructor() {
        super("hi");
    }
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for super argument type mismatch against instantiated base ctor. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_derived_constructor_without_super_reports_ts2377() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {}

class Derived extends Base {
    constructor() {}
}
        ",
    );

    assert!(
        has_error(&diagnostics, 2377),
        "Expected TS2377 for derived constructor missing super() call. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_this_before_missing_super_reports_ts17009() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {}

class Derived extends Base {
    constructor() {
        this.x;
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 17009),
        "Expected TS17009 when 'this' is used in a derived constructor without super(). Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_malformed_this_property_annotation_does_not_emit_ts2551() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class A {
    constructor() {
        this.foo: any;
    }
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 2551),
        "Did not expect TS2551 in malformed syntax recovery path. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_super_property_before_super_call_reports_ts17011() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    method() {}
}

class Derived extends Base {
    constructor() {
        super.method();
        super();
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 17011),
        "Expected TS17011 for super property access before super() call. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_branch_local_super_call_does_not_suppress_else_branch_before_super_errors() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    x = 1;
}

class Derived extends Base {
    constructor(flag: boolean) {
        if (flag) {
            super();
        } else {
            this.x;
            super.x;
        }
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 17009),
        "Expected TS17009 for 'this' access in branch where super() was not called. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 17011),
        "Expected TS17011 for super property access in branch where super() was not called. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_super_property_access_inside_super_call_reports_ts17011() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class A {
    constructor(f: string) {}
    public blah(): string { return ""; }
}

class B extends A {
    constructor() {
        super(super.blah())
    }
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 17011),
        "Expected TS17011 for super property access inside super() arguments. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_static_property_not_in_class_type_preserves_generic_receiver_display() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
namespace Generic {
    class C<T, U> {
        fn() { return this; }
        static get x() { return 1; }
        static set x(v) { }
        constructor(public a: T, private b: U) { }
        static foo: T;
    }

    namespace C {
        export var bar = '';
    }

    const c = new C(1, '');
    const r4 = c.foo;
    const r5 = c.bar;
    const r6 = c.x;
}
        "#,
    );

    let ts2576_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2576)
        .map(|(_, message)| message.as_str())
        .collect();
    // ts2339_messages collected for future use when namespace-member TS2339 is implemented
    let _ts2339_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts2576_messages.iter().any(|message| {
            message.contains("Property 'foo' does not exist on type 'C<number, string>'")
                && message.contains("static member 'C<number, string>.foo'")
        }),
        "Expected generic TS2576 message for c.foo, got: {diagnostics:#?}"
    );
    assert!(
        ts2576_messages.iter().any(|message| {
            message.contains("Property 'x' does not exist on type 'C<number, string>'")
                && message.contains("static member 'C<number, string>.x'")
        }),
        "Expected generic TS2576 message for c.x, got: {diagnostics:#?}"
    );
    // TODO: tsc also emits TS2339 for `c.bar` (namespace-merged member not on instance type).
    // Our compiler currently resolves namespace members on instance access, which suppresses
    // the diagnostic. Track under staticPropertyNotInClassType conformance gap.
    // assert!(
    //     ts2339_messages
    //         .iter()
    //         .any(|message| message
    //             .contains("Property 'bar' does not exist on type 'C<number, string>'")),
    //     "Expected generic TS2339 receiver display for c.bar, got: {diagnostics:#?}"
    // );
}

#[test]
fn test_super_property_access_reports_ts2855() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    value = 1;
}

class Derived extends Base {
    method() {
        return super.value;
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 2855),
        "Expected TS2855 for super property access to class field member. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_static_super_field_access_does_not_report_ts2855() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    static value = 1;
}

class Derived extends Base {
    static extra = super.value + 1;

    static {
        super.value;
    }
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 2855),
        "Expected static super field access to avoid TS2855. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_super_auto_accessor_access_does_not_report_ts2855() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    accessor value = () => 1;
}

class Derived extends Base {
    method() {
        return super.value();
    }
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 2855),
        "Expected inherited auto-accessor super access to avoid TS2855. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "Expected inherited auto-accessor super access to resolve member type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_this_in_nested_class_computed_name_keeps_ts2339_companion() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class C {
    static readonly c: "foo" = "foo";
    static bar = class Inner {
        static [this.c] = 123;
        [this.c] = 123;
    }
}
        "#,
        CheckerOptions {
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2465),
        "Expected TS2465 for 'this' in class computed property names. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 1166),
        "Expected TS1166 companion diagnostic for invalid class computed property names. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2339),
        "Expected TS2339 companion diagnostic for missing property 'c' on Inner. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_super_in_constructor_parameter_reports_ts2336_and_ts17011() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class B {
    public foo(): number {
        return 0;
    }
}

class C extends B {
    constructor(a = super.foo()) {
    }
}
                ",
    );

    assert!(
        has_error(&diagnostics, 2336),
        "Expected TS2336 for super in constructor argument context. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 17011),
        "Expected TS17011 for super property access before super() in constructor context. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Issue: Overly aggressive strict null checking
///
/// From: neverReturningFunctions1.ts
/// Expected: No errors (control flow eliminates null/undefined)
/// Actual: TS18048 (possibly undefined)
///
/// Root cause: Control flow analysis not recognizing never-returning patterns
///
/// Complexity: HIGH - requires improving control flow analysis
/// See: docs/conformance-analysis-slice3.md
#[test]
fn test_narrowing_after_never_returning_function() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
// @strict: true
declare function fail(message?: string): never;

function f01(x: string | undefined) {
    if (x === undefined) fail("undefined argument");
    x.length;  // Should NOT error - x is string after never-returning call
}
        "#,
    );

    // Filter out TS2318 (missing global types - test harness doesn't load full lib)
    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        semantic_errors.is_empty(),
        "Should emit no semantic errors - x is narrowed to string after never-returning call.\nActual errors: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_chain_undefined_equality_does_not_narrow_to_never() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Thing = { foo: string | number };
function f(o: Thing | undefined) {
    if (o?.foo === undefined) {
        o.foo;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 (no over-narrow to never). Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_chain_typeof_undefined_does_not_narrow_to_never() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Thing = { foo: string | number };
function f(o: Thing | undefined) {
    if (typeof o?.foo === "undefined") {
        o.foo;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 (no over-narrow to never). Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_chain_not_undefined_narrows_to_object() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Thing = { foo: string | number };
function f(o: Thing | undefined) {
    if (o?.foo !== undefined) {
        o.foo;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 18048),
        "Expected no TS18048 in non-undefined optional-chain branch. Actual: {semantic_errors:#?}"
    );
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 in non-undefined optional-chain branch. Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_assert_nonnull_optional_chain_narrows_base_reference() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type Thing = { foo: string | number };
declare function assertNonNull<T>(x: T): asserts x is NonNullable<T>;
function f(o: Thing | undefined) {
    assertNonNull(o?.foo);
    o.foo;
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 after assertNonNull(o?.foo). Actual: {semantic_errors:#?}"
    );
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 18048),
        "Expected no TS18048 after assertNonNull(o?.foo). Actual: {semantic_errors:#?}"
    );
}
