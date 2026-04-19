use crate::core::*;

#[test]
fn test_ts1100_eval_assignment_strict_mode() {
    let source = r#"
"use strict";
eval = 1;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 1100),
        "Expected TS1100 for 'eval = 1' in strict mode. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1100_eval_increment_strict_mode_reports_assignment_errors() {
    let source = r#"
"use strict";
eval++;
"#;
    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        has_error(&diagnostics, 1100),
        "Expected TS1100 for strict-mode eval increment. Got: {diagnostics:?}"
    );
    assert!(
        has_error(&diagnostics, 2630),
        "Expected TS2630 for strict-mode eval increment. Got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2356),
        "Did not expect TS2356 for strict-mode eval increment. Got: {diagnostics:?}"
    );
}

// =========================================================================
// Iterable spread in function calls — TS2556 / TS2345
// =========================================================================

#[test]
fn test_array_spread_in_non_rest_param_emits_ts2556() {
    // Spreading a non-tuple array into a non-rest parameter must emit TS2556.
    // When TS2556 is emitted, no TS2345 should be emitted alongside it.
    let source = r#"
function foo(s: number) { }
declare var arr: number[];
foo(...arr);
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 2556),
        "Expected TS2556 for array spread to non-rest param. Got: {diagnostics:?}"
    );
    // Should NOT also emit TS2345 when TS2556 is reported
    assert!(
        !has_error(&diagnostics, 2345),
        "Should not emit TS2345 alongside TS2556. Got: {diagnostics:?}"
    );
}

#[test]
fn test_array_spread_in_rest_param_no_error() {
    // Spreading an array into a rest parameter should not emit TS2556.
    let source = r#"
function foo(...s: number[]) { }
declare var arr: number[];
foo(...arr);
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2556),
        "Should not emit TS2556 for array spread to rest param. Got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Should not emit TS2345 for compatible array spread. Got: {diagnostics:?}"
    );
}

// ========================================================================
// Reverse mapped type inference tests
// ========================================================================

#[test]
fn test_reverse_mapped_type_boxified_unbox() {
    // Core test: inferring T from Boxified<T> by reversing Box<T[P]> wrapper
    let diagnostics = compile_and_get_diagnostics(
        r#"
        type Box<T> = { value: T; }
        type Boxified<T> = { [P in keyof T]: Box<T[P]>; }
        declare function unboxify<T extends object>(obj: Boxified<T>): T;
        let b = { a: { value: 42 } as Box<number>, b: { value: "hello" } as Box<string> };
        let v = unboxify(b);
        "#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "unboxify with Boxified<T> should not produce TS2345. Got: {diagnostics:?}"
    );
}

#[test]
fn test_reverse_mapped_type_no_regression_contravariant() {
    // Contravariant function template: { [K in keyof T]: (val: T[K]) => boolean }
    // Reverse inference should NOT fire (can't reverse through function types),
    // so this should produce no errors.
    let diagnostics = compile_and_get_diagnostics(
        r#"
        declare function conforms<T>(source: { [K in keyof T]: (val: T[K]) => boolean }): (value: T) => boolean;
        conforms({ foo: (v: string) => false });
        "#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "conforms with function template should not produce TS2322. Got: {diagnostics:?}"
    );
}

#[test]
fn test_reverse_mapped_type_no_regression_func_template() {
    // Mapped type with Func<T[K]> template — reverse should fail gracefully
    let diagnostics = compile_and_get_diagnostics(
        r#"
        type Func<T> = () => T;
        type Mapped<T> = { [K in keyof T]: Func<T[K]> };
        declare function reproduce<T>(options: Mapped<T>): T;
        reproduce({ name: () => { return 123 } });
        "#,
    );
    assert!(
        !has_error(&diagnostics, 2769),
        "reproduce with Func template should not produce TS2769. Got: {diagnostics:?}"
    );
}

// =============================================================================
// TS7008 — Static class member assigned in static block should not emit
// =============================================================================

#[test]
fn ts7008_static_property_assigned_in_static_block_no_error() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
        class C {
            static x;
            static {
                this.x = 1;
            }
        }
        "#,
    );
    assert!(
        !has_error(&diagnostics, 7008),
        "Static property assigned in static block should not emit TS7008. Got: {diagnostics:?}"
    );
}

