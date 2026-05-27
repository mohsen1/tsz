#[test]
fn mapped_type_generic_indexed_access_class_member() {
    // Repro from TypeScript#49242: accessing a mapped type class member
    // with a generic key derived from the same keyof should work.
    let source = r#"
type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
};

class Test {
    entries: { [T in keyof Types]?: Types[T][] };
    constructor() { this.entries = {}; }
    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    // Should not emit TS2349 (not callable) for .push() call
    assert!(
        !has_diagnostic_code(&diagnostics, 2349),
        "push on mapped type with generic index should be callable, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_type_generic_indexed_access_full_file_has_no_ts2344_or_ts7006() {
    let source = r#"
type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
};

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar' };
    [1]: { a: 'b' };
};

type P<T extends keyof TypesMap> = { t: T } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => p.foo,
    [1]: (p) => p.a,
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_diagnostic_code(&diagnostics, 2344),
        "full mapped-type generic indexed-access repro should not emit TS2344, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "full mapped-type generic indexed-access repro should not emit TS7006, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_type_recursive_inference_generic_call_preserves_nested_callback_context() {
    let source = r#"
type MorphTuple = [string, "|>", any];

type validateMorph<def extends MorphTuple> = def[1] extends "|>"
    ? [validateDefinition<def[0]>, "|>", (In: def[0]) => unknown]
    : def;

type validateDefinition<def> = def extends MorphTuple
    ? validateMorph<def>
    : {
          [k in keyof def]: validateDefinition<def[k]>
      };

declare function type<def>(def: validateDefinition<def>): def;

const shallow = type(["ark", "|>", (x) => x.length]);
const objectLiteral = type({ a: ["ark", "|>", (x) => x.length] });
const nestedTuple = type([["ark", "|>", (x) => x.length]]);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "recursive mapped/conditional generic call should contextually type nested callbacks, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_overloaded_array_method_aliases_preserves_callback_context() {
    let source = r#"
interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }
interface Arr<T> {
  filter<S extends T>(pred: (value: T) => value is S): S[];
  filter(pred: (value: T) => unknown): T[];
}
declare const m: Arr<Fizz>["filter"] | Arr<Buzz>["filter"];
m(item => item.id < 5);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "union of overloaded array method aliases should contextually type callback params, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_builtin_array_methods_preserves_callback_context() {
    let source = r#"
interface Fizz { id: number; fizz: string }
interface Buzz { id: number; buzz: string }

([] as Fizz[] | Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | readonly Buzz[]).filter(item => item.id < 5);
([] as Fizz[] | Buzz[]).find(item => item);
([] as Fizz[] | Buzz[]).every(item => item.id < 5);
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "union of built-in array methods should contextually type callback params, got: {diagnostics:?}"
    );
}

#[test]
fn inherited_generic_class_field_array_methods_preserve_callback_context() {
    let source = r#"
export {};
type StringCheck =
  | { kind: "email"; pattern: string }
  | { kind: "regex"; regex: RegExp };
interface StringDef extends BaseDef {
  checks: StringCheck[];
}
interface BoxDef<Item> extends BaseDef {
  values: Item[];
}
interface BaseDef {
  errorMap?: (issue: unknown) => string;
}
declare abstract class Base<
  Output,
  Def extends BaseDef = BaseDef,
  Input = Output
> {
  readonly _type: Output;
  readonly _output: Output;
  readonly _input: Input;
  readonly _def: Def;
  abstract parse(input: unknown): Output;
}
class StringSchema extends Base<string, StringDef> {
  parse(input: unknown): string {
    return String(input);
  }
  get isEmail() {
    return !!this._def.checks.find(ch => ch.kind === "email");
  }
  get usesRegex() {
    return !!this._def.checks.find(entry => entry.kind === "regex");
  }
}
class BoxSchema<T> extends Base<T, BoxDef<T>> {
  parse(input: unknown): T {
    return input as T;
  }
  has(value: T) {
    return this._def.values.find(item => item === value);
  }
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "array methods reached through inherited generic class fields should contextually type callback params, got: {diagnostics:?}"
    );
}
// =============================================================================
// Assignment Expression Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_assignment_wrong_primitive() {
    let source = r#"
        let a: number;
        a = "string";
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_assignment_wrong_object_property() {
    let source = r#"
        let obj: { a: number };
        obj = { a: "string" };
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// =============================================================================
// Multiple TS2322 Errors
// =============================================================================

#[test]
fn test_ts2322_multiple_errors() {
    let source = r#"
        function f1(): number {
            return "string";
        }
        function f2(): string {
            return 42;
        }
        let x: number = "x";
        let y: string = 123;
    "#;

    let count = count_errors_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(count >= 4, "Expected at least 4 TS2322 errors, got {count}");
}

#[test]
fn test_ts2322_distinct_type_parameters_are_not_suppressed() {
    let source = r#"
        function unconstrained<T, U>(t: T, u: U) {
            t = u;
            u = t;
        }

        function constrained<T extends { foo: string }, U extends { foo: string }>(t: T, u: U) {
            t = u;
            u = t;
        }

        class Box<T extends { foo: string }, U extends { foo: string }> {
            t!: T;
            u!: U;

            assign() {
                this.t = this.u;
                this.u = this.t;
            }
        }
    "#;

    let count = count_errors_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        count, 6,
        "Expected TS2322 for each distinct type-parameter assignment, got {count}"
    );
}

// =============================================================================
// No Error Tests (Verify we don't emit false positives)
// =============================================================================