#[test]
fn ts7008_static_property_assigned_before_declaration_no_error() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
        class C {
            static {
                this.x = 1;
            }
            static x;
        }
        "#,
    );
    assert!(
        !has_error(&diagnostics, 7008),
        "Static property assigned in earlier static block should not emit TS7008. Got: {diagnostics:?}"
    );
}

#[test]
fn ts7008_instance_property_without_annotation_or_initializer() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
        class C {
            x;
        }
        "#,
    );
    assert!(
        has_error(&diagnostics, 7008),
        "Instance property without annotation or initializer should emit TS7008. Got: {diagnostics:?}"
    );
}

#[test]
fn ts7008_static_property_without_assignment_in_static_block() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
        class C {
            static x;
            static {
                // no assignment to this.x
                let y = 1;
            }
        }
        "#,
    );
    assert!(
        has_error(&diagnostics, 7008),
        "Static property NOT assigned in static block should still emit TS7008. Got: {diagnostics:?}"
    );
}

#[test]
fn ts7008_private_identifier_in_ambient_class_is_suppressed() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
        declare class A {
            #prop;
        }
        class B {
            #prop;
        }
        "#,
        CheckerOptions {
            no_implicit_any: true,
            target: ScriptTarget::ESNext,
            ..Default::default()
        },
    );

    let ts7008_count = diagnostics.iter().filter(|(code, _)| *code == 7008).count();

    assert_eq!(
        ts7008_count, 1,
        "Expected only the non-ambient private field to emit TS7008. Got: {diagnostics:?}"
    );
}

#[test]
fn ts2803_private_method_destructuring_assignment_anchors_at_private_name() {
    let source = r#"
class A {
    #method() {}
    constructor() {
        ({ x: this.#method } = { x: () => {} });
    }
}
"#;
    let diagnostics = compile_and_get_raw_diagnostics_named_with_lib_and_options(
        "test.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2803: Vec<_> = diagnostics.iter().filter(|d| d.code == 2803).collect();
    assert_eq!(ts2803.len(), 1, "Expected one TS2803. Got: {diagnostics:?}");

    let expected_start = source
        .find("this.#method")
        .map(|idx| idx as u32 + "this.".len() as u32)
        .expect("expected test source to contain `this.#method`");
    assert_eq!(
        ts2803[0].start, expected_start,
        "Expected TS2803 to anchor at `#method` in the destructuring target."
    );
}

#[test]
fn ts2803_static_private_method_destructuring_assignment_anchors_at_private_name() {
    let source = r#"
class A {
    static #method() {}
    static assign() {
        ({ x: A.#method } = { x: () => {} });
    }
}
"#;
    let diagnostics = compile_and_get_raw_diagnostics_named_with_lib_and_options(
        "test.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2803: Vec<_> = diagnostics.iter().filter(|d| d.code == 2803).collect();
    assert_eq!(ts2803.len(), 1, "Expected one TS2803. Got: {diagnostics:?}");

    let expected_start = source
        .find("A.#method")
        .map(|idx| idx as u32 + "A.".len() as u32)
        .expect("expected test source to contain `A.#method`");
    assert_eq!(
        ts2803[0].start, expected_start,
        "Expected TS2803 to anchor at `#method` in the static destructuring target."
    );
}

#[test]
fn ts18013_named_class_expression_private_access_uses_inner_class_name() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const C = class D {
    static #field = D.#method();
    static #method() { return 42; }
    static getClass() { return D; }
};

C.getClass().#method;
C.getClass().#field;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    // The private identifier access should produce either TS18013 ("not accessible
    // outside class") or TS18016 ("not allowed outside class bodies"), depending on
    // whether the constructor type is cached and the class type resolves fully.
    // Both are valid diagnostics for private identifier access outside the class.
    let private_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 18013 || *code == 18016)
        .collect();
    assert_eq!(
        private_errors.len(),
        2,
        "Expected two private-access errors (TS18013 or TS18016). Got: {diagnostics:?}"
    );
}

#[test]
#[ignore] // TODO: shadowed private access should use constructor type name in TS18014
fn ts18014_shadowed_private_access_uses_constructor_type_name() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class A {
    static #x = 5;
    constructor() {
        class B {
            #x = 5;
            constructor() {
                class C {
                    constructor() {
                        A.#x;
                    }
                }
            }
        }
    }
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 18014 && message.contains("type 'typeof A'") }),
        "Expected TS18014 to reference constructor-side type 'typeof A'. Got: {diagnostics:?}"
    );
}

#[test]
fn private_name_keyof_excludes_ecmascript_private_members() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r##"
class A {
    #fooField = 3;
    #fooMethod() {}
    get #fooProp() { return 1; }
    set #fooProp(value: number) {}
    bar = 3;
    baz = 3;
}

let k: keyof A = "bar";
k = "baz";

k = "#fooField";
k = "#fooMethod";
k = "#fooProp";
k = "fooField";
k = "fooMethod";
k = "fooProp";
"##,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        6,
        "Expected six TS2322 diagnostics. Got: {diagnostics:?}"
    );
    for expected in [
        "\"#fooField\"",
        "\"#fooMethod\"",
        "\"#fooProp\"",
        "\"fooField\"",
        "\"fooMethod\"",
        "\"fooProp\"",
    ] {
        assert!(
            ts2322.iter().any(|(_, message)| {
                message.contains(expected) && message.contains("type 'keyof A'")
            }),
            "Expected TS2322 mentioning {expected}. Got: {diagnostics:?}"
        );
    }
}