#[test]
fn test_ts2322_no_error_correct_types() {
    let source = r#"
        function returnNumber(): number {
            return 42;
        }
        let x: number = 42;
        let y: { a: number } = { a: 42 };
        let z: string[] = ["a", "b"];
        let a: number;
        a = 42;
    "#;

    assert!(!has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_generic_object_literal_call_property_anchor_and_message() {
    let source = r#"
function foo<T>(x: { bar: T; baz: T }) {
    return x;
}
var r = foo<number>({ bar: 1, baz: '' });
"#;

    let diagnostics = diagnostics_for_source(source);
    let errors: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    let has_ts2345 = diagnostics.iter().any(|d| {
        d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
    });

    assert_eq!(
        errors.len(),
        1,
        "Expected exactly one TS2322 diagnostic, got: {errors:?}"
    );
    let diag = errors[0];
    let expected_messages = [
        "Type 'string' is not assignable to type 'number'.",
        "Type 'number' is not assignable to type 'string'.",
    ];
    assert!(
        expected_messages.contains(&diag.message_text.as_str()),
        "Unexpected TS2322 message: {}",
        diag.message_text
    );
    assert!(
        !has_ts2345,
        "Did not expect outer TS2345 once property-level TS2322 elaboration applies, got: {diagnostics:?}"
    );

    let expected_baz_start = source
        .find("baz: ''")
        .expect("expected test snippet to contain baz property");
    let expected_bar_start = source
        .find("bar: 1")
        .expect("expected test snippet to contain bar property");
    let expected_object_start = source
        .find("{ bar: 1, baz: '' }")
        .expect("expected test snippet to contain object literal");
    assert!(
        diag.start == expected_baz_start as u32
            || diag.start == expected_bar_start as u32
            || diag.start == expected_object_start as u32,
        "Expected TS2322 on baz/bar/object literal node, got start {}",
        diag.start
    );
}

#[test]
fn test_ts2322_generic_private_class_assignment_preserves_type_arguments() {
    let source = r#"
class C<T> {
    #foo: T;
    #method(): T { return this.#foo; }
    get #prop(): T { return this.#foo; }
    set #prop(value: T) { this.#foo = value; }

    bar(x: C<T>) { return x.#foo; }
    bar2(x: C<T>) { return x.#method(); }
    bar3(x: C<T>) { return x.#prop; }

    baz(x: C<number>) { return x.#foo; }
    baz2(x: C<number>) { return x.#method; }
    baz3(x: C<number>) { return x.#prop; }

    quux(x: C<string>) { return x.#foo; }
    quux2(x: C<string>) { return x.#method; }
    quux3(x: C<string>) { return x.#prop; }
}

declare let a: C<number>;
declare let b: C<string>;
a.#foo;
a.#method;
a.#prop;
a = b;
b = a;
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    );
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        messages.len(),
        2,
        "expected exactly two TS2322 assignment diagnostics, got: {diagnostics:?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Type 'C' is not assignable to type 'C'.")),
        "generic class TS2322 should not erase type arguments, got: {diagnostics:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message
                .contains("Type 'C<string>' is not assignable to type 'C<number>'.")),
        "expected C<string> -> C<number> TS2322 display, got: {diagnostics:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message
                .contains("Type 'C<number>' is not assignable to type 'C<string>'.")),
        "expected C<number> -> C<string> TS2322 display, got: {diagnostics:?}"
    );
}

#[test]
fn inferred_generic_class_new_preserves_application_display() {
    let source = r#"
namespace FirstSource {
    export interface OptionalShape<T, U> {
        one: T;
        two?: U;
    }
    var obj: OptionalShape<number, string> = { one: 1 };
    export var value = obj;
}

namespace FirstTarget {
    export class RequiredShape<T, U> {
        constructor(public one: T, public two: U) {}
    }
    var instance = new RequiredShape(1, "a");
    export var value = instance;
}

FirstTarget.value = FirstSource.value;

namespace SecondSource {
    export interface MaybePair<X, Y> {
        left: X;
        right?: Y;
    }
    var obj: MaybePair<boolean, number> = { left: true };
    export var value = obj;
}

namespace SecondTarget {
    export class StrictPair<A, B> {
        constructor(public left: A, public right: B) {}
    }
    var instance = new StrictPair(false, 1);
    export var value = instance;
}

SecondTarget.value = SecondSource.value;
"#;

    let messages = ts2322_messages(source);

    assert!(
        messages.iter().any(|message| {
            message.contains("OptionalShape<number, string>")
                && message.contains("RequiredShape<number, string>")
        }),
        "expected inferred RequiredShape application display, got: {messages:?}"
    );
    assert!(
        messages.iter().any(|message| {
            message.contains("MaybePair<boolean, number>")
                && message.contains("StrictPair<boolean, number>")
        }),
        "expected inferred StrictPair application display, got: {messages:?}"
    );
}

#[test]
fn generic_object_assign_initializer_keeps_outer_ts2322() {
    let source = r#"
type Omit<T, K> = Pick<T, Exclude<keyof T, K>>;
type Assign<T, U> = Omit<T, keyof U> & U;

class Base<T> {
    constructor(public t: T) {}
}

export class Foo<T> extends Base<T> {
    update(): Foo<Assign<T, { x: number }>> {
        const v: Assign<T, { x: number }> = Object.assign(this.t, { x: 1 });
        return new Foo(v);
    }
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let codes = project_diagnostic_codes(&diagnostics);
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected outer TS2322 for generic Object.assign initializer, got: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Expected initializer TS2769 for generic Object.assign initializer, got: {diagnostics:?}"
    );
}

#[test]
fn generic_object_assign_helper_keeps_outer_ts2322() {
    let source = r#"
const func = <T>() => {};
const assign = <T, U>(a: T, b: U) => Object.assign(a, b);
const res: (() => void) & { func: any } = assign(() => {}, { func });
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let codes = project_diagnostic_codes(&diagnostics);
    assert!(
        codes.contains(&diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Expected inner TS2769 for generic Object.assign helper, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("Type '{ func: <T>() => void; }' is not assignable to type '(() => void) & { func: any; }'.")
        }),
        "Expected outer TS2322 for generic Object.assign helper, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_string_intrinsic_targets_widen_literal_sources() {
    let source = r#"
let x: Uppercase<string>;
x = "AbC";

let y: Lowercase<string>;
y = "AbC";
"#;

    let diagnostics = diagnostics_for_source(source);
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|d| d.message_text.as_str())
        .collect();

    assert!(
        messages.contains(&"Type 'string' is not assignable to type 'Uppercase<string>'."),
        "Expected widened source diagnostic for Uppercase<string>, got: {messages:?}"
    );
    assert!(
        messages.contains(&"Type 'string' is not assignable to type 'Lowercase<string>'."),
        "Expected widened source diagnostic for Lowercase<string>, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains("\"AbC\"")),
        "String intrinsic diagnostics should widen the source literal, got: {messages:?}"
    );
}

#[test]
fn test_type_literal_local_intrinsic_utility_aliases_shadow_lib_intrinsics() {
    let diagnostics = get_all_diagnostics(
        r#"
export {};

type Uppercase<T> = { custom: T };
type NoInfer<T> = { custom: T };

type UpperBox = {
  value: Uppercase<"abc">;
};

type NoInferBox = {
  value: NoInfer<string>;
};

const upperOk: UpperBox = { value: { custom: "abc" } };
const upperBad: UpperBox = { value: "ABC" };

const noInferOk: NoInferBox = { value: { custom: "abc" } };
const noInferBad: NoInferBox = { value: "abc" };

upperOk;
upperBad;
noInferOk;
noInferBad;
"#,
    );

    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        messages.len(),
        2,
        "Expected only the two string assignments to fail, got: {diagnostics:#?}"
    );
    assert!(
        messages.iter().any(|message| message
            .contains("Type 'string' is not assignable to type 'Uppercase<\"abc\">'.")),
        "Expected local Uppercase alias target, got: {messages:?}"
    );
    assert!(
        messages.iter().any(|message| message
            .contains("Type 'string' is not assignable to type 'NoInfer<string>'.")),
        "Expected local NoInfer alias target, got: {messages:?}"
    );
    assert!(
        messages.iter().all(|message| !message.contains("\"ABC\"")),
        "Local Uppercase alias should not lower to the string intrinsic, got: {messages:?}"
    );
}

#[test]
fn test_ts2322_string_mapping_alias_displays_resolved_literal_target() {
    let source = r#"
type A = "aA";
type B = Uppercase<A>;
type ATemplate = `aA${string}`;
type BTemplate = Uppercase<ATemplate>;

declare let lit: B;
declare let tpl: BTemplate;

lit = tpl;
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Type '`AA${Uppercase<string>}`'"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for assigning uppercase template to uppercase literal");

    assert!(
        message.contains(r#"is not assignable to type '"AA"'."#),
        "expected evaluated uppercase literal target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<A>"),
        "did not expect intrinsic alias repaint for literal target, got: {message}"
    );
}

#[test]
fn test_ts2322_template_union_source_covered_by_string_displays_string() {
    let source = r#"
function f(s: string, cond: boolean) {
    const c1 = cond ? `foo${s}` : `bar${s}`;
    const c2: `foo${string}` | `bar${string}` = c1;
}
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
                .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for assigning widened template union source");

    assert!(
        message.contains("Type 'string' is not assignable"),
        "expected widened string source display, got: {message}"
    );
    assert!(
        !message.contains("string | `foo${string}`")
            && !message.contains("string | `bar${string}`"),
        "source display should not include template members covered by string: {message}"
    );
}

#[test]
fn test_ts2322_string_mapping_alias_displays_resolved_template_target() {
    let source = r#"
type Source = `aA${string}`;
type Target = Uppercase<Source>;

declare let sourceValue: Source;
declare let targetValue: Target;

targetValue = sourceValue;
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Type '`aA${string}`'"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for assigning unmapped template to mapped template target");

    assert!(
        message.contains("is not assignable to type '`AA${Uppercase<string>}`'."),
        "expected evaluated uppercase template target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<Source>"),
        "did not expect intrinsic alias repaint for template target, got: {message}"
    );
}

#[test]
fn test_ts2322_string_intrinsic_target_does_not_gain_nested_alias_display() {
    let source = r#"
declare let upper: Uppercase<string>;
declare let lowerUpper: Lowercase<Uppercase<string>>;

upper = lowerUpper;
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Lowercase<Uppercase<string>>"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for lowerUpper assigned to upper");

    assert!(
        message.contains("is not assignable to type 'Uppercase<string>'."),
        "expected resolved intrinsic target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<Uppercase<string>>"),
        "did not expect nested intrinsic repaint in target display, got: {message}"
    );
}