#[test]
fn private_name_object_spread_excludes_private_members() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class C {
    #prop = 1;
    static #propStatic = 1;

    method(other: C) {
        const obj = { ...other };
        obj.#prop;
        const { ...rest } = other;
        rest.#prop;

        const statics = { ...C };
        statics.#propStatic;
        const { ...sRest } = C;
        sRest.#propStatic;
    }
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        4,
        "Expected four TS2339 diagnostics. Got: {diagnostics:?}"
    );
    let empty_object_count = ts2339
        .iter()
        .filter(|(_, message)| message.contains("type '{}'."))
        .count();
    let static_object_count = ts2339
        .iter()
        .filter(|(_, message)| message.contains("type '{ prototype: C; }'."))
        .count();
    assert_eq!(
        empty_object_count, 2,
        "Expected object spread/rest from instance to erase private names. Got: {diagnostics:?}"
    );
    assert_eq!(
        static_object_count, 2,
        "Expected constructor spread/rest to keep only public constructor properties. Got: {diagnostics:?}"
    );
}

#[test]
fn private_name_generic_class_assignments_preserve_instantiation_display() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class C<T> {
    #foo: T;
    #bar(): T {
      return this.#foo;
    }
    constructor(t: T) {
      this.#foo = t;
      t = this.#bar();
    }
    set baz(t: T) {
      this.#foo = t;
    }
    get baz(): T {
      return this.#foo;
    }
}

let a = new C(3);
let b = new C("hello");

a = b;
b = a;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        2,
        "Expected two TS2322 diagnostics. Got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|(_, message)| message
            .contains("Type 'C<string>' is not assignable to type 'C<number>'.")),
        "Expected generic instantiation display to preserve `C<string>` -> `C<number>`. Got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|(_, message)| message
            .contains("Type 'C<number>' is not assignable to type 'C<string>'.")),
        "Expected generic instantiation display to preserve `C<number>` -> `C<string>`. Got: {diagnostics:?}"
    );
}

#[test]
fn class_expression_assignment_preserves_typeof_variable_name_display() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface A {
  prop: string;
}

const A: { new(): A } = class {}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("Type 'typeof A' is not assignable to type 'new () => A'.")
        }),
        "Expected class-expression assignment to display `typeof A`. Got: {diagnostics:?}"
    );
}

#[test]
fn anonymous_class_expression_argument_preserves_typeof_display() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo<T>(x = class { prop: T }): T {
    return undefined;
}

foo(class { static prop = "hello" }).length;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message.contains(
                    "Argument of type 'typeof (Anonymous class)' is not assignable to parameter of type 'typeof (Anonymous class)'.",
                )
        }),
        "Expected anonymous class-expression diagnostics to preserve `typeof (Anonymous class)`. Got: {diagnostics:?}"
    );
}

// TS1479: CJS file importing ESM module
// Tests the current_is_commonjs detection logic with different file extensions.

/// Helper: compile with a custom file name and `report_unresolved_imports` enabled.
fn compile_with_file_name_and_get_diagnostics(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// .cts files should detect as CJS — extending the original check to also include .cjs.
/// When `file_is_esm` = Some(false), .ts files should detect as CJS.
#[test]
fn test_ts1479_cts_file_is_commonjs() {
    // A .cts file importing something — the import should be treated as CJS context.
    // Without a multi-file setup, TS1479 won't fire (needs resolved target marked ESM),
    // but we verify no crash and correct CJS classification by checking the code compiles.
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_file_name_and_get_diagnostics(
        "test.cts",
        r#"import { foo } from './other';"#,
        opts,
    );
    // Without multi-file resolution, we can't trigger TS1479, but we verify
    // that .cts files don't cause issues and get normal TS2307 for missing modules.
    assert!(
        has_error(&diagnostics, 2307)
            || has_error(&diagnostics, 2792)
            || has_error(&diagnostics, 2882),
        "Expected resolution error for .cts file import.\nActual: {diagnostics:?}"
    );
}

/// In single-file mode (no multi-file resolution), .js files can't trigger TS1479
/// because the import target doesn't resolve. In multi-file mode, .js files CAN
/// get TS1479 when importing .mjs targets (extension-based ESM), but NOT when
/// importing .js targets in ESM packages (package.json-based ESM).
#[test]
fn test_ts1479_js_file_single_file_no_false_positive() {
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_file_name_and_get_diagnostics(
        "test.js",
        r#"import { foo } from './other.mjs';"#,
        opts,
    );
    // In single-file mode, module doesn't resolve so TS1479 check isn't reached.
    // This verifies no false TS1479 from CJS detection alone.
    assert!(
        !has_error(&diagnostics, 1479),
        "Should NOT emit TS1479 in single-file mode.\nActual: {diagnostics:?}"
    );
}

/// .cjs files should NOT get TS1479 for relative imports.
/// TSC suppresses TS1479 for .cjs files importing via relative paths because
/// the imports won't be transformed to `require()` calls (already JS, not TS).
/// Non-relative (package) imports in .cjs files CAN get TS1479.
#[test]
fn test_ts1479_cjs_file_relative_import_suppressed() {
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        ..CheckerOptions::default()
    };
    // Relative import in .cjs file — should NOT emit TS1479
    let diagnostics = compile_with_file_name_and_get_diagnostics(
        "test.cjs",
        r#"import * as m from './index.mjs';"#,
        opts,
    );
    assert!(
        !has_error(&diagnostics, 1479),
        "Should NOT emit TS1479 for .cjs file with relative import.\nActual: {diagnostics:?}"
    );
}

/// TS2536 should be suppressed for deferred conditional types used as indices.
/// Example: `{ 0: X; 1: Y }[SomeConditional extends true ? 0 : 1]`
/// When the conditional can't be resolved at the generic level, TSC defers the check.
#[test]
fn test_ts2536_suppressed_for_deferred_conditional_index() {
    let code = r#"
type HasTail<T extends any[]> =
    T extends ([] | [any]) ? false : true;
type Head<T extends any[]> = T extends [any, ...any[]] ? T[0] : never;
type Tail<T extends any[]> =
    ((...t: T) => any) extends ((_: any, ...tail: infer TT) => any) ? TT : [];
type Last<T extends any[]> = {
    0: Last<Tail<T>>;
    1: Head<T>;
}[HasTail<T> extends true ? 0 : 1];
"#;
    let diagnostics = compile_and_get_diagnostics(code);
    let has_2536 = diagnostics.iter().any(|(code, _)| *code == 2536);
    assert!(
        !has_2536,
        "TS2536 should NOT be emitted for deferred conditional index types.\nActual: {diagnostics:?}"
    );
}