#[test]
fn test_ts2322_parameter_string_intrinsic_target_does_not_gain_nested_alias_display() {
    let source = r#"
function f(
    upper: Uppercase<string>,
    upperUpper: Uppercase<Uppercase<string>>,
    lowerUpper: Lowercase<Uppercase<string>>,
) {
    upper = lowerUpper;
}
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Lowercase<Uppercase<string>>"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for lowerUpper assigned to upper parameter");

    assert!(
        message.contains("is not assignable to type 'Uppercase<string>'."),
        "expected resolved intrinsic parameter target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<Uppercase<string>>"),
        "did not expect nested intrinsic repaint for parameter target, got: {message}"
    );
}

#[test]
fn test_ts2322_parameter_nested_same_kind_string_intrinsic_simplifies_target_display() {
    let source = r#"
function f(
    upper: Uppercase<string>,
    upperUpper: Uppercase<Uppercase<string>>,
    lowerUpper: Lowercase<Uppercase<string>>,
) {
    upperUpper = lowerUpper;
}
"#;

    let diagnostics = diagnostics_for_source(source);
    let message = diagnostics
        .iter()
        .find_map(|d| {
            (d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("Lowercase<Uppercase<string>>"))
            .then_some(d.message_text.as_str())
        })
        .expect("expected TS2322 for lowerUpper assigned to upperUpper parameter");

    assert!(
        message.contains("is not assignable to type 'Uppercase<string>'."),
        "expected simplified same-kind intrinsic target display, got: {message}"
    );
    assert!(
        !message.contains("Uppercase<Uppercase<string>>"),
        "did not expect nested same-kind intrinsic target display, got: {message}"
    );
}

// =============================================================================
// User-Defined Generic Type Application Tests (TS2322 False Positives)
// These test the root cause of 11,000+ extra TS2322 errors
// =============================================================================

#[test]
fn test_ts2322_no_false_positive_simple_generic_identity() {
    // type Id<T> = T; let a: Id<number> = 42;
    let source = r"
        type Id<T> = T;
        let a: Id<number> = 42;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> =
        diagnostics_with_code(&errors, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for Id<number> = 42, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_generic_object_wrapper() {
    // type Box<T> = { value: T }; let b: Box<number> = { value: 42 };
    let source = r"
        type Box<T> = { value: T };
        let b: Box<number> = { value: 42 };
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> =
        diagnostics_with_code(&errors, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for Box<number> = {{ value: 42 }}, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_type_true_branch() {
    // IsStr<string> should evaluate to 'true', and true is assignable to true
    let source = r"
        type IsStr<T> = T extends string ? true : false;
        let a: IsStr<string> = true;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> =
        diagnostics_with_code(&errors, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for IsStr<string> = true, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_type_false_branch() {
    // IsStr<number> should evaluate to 'false', and false is assignable to false
    let source = r"
        type IsStr<T> = T extends string ? true : false;
        let b: IsStr<number> = false;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> =
        diagnostics_with_code(&errors, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for IsStr<number> = false, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_user_defined_mapped_type() {
    // MyPartial<Cfg> should behave like Partial<Cfg>
    let source = r#"
        type MyPartial<T> = { [K in keyof T]?: T[K] };
        interface Cfg { host: string; port: number }
        let a: MyPartial<Cfg> = {};
        let b: MyPartial<Cfg> = { host: "x" };
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> =
        diagnostics_with_code(&errors, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for MyPartial<Cfg>, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_infer() {
    // UnpackPromise<Promise<number>> should evaluate to number
    let source = r"
        type UnpackPromise<T> = T extends Promise<infer U> ? U : T;
        let a: UnpackPromise<Promise<number>> = 42;
    ";

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> =
        diagnostics_with_code(&errors, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for UnpackPromise<Promise<number>> = 42, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_conditional_doesnt_leak_uninstantiated_type_parameter() {
    // SyntheticDestination<number, Synthetic<number, number>> should resolve to number, not T
    let source = r#"
        interface Synthetic<A, B extends A> {}
        type SyntheticDestination<T, U> = U extends Synthetic<T, infer V> ? V : never;
        type TestSynthetic = SyntheticDestination<number, Synthetic<number, number>>;
        const y: TestSynthetic = 3;
        const z: TestSynthetic = '3';
    "#;

    let errors = get_all_diagnostics(source);
    // Debug: All diagnostics: {errors:?}
    let _ = &errors;

    // y = 3 should NOT error (number is assignable to number)
    // z = '3' SHOULD error (string is not assignable to number)
    let ts2322_errors: Vec<_> =
        diagnostics_with_code(&errors, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322_errors.len(),
        1,
        "Expected exactly 1 TS2322 for string->number mismatch, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors[0].1.contains("not assignable"),
        "Expected assignability error, got: {:?}",
        ts2322_errors[0].1
    );
}

#[test]
fn test_ts2322_no_false_positive_conditional_expression_with_generics() {
    // Conditional expressions should compute union type first, not check branches individually
    // This tests the fix for premature assignability checking in conditional expressions
    let source = r#"
        interface Shape {
            name: string;
            width: number;
            height: number;
        }

        function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
            return obj[key];
        }

        function test(shape: Shape, cond: boolean) {
            // cond ? "width" : "height" should be type "width" | "height"
            // which IS assignable to K extends keyof Shape
            // Should NOT emit TS2322 on individual branches
            let widthOrHeight = getProperty(shape, cond ? "width" : "height");
        }
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> =
        diagnostics_with_code(&errors, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for conditional expression in generic function call, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_nested_conditional() {
    // Nested conditional expressions should also work
    let source = r#"
        function pick<T, K extends keyof T>(obj: T, key: K): T[K] {
            return obj[key];
        }

        type Point = { x: number; y: number; z: number };

        function test(p: Point, a: boolean, b: boolean) {
            // Nested ternary should produce "x" | "y" | "z"
            let value = pick(p, a ? "x" : (b ? "y" : "z"));
        }
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> =
        diagnostics_with_code(&errors, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for nested conditional expression, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_generic_indexed_write_preserves_type_parameter_display() {
    let source = r#"
        type Item = { a: string; b: number };

        function setValue<T extends Item, K extends keyof T>(obj: T, key: K) {
            obj[key] = 123;
        }
    "#;

    let ts2322_errors: Vec<_> = get_all_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'number' is not assignable to type 'T[K]'")),
        "Expected generic indexed-write TS2322 to preserve T[K] display, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_generic_indexed_write_rejects_concrete_constraint_values() {
    let source = r#"
        function setAny<T extends Record<string, any>, K extends keyof T>(obj: T, key: K) {
            obj[key] = 123;
        }

        function setNumber<T extends Record<string, number>, K extends keyof T>(obj: T, key: K) {
            obj[key] = 123;
        }
    "#;

    let ts2322_messages: Vec<_> = get_all_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message)
        .collect();

    assert_eq!(
        ts2322_messages.len(),
        2,
        "Expected one TS2322 for each concrete generic indexed write, got: {ts2322_messages:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'number' is not assignable to type 'T[K]'")),
        "Expected numeric generic indexed-write TS2322, got: {ts2322_messages:#?}"
    );
}

#[test]
fn test_ts2322_accessor_incompatible_getter_setter() {
    // TS 5.1+: when BOTH getter and setter have explicit type annotations,
    // unrelated types are allowed (no error).
    let source_both_explicit = r#"
        class C {
            get x(): string { return "s"; }
            set x(value: number) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source_both_explicit);
    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        ts2322.is_empty(),
        "TS 5.1+ allows unrelated types when both annotated; got: {ts2322:?}"
    );

    // But when getter has NO explicit return annotation (inferred type),
    // the inferred type must be compatible with the setter's explicit param type.
    let source_inferred_getter = r#"
        class C {
            get bar() { return 0; }
            set bar(n: string) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source_inferred_getter);
    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        !ts2322.is_empty(),
        "Inferred getter type (number) conflicts with explicit setter type (string) → TS2322"
    );
}

#[test]
fn test_ts2322_accessor_compatible_divergent_types() {
    // When getter return IS assignable to setter param, no error.
    let source = r#"
        class C {
            get x(): string { return "hello"; }
            set x(value: string | number) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );

    assert!(
        ts2322.is_empty(),
        "Getter return type (string) is assignable to setter param (string|number), no TS2322; got: {ts2322:?}"
    );
}

#[test]
fn test_ts2322_annotated_getter_contextually_types_unannotated_setter_parameter() {
    let source = r#"
        class C {
            get x(): string { return ""; }
            set x(value) { value = 0; }
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    let ts7006: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected setter body assignment to be checked against getter type: {diagnostics:?}"
    );
    assert!(
        ts7006.is_empty(),
        "paired getter should contextually type the setter parameter: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_js_accessor_jsdoc_does_not_force_inferred_getter_mismatch() {
    let source = r#"
        export class Foo {
            /**
             * @type {null | string}
             */
            _bar = null;

            get bar() {
                return this._bar;
            }
            /**
             * @type {string}
             */
            set bar(value) {
                this._bar = value;
            }
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            allow_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected JS accessor JSDoc pair to avoid TS2322 getter/setter mismatch. Actual diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_of_annotation_mismatch() {
    let source = r"
        for (const x: string of [1, 2, 3]) {}
    ";

    assert!(
        has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for for-of annotation mismatch"
    );
}

#[test]
fn test_ts2322_check_js_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        has_2322,
        "Expected TS2322 when checkJs checks mismatched JS annotation, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_mjs_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.mjs",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        has_2322,
        "Expected TS2322 for .mjs jsdoc mismatch when checkJs is enabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_false_does_not_enforce_annotation_type() {
    // No @ts-check: JSDoc types should NOT be enforced when checkJs is false.
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        !has_2322,
        "Expected no TS2322 when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_cjs_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.cjs",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        has_2322,
        "Expected TS2322 for .cjs jsdoc mismatch when checkJs is enabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_cjs_false_does_not_enforce_annotation_type() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.cjs",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no TS2322 for .cjs when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_local_exports_and_module_bindings_are_not_commonjs_roots() {
    let diagnostics = compile_with_options(
        r#"
// @ts-check
const exports = { n: 1 };
exports.n = "x";
exports.n.toFixed();

const module = { exports: { n: 1 } };
module.exports.n = "x";
module.exports.n.toFixed();
"#,
        "local-cjs-names.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert_eq!(
        ts2322.len(),
        2,
        "Local exports/module bindings should stay ordinary checked-JS assignments, got: {diagnostics:#?}"
    );
    assert!(
        ts2322.iter().all(
            |(_, message)| message.contains("Type 'string' is not assignable to type 'number'")
        ),
        "Expected string-to-number TS2322 diagnostics for both local CJS-name writes, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(_, message)| !message.contains("toFixed")),
        "Invalid writes should not retarget the local numeric properties to string, got: {diagnostics:#?}"
    );
}

#[test]
fn test_conflicting_private_intersection_reduces_before_missing_property_classification() {
    let diags = with_lib_contexts(
        r#"
class A { private x: unknown; y?: string; }
class B { private x: unknown; y?: string; }

declare let ab: A & B;
ab.y = 'hello';
ab = {};
"#,
        "test.ts",
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for impossible private-brand intersection assignment, got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected TS2339 on property access through never, got: {diags:?}"
    );
    assert!(
        !diags
            .iter()
            .any(|(code, _)| *code
                == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE),
        "Intersection should reduce before TS2741 missing-property classification, got: {diags:?}"
    );
}

#[test]
fn test_private_public_intersection_reduces_to_never_for_asserts_this() {
    let diags = with_lib_contexts(
        r#"
class Value<T> {
  constructor(private value: T | null) {}

  assertHasValue(): asserts this is { value: T } & Value<T> {
    if (this.value === null) {
      throw new Error("No value");
    }
  }

  getValue(): T {
    this.assertHasValue();
    return this.value;
  }
}
"#,
        "test.ts",
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diags.iter().any(|(code, message)| {
            *code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                && message.contains("Property 'value' does not exist on type 'never'")
        }),
        "Expected TS2339 for private/public impossible intersection reduced to never, got: {diags:?}"
    );
}

#[test]
fn test_ts2322_check_mjs_false_does_not_enforce_annotation_type() {
    // No @ts-check: JSDoc types should NOT be enforced when checkJs is false.
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.mjs",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no TS2322 for .mjs when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_false_does_not_enforce_jsdoc_return_type() {
    // No @ts-check: JSDoc @returns should NOT be enforced when checkJs is false.
    let source = r#"
        /** @returns {number} */
        function id(value) {
            return "string";
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no TS2322 for jsdoc return annotation when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_strict_js_strictness_affects_nullability() {
    let source = r"
        // @ts-check
        /** @type {number} */
        const maybeNumber = null;
    ";

    let loose = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: false,
            strict_null_checks: false,
            ..CheckerOptions::default()
        },
    );
    let strict = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let strict_has_2322 =
        has_diagnostic_code(&strict, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        strict_has_2322,
        "Expected strict+checkJs to emit TS2322 for null -> number jsdoc mismatch, got: {strict:?}"
    );
    assert!(
        strict.len() > loose.len(),
        "Expected strict mode to increase diagnostics for nullability in checkJs source"
    );
}

#[test]
fn test_ts2322_target_es2015_enables_template_lib_type_checks_without_falsely_reporting_target() {
    let source = r#"
        const x: number = 1;
        const y = "2";
        const z: number = y as any;
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        !has_2322,
        "No TS2322 expected in valid ES2015 + strict baseline case: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_target_es3_vs_target_es2015_jsdoc_annotation_mismatch() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const value = "bad";
    "#;

    let es3 = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            target: ScriptTarget::ES3,
            strict: true,
            ..Default::default()
        },
    );
    let es2022 = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            target: ScriptTarget::ES2022,
            strict: true,
            ..Default::default()
        },
    );
    let es3_has_2322 = has_diagnostic_code(&es3, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    let es2022_has_2322 =
        has_diagnostic_code(&es2022, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        es3_has_2322 && es2022_has_2322,
        "Expected jsdoc mismatch TS2322 under both targets, got es3={es3:?}, es2022={es2022:?}"
    );
}

#[test]
fn test_call_object_literal_optional_param_prefers_property_ts2322_over_ts2345() {
    let source = r#"
function foo({ x, y, z }?: { x: string; y: number; z: boolean }) {}
foo({ x: false, y: 0, z: "" });
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_count = diagnostic_count(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    let has_ts2345 = diagnostics.iter().any(|(code, _)| {
        *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
    });

    assert!(
        ts2322_count >= 2,
        "Expected property-level TS2322 for the mismatched object-literal fields, got: {diagnostics:?}"
    );
    assert!(
        !has_ts2345,
        "Did not expect outer TS2345 once property-level elaboration applies, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_callback_return_mismatch_reports_ts2345_for_identifier_expression_body() {
    // For contextually-typed expression-bodied arrow functions with identifier bodies
    // (like `undefined`), tsc elaborates the return type mismatch and reports TS2322
    // on the body expression rather than TS2345 on the whole callback argument.
    // This matches tsc behavior for contextual callbacks (no explicit param annotations).
    let source = r#"
function someGenerics3<T>(producer: () => T) { }
someGenerics3<number>(() => undefined);
"#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );

    assert!(
        has_ts2322,
        "Expected TS2322 on the body expression for contextual callback, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_true_does_not_relabel_with_unrelated_diagnostics() {
    let source = r#"
        // @ts-check
        /** @template T */
        /** @returns {{ value: T }} */
        function wrap(value) {
            return { value };
        }
        /** @type {number} */
        const n = wrap("string");
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: false,
            ..Default::default()
        },
    );
    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        has_2322,
        "Expected TS2322 for generic helper return mismatched with number annotation in JS, got: {diagnostics:?}"
    );
}

