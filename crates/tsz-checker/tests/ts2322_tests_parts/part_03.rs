#[test]
fn test_nested_discriminated_union_property_mismatch_emits_ts2322() {
    let source = r#"
type AN = { a: string } | { c: string }
type BN = { b: string }
type AB = { kind: "A", n: AN } | { kind: "B", n: BN }

const abab: AB = {
    kind: "A",
    n: {
        a: "a",
        b: "b",
    }
}
"#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one nested union TS2322 mismatch. Got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|diagnostic| {
            diagnostic.message_text.contains(
            "Type '{ kind: \"A\"; n: { a: string; b: string; }; }' is not assignable to type 'AB'."
        )
        }),
        "Expected outer AB assignability message. Got: {diagnostics:?}"
    );
    let expected_start = source.find("b: \"b\"").expect("expected b property") as u32;
    assert_eq!(
        ts2322[0].start, expected_start,
        "Expected TS2322 to anchor at the rejected nested property. Got: {diagnostics:?}"
    );

    let ok_source = r#"
type AN = { a: string } | { c: string }
type BN = { b: string }
type AB = { kind: "A", n: AN } | { kind: "B", n: BN }

const abac: AB = {
    kind: "A",
    n: {
        a: "a",
        c: "c",
    }
}
"#;

    let ok_diagnostics = get_all_diagnostics(ok_source);
    assert!(
        !ok_diagnostics.iter().any(|(code, _)| matches!(
            *code,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE | 2353
        )),
        "Expected valid nested union object to stay accepted. Got: {ok_diagnostics:?}"
    );
}

#[test]
fn object_freeze_preserves_literal_property_values_for_readonly_return() {
    let source = r#"
const PUPPETEER_REVISIONS = Object.freeze({
    chromium: '1011831',
    firefox: 'latest',
});

let preferredRevision = PUPPETEER_REVISIONS.chromium;
preferredRevision = PUPPETEER_REVISIONS.firefox;
"#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for Object.freeze literal property mismatch. Got: {diagnostics:?}"
    );
    assert!(
        ts2322[0]
            .message_text
            .contains("Type '\"latest\"' is not assignable to type '\"1011831\"'."),
        "Expected literal property values to be preserved through Object.freeze. Got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322[0].start,
        source
            .find("preferredRevision = PUPPETEER_REVISIONS.firefox")
            .expect("assignment should exist") as u32,
        "Expected TS2322 to anchor at the assignment expression. Got: {diagnostics:?}"
    );
}

#[test]
fn object_seal_widens_mutable_literal_property_values() {
    let source = r#"
const sealed = Object.seal({ x: 1 });
sealed.x = 2;

const frozen = Object.freeze({ x: 1 });
frozen.x = 2;
"#;

    let diagnostics = diagnostics_for_source(source);
    let seal_assignment_start = source
        .find("sealed.x = 2")
        .expect("sealed assignment should exist") as u32;
    let frozen_assignment_start = source
        .find("frozen.x = 2")
        .expect("frozen assignment should exist") as u32;
    let seal_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.start == seal_assignment_start)
        .collect();
    assert_eq!(
        seal_diagnostics.len(),
        0,
        "Expected Object.seal property assignment to remain mutable and widened. Got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code
            == diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY
            && diagnostic.start == frozen_assignment_start + "frozen.".len() as u32),
        "Expected Object.freeze assignment to remain readonly. Got: {diagnostics:?}"
    );
}

/// Regression: assignFromStringInterface2.ts
/// When both source and target have number index signatures but the source is
/// missing named properties from the target, TS2739/TS2740 should be emitted
/// (not TS2322). Number index signatures (common on String, Array, etc.) must
/// NOT suppress the missing-properties diagnostic.
#[test]
fn test_missing_properties_not_suppressed_by_number_index_signatures() {
    let source = r#"
        interface Target {
            foo(): string;
            bar(): string;
            baz(): string;
            qux(): string;
            quux(): string;
            corge(): string;
            grault(): string;
            [index: number]: string;
        }

        interface Source {
            foo(): string;
            [index: number]: string;
        }

        declare var target: Target;
        declare var source: Source;
        target = source;
    "#;

    let diagnostics = get_all_diagnostics(source);
    // TS2740 = "missing the following properties ... and N more" (6+ missing)
    let has_missing_props = diagnostics.iter().any(|(code, _)| {
        *code == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
    });
    assert!(
        has_missing_props,
        "Expected TS2740 (missing properties) when both types have number index signatures \
         but source is missing named properties. Number index signatures should NOT suppress \
         missing-property diagnostics in favor of TS2322. Got: {diagnostics:?}"
    );
    // Should NOT have TS2322 for this case — TS2740 replaces it
    let has_ts2322 = has_diagnostic_code(&diagnostics, 2322);
    assert!(
        !has_ts2322,
        "Expected TS2740, not TS2322, when source is missing named properties. Got: {diagnostics:?}"
    );
}

/// Regression: didYouMeanElaborationsForExpressionsWhichCouldBeCalled.ts
/// `toLocaleString` (and other Object-prototype methods) must always be filtered
/// from TS2740/TS2739 missing-property lists — even when the target overrides it.
/// tsc's `getMissingMembersOfType` treats a property as missing only when the
/// source lacks any member with that name, and Object inheritance always
/// satisfies the name lookup for `toLocaleString`.  Including it in the
/// missing list inflates the "and N more" count by 1.
#[test]
fn test_ts2740_does_not_list_tolocalestring_as_missing() {
    // Synthesize a target with 6+ missing properties so TS2740 (with truncation)
    // fires.  The target adds a `toLocaleString` overload that the source does
    // not match, which in tsz used to surface `toLocaleString` as a missing
    // property.  tsz must always filter Object-prototype names from the missing
    // list since the source has them by name via Object inheritance.
    let source = r#"
interface Target {
    toLocaleString(): string;
    toLocaleString(locale: string, options: object): string;
    m1: number;
    m2: number;
    m3: number;
    m4: number;
    m5: number;
    m6: number;
    m7: number;
}

declare const s: { foo: string };
const tt: Target = s;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2740 = diagnostics
        .iter()
        .find(|(code, _)| {
            *code == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
        })
        .expect("expected TS2740 for assigning narrower type to Target");
    // The missing list is the substring after the colon.  Splitting at ": "
    // yields the source display first, then the target display, then the list.
    let missing_list = ts2740
        .1
        .split(": ")
        .nth(2)
        .expect("TS2740 message should contain `: <list>`");
    assert!(
        !missing_list.contains("toLocaleString"),
        "TS2740 missing list must not include `toLocaleString` (Object-prototype method), got: {missing_list}"
    );
    assert!(
        missing_list.contains("and 3 more"),
        "TS2740 missing list should report `and 3 more` for 7 missing m1..m7, got: {missing_list}"
    );
}

/// When `strictBuiltinIteratorReturn` is true, `BuiltinIteratorReturn` resolves to `undefined`.
/// Assigning `undefined` to `number` must produce TS2322.
#[test]
fn test_strict_builtin_iterator_return_ts2322() {
    // Use BuiltinIteratorReturn directly — it's defined as `type BuiltinIteratorReturn = intrinsic`
    // in lib.es2015.iterable.d.ts and resolves to `undefined` when strict.
    let source = r#"
type R = BuiltinIteratorReturn;
const x: number = undefined as R;
"#;
    let options = CheckerOptions {
        strict_builtin_iterator_return: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", options);

    let ts2322_count = diagnostic_count(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        ts2322_count >= 1,
        "Expected TS2322 for assigning BuiltinIteratorReturn (=undefined) to number when \
         strictBuiltinIteratorReturn is true. Got: {diagnostics:?}"
    );
}

#[test]
fn test_strict_builtin_iterator_return_in_lib_heritage_displays_undefined() {
    let source = r#"
declare const map: Map<string, number>;
const r1: number = map.values().next().value;
"#;
    let options = CheckerOptions {
        strict_builtin_iterator_return: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", options);

    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("number | undefined")),
        "Expected IteratorObject heritage argument BuiltinIteratorReturn to resolve to undefined, got: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("BuiltinIteratorReturn")),
        "BuiltinIteratorReturn should not leak into strict diagnostics, got: {messages:?}"
    );
}

#[test]
fn test_builtin_iterator_helpers_keep_contextual_callback_types() {
    let source = r#"
const iterator = Iterator.from([0, 1, 2]);

const mapped: IteratorObject<string> =
    iterator.map((value, index) => value === index ? "same" : String(value));
const filtered: IteratorObject<number> =
    iterator.filter((value, index) => value > index);

function isZero(value: number): value is 0 {
    return value === 0;
}
const zero: IteratorObject<0> = iterator.filter(isZero);

function* gen() {
    yield 0;
}
const mappedGen: IteratorObject<string> =
    gen().map(value => value === 0 ? "zero" : "other");
const mappedValues: IteratorObject<string> =
    [0, 1, 2].values().map(value => value === 0 ? "zero" : "other");

class GoodIterator extends Iterator<number> {
    next() {
        return { done: false, value: 0 } as const;
    }
}

mapped;
filtered;
zero;
mappedGen;
mappedValues;
new GoodIterator();
"#;
    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_builtin_iterator_return: true,
            strict_null_checks: true,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    for code in [
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
        diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
    ] {
        assert_eq!(
            diagnostic_count(&diagnostics, code),
            0,
            "builtin iterator helpers should not emit TS{code}, got: {diagnostics:?}"
        );
    }
}

/// When `strictBuiltinIteratorReturn` is false, `BuiltinIteratorReturn` resolves to `any`.
/// Assigning `any` to `number` is always allowed, so no error.
#[test]
fn test_no_error_without_strict_builtin_iterator_return() {
    let source = r#"
declare const x: BuiltinIteratorReturn;
const r1: number = x;
"#;
    let options = CheckerOptions {
        strict_builtin_iterator_return: false,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", options);

    let ts2322_count = diagnostic_count(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        ts2322_count == 0,
        "Expected no TS2322 when strictBuiltinIteratorReturn is false \
         (BuiltinIteratorReturn=any). Got: {diagnostics:?}"
    );
}

#[test]
fn iterator_intersection_return_method_name_is_not_unresolved_identifier() {
    let source = r#"
type WithReturn = Iterator<number> & { return(): IteratorReturnResult<void> };

const iter: WithReturn = {
  next() { return { value: 1, done: false as const }; },
  return() { return { value: undefined, done: true as const }; }
};
"#;
    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<_> = diagnostics.iter().map(|diagnostic| diagnostic.0).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME)
            && !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "object literal method name `return` should satisfy Iterator intersection without TS2304/TS2322, got {diagnostics:#?}"
    );
}

#[test]
fn test_module_local_builtin_iterator_return_alias_shadows_intrinsic() {
    let source = r#"
export {};

type BuiltinIteratorReturn = string;

const ok: BuiltinIteratorReturn = "done";
const bad: BuiltinIteratorReturn = undefined;

ok;
bad;
"#;
    let options = CheckerOptions {
        strict_builtin_iterator_return: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", options);
    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect::<Vec<_>>();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 for assigning undefined to the local string alias. Got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0]
            .1
            .contains("Type 'undefined' is not assignable to type 'string'."),
        "Expected the local alias to resolve to string. Actual diagnostic: {ts2322:#?}"
    );
}

#[test]
fn test_ts2322_intersections_and_optional_properties_source_display() {
    let source = r#"
declare let x: { a?: number, b: string };
declare let y: { a: null, b: string };
declare let z: { a: null } & { b: string };
x = y;
x = z;
"#;
    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2322.iter().any(|message| message.contains(
            "Type '{ a: null; b: string; }' is not assignable to type '{ a?: number | undefined; b: string; }'."
        )),
        "expected plain object source to display as a collapsed object, got: {ts2322:#?}"
    );
    assert!(
        ts2322.iter().any(|message| message.contains(
            "Type '{ a: null; } & { b: string; }' is not assignable to type '{ a?: number | undefined; b: string; }'."
        )),
        "expected declared intersection source to keep its intersection surface, got: {ts2322:#?}"
    );
}

#[test]
fn test_ts2322_reports_alias_intersection_optional_property_conflict() {
    let source = r#"
interface To {
    field?: number;
    anotherField: string;
}
type From = { field: null } & Omit<To, 'field'>;
function foo(v: From) {
    let x: To;
    x = v;
}
"#;
    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'From' is not assignable to type 'To'.")),
        "expected alias intersection assignment to report TS2322 as From -> To, got: {ts2322:#?}"
    );
}

#[test]
fn test_ts2322_keeps_outer_object_error_for_direct_index_access_target() {
    let source = r#"
interface TextChannel {
    id: string;
    type: 'text';
    phoneNumber: string;
}

interface EmailChannel {
    id: string;
    type: 'email';
    addres: string;
}

type Channel = TextChannel | EmailChannel;

export type ChannelType = Channel extends { type: infer R } ? R : never;

type Omit<T, K extends keyof T> = Pick<
    T,
    ({ [P in keyof T]: P } & { [P in K]: never } & { [x: string]: never })[keyof T]
>;

type ChannelOfType<T extends ChannelType, A = Channel> = A extends { type: T }
    ? A
    : never;

export type NewChannel<T extends Channel> = Pick<T, 'type'> &
    Partial<Omit<T, 'type' | 'id'>> & { localChannelId: string };

export function makeNewChannel<T extends ChannelType>(type: T): NewChannel<ChannelOfType<T>> {
    const localChannelId = `blahblahblah`;
    return { type, localChannelId };
}

const newTextChannel = makeNewChannel('text');
newTextChannel.phoneNumber = '613-555-1234';

const newTextChannel2 : NewChannel<TextChannel> = makeNewChannel('text');
newTextChannel2.phoneNumber = '613-555-1234';
"#;
    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one outer TS2322 for the return object. Got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(
            |(_, message)| message.contains("Type '{ type: T; localChannelId:")
                && message.contains("}' is not assignable to type 'NewChannel<")
                && message.contains(
                    "NewChannel<ChannelOfType<T, TextChannel> | ChannelOfType<T, EmailChannel>>"
                )
        ),
        "Expected TS2322 to keep the outer object literal and source-order target union. Got: {diagnostics:?}"
    );
    let message = &ts2322[0].1;
    assert!(
        message.contains("Type '{ type: T; localChannelId: string; }'"),
        "Expected shorthand property display to widen localChannelId. Got: {message}"
    );
    assert!(
        !message.contains(r#"localChannelId: "blahblahblah""#),
        "Did not expect shorthand property display to preserve const literal. Got: {message}"
    );
    assert!(
        ts2322
            .iter()
            .all(|(_, message)| !message.contains("never[\"type\"]")),
        "Did not expect property-level never[\"type\"] elaboration. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_flatarray_assignment_keeps_rhs_declared_alias_display() {
    let source = r#"
declare const foo: unknown[];
const bar = foo.flatMap(bar => bar as Foo);

interface Foo extends Array<string> {}

function f<Arr, D extends number>(x: FlatArray<Arr, any>, y: FlatArray<Arr, D>) {
    x = y;
    y = x;
}
"#;
    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            emit_declarations: true,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one FlatArray assignment TS2322. Got: {diagnostics:?}"
    );
    let message = &ts2322[0].1;
    assert!(
        message
            .contains("Type 'FlatArray<Arr, any>' is not assignable to type 'FlatArray<Arr, D>'."),
        "Expected source display to preserve the RHS FlatArray alias. Got: {message}"
    );
    assert!(
        !message.contains("Arr | Arr extends"),
        "Did not expect FlatArray source to expand to its conditional body. Got: {message}"
    );
}

#[test]
fn test_ts2322_recursive_indexed_alias_assignment_keeps_declared_alias_display() {
    let source = r#"
type Step<Arr, Depth extends number> = {
    done: Arr;
    recur: Arr extends { item: infer InnerArr } ? Step<InnerArr, [-1, 0, 1, 2][Depth]> : Arr;
}[Depth extends -1 ? "done" : "recur"];

function f<Arr, D extends number>(x: Step<Arr, any>, y: Step<Arr, D>) {
    x = y;
    y = x;
}
"#;
    let diagnostics = with_lib_contexts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one recursive alias assignment TS2322. Got: {diagnostics:?}"
    );
    let message = &ts2322[0].1;
    assert!(
        message.contains("Type 'Step<Arr, any>' is not assignable to type 'Step<Arr, D>'."),
        "Expected source display to preserve the declared Step alias. Got: {message}"
    );
    assert!(
        !message.contains("Arr extends") && !message.contains("infer InnerArr"),
        "Did not expect Step source to expand to its conditional body. Got: {message}"
    );
}

#[test]
fn test_ts2322_infinite_constraints_duplicate_value_fingerprints() {
    let source = r#"
type AProp<T extends { a: string }> = T

declare function myBug<
  T extends { [K in keyof T]: T[K] extends AProp<infer U> ? U : never }
>(arg: T): T

const out = myBug({obj1: {a: "test"}})

type Value<V extends string = string> = Record<"val", V>;
declare function value<V extends string>(val: V): Value<V>;

declare function ensureNoDuplicates<
  T extends {
    [K in keyof T]: Extract<T[K], Value>["val"] extends Extract<T[Exclude<keyof T, K>], Value>["val"]
      ? never
      : any
  }
>(vals: T): void;

const noError = ensureNoDuplicates({main: value("test"), alternate: value("test2")});

const shouldBeNoError = ensureNoDuplicates({main: value("test")});

const shouldBeError = ensureNoDuplicates({main: value("dup"), alternate: value("dup")});
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2322
            .iter()
            .filter(|message| message
                .contains("Type 'Value<\"dup\">' is not assignable to type 'never'."))
            .count(),
        2,
        "expected two duplicate Value<\"dup\"> TS2322 diagnostics, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .all(|message| !message
                .contains("Type '{ a: string; }' is not assignable to type 'never'.")),
        "did not expect recursive AProp inference to produce a false TS2322, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_const_type_param_multi() {
    // When a function has multiple type params and the first is `const`,
    // the solver's full inference path (used for >1 type params) must not
    // produce a false TS2322 on the argument. Previously, the final argument
    // check compared the checker's const-asserted arg type against the
    // solver's independently const-inferred type (different TypeIds for
    // semantically identical readonly types).
    let source = r#"
function f<const T, U>(x: T): T { return x; }
const t = f({ a: 1, b: "c", d: ["e", 2] });
"#;
    assert!(
        !has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for const type parameter with multiple type params"
    );
}

#[test]
fn mixin_inferred_const_literal_tag_substitutes_return_class_property() {
    let source = r#"
type Constructor<T = {}> = new (...args: any[]) => T;

class User {
  name = 'unknown';
}

function Tagged<TBase extends Constructor, TTag>(Base: TBase, tag: TTag) {
  return class Tagged extends Base {
    tag: TTag = tag;
  };
}

const TaggedUser = Tagged(User, 'user' as const);
const tagu = new TaggedUser();
const tag: 'user' = tagu.tag;
"#;

    assert!(
        !has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 when an inferred const literal tag flows into a mixin return class property"
    );
}

#[test]
fn non_primitive_conditional_with_type_params_matches_tsc_errors() {
    let source = r#"
type A<T, V> = { [P in keyof T]: T[P] extends V ? 1 : 0; };
type B<T, V> = { [P in keyof T]: T[P] extends V | object ? 1 : 0; };

let a: A<{ a: 0 | 1 }, 0> = { a: 0 };
let b: B<{ a: 0 | 1 }, 0> = { a: 0 };

function foo<T, U>(x: T) {
    let a: object = x;
    let b: U | object = x;
}
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_messages = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ts2322_messages.len(),
        2,
        "expected only the two generic assignment errors, got: {diagnostics:?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'T' is not assignable to type 'object'.")),
        "missing T to object diagnostic, got: {ts2322_messages:?}"
    );
    assert!(
        ts2322_messages.iter().any(|message| {
            message.contains("Type 'T' is not assignable to type 'object | U'.")
                || message.contains("Type 'T' is not assignable to type 'U | object'.")
        }),
        "missing T to U | object diagnostic, got: {ts2322_messages:?}"
    );
    assert!(
        !ts2322_messages
            .iter()
            .any(|message| message.contains("B<{")),
        "mapped conditional assignment should not fail, got: {ts2322_messages:?}"
    );
}

#[test]
fn ts2322_optional_property_vs_number_index_preserves_implicit_undefined() {
    // tsc: `{ 1?: string }` assigned to `{ [k: number]: string }` must error
    // because the optional `1` contributes `string | undefined` to the check
    // against the number index value type `string`. Regression test for
    // `optionalPropertyAssignableToStringIndexSignature.ts`.
    let source = r#"
declare let probablyArray: { [key: number]: string };
declare let numberLiteralKeys: { 1?: string };
probablyArray = numberLiteralKeys;
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    assert!(
        has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "expected TS2322 for optional numeric property vs number index, got: {diagnostics:?}"
    );
}

#[test]
fn ts2322_optional_string_property_vs_string_index_still_ok() {
    // Regression guard: tsc allows `{ k1?: string }` assigned to
    // `{ [k: string]: string }` because the string index strips the implicit
    // `| undefined` contributed by the optional flag.
    let source = r#"
declare let optionalProperties: { k1?: string };
let stringDictionary: { [key: string]: string } = optionalProperties;
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "expected no TS2322 for `{{ k1?: string }}` vs string index, got: {diagnostics:?}"
    );
}

#[test]
fn exact_optional_property_write_uses_ts2412() {
    let source = r#"
interface U2 {
    email?: string | number;
}
declare const e: string | boolean | undefined;
declare let u2: U2;
u2.email = e;
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    assert!(
        diagnostics.iter().any(|(code, _)| {
            *code
                == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2
        }),
        "Expected TS2412 for exact-optional property write mismatch, got: {diagnostics:#?}"
    );
}

#[test]
fn exact_optional_property_direct_undefined_write_uses_ts2412() {
    let source = r#"
function f(obj: { a?: string, b?: string | undefined }) {
    let a = obj.a;
    let b = obj.b;
    obj.a = "hello";
    obj.b = "hello";
    obj.a = undefined;
    obj.b = undefined;
}
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        emit_declarations: true,
        strict: true,
        strict_null_checks: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    assert_eq!(
        diagnostics
            .iter()
            .filter(|(code, _)| {
                *code
                    == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2
            })
            .count(),
        1,
        "Expected one TS2412 for direct undefined write to exact-optional property, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code
                == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2
                && message.contains("Type 'undefined' is not assignable to type 'string'")
        }),
        "Expected TS2412 to report the offending undefined source, got: {diagnostics:#?}"
    );
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected direct undefined exact-optional write to avoid TS2322 fallback, got: {diagnostics:#?}"
    );
}

#[test]
fn exact_optional_property_presence_narrows_self_assignment_source() {
    let source = r#"
function f(obj: { a?: string, b?: string | undefined }) {
    if ("a" in obj) {
        obj.a = obj.a;
    }
    else {
        obj.a = obj.a;
    }
    if (obj.hasOwnProperty("a")) {
        obj.a = obj.a;
    }
    else {
        obj.a = obj.a;
    }
    if ("b" in obj) {
        obj.b = obj.b;
    }
    else {
        obj.b = obj.b;
    }
}
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts_and_positions(source, "test.ts", options);
    let ts2412: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _, _)| {
            *code
                == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2
        })
        .collect();

    assert_eq!(
        ts2412.len(),
        2,
        "Expected TS2412 only for absent exact-optional property reads, got: {diagnostics:#?}"
    );
    assert!(
        ts2412.iter().all(|(_, _, message)| message
            .contains("Type 'undefined' is not assignable to type 'string'")),
        "Expected absent-branch TS2412 to report `undefined`, got: {diagnostics:#?}"
    );
    assert!(
        ts2412
            .iter()
            .all(|(_, _, message)| !message.contains("string | undefined")),
        "Expected present/absent exact-optional narrowing to avoid `string | undefined`, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(code, _, _)| *code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected present-branch self-assignments to avoid TS2322 fallback, got: {diagnostics:#?}"
    );
}

#[test]
fn exact_optional_property_object_message_preserves_optional_target_surface() {
    let source = r#"
const x: { foo?: number } = { foo: undefined };
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code
                == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD
                && message.contains("type '{ foo?: number; }'")
                && !message.contains("foo?: number | undefined")
        }),
        "Expected TS2375 target display to omit synthetic undefined, got: {diagnostics:#?}"
    );
}

#[test]
fn exact_optional_tuple_elements_reject_present_undefined() {
    let source = r#"
declare let t: [number, string?, boolean?];
t[1] = undefined;
t = [1, undefined];
t = [1, "ok", undefined];
t = [1, undefined, undefined];
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    let ts2322_messages = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ts2322_messages.len(),
        4,
        "Expected exact optional tuple writes/literals with present undefined to emit TS2322, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'undefined' is not assignable to type 'string'")),
        "Expected direct tuple slot write to reject undefined against string, got: {diagnostics:#?}"
    );
}

#[test]
fn tuple_source_display_widens_boolean_literals_past_fixed_target_slots() {
    let source = r#"
declare let target: [number, string];
target = [1, "x", true];
target = [1, "x", (false)];
"#;
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    let ts2322_messages = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ts2322_messages.len(),
        2,
        "Expected both tuple overflow literals to emit TS2322, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .all(|message| message.contains("Type '[number, string, boolean]'")),
        "Expected overflow boolean literals to display as boolean, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .all(|message| !message.contains("Type '[number, string, true]'")
                && !message.contains("Type '[number, string, false]'")),
        "Tuple overflow source display must not preserve uncontextualized boolean literals, got: {diagnostics:#?}"
    );
}

#[test]
fn exact_optional_tuple_source_display_uses_boolean_literal_policy() {
    let source = r#"
declare let t: [number, string?, boolean?];
declare let u: [number, string?, false?];
declare let p: [number, string?, boolean?];
declare let c: [number, string?, boolean?];
declare let s: [number, string?, boolean?];
declare let a: [number, string?, boolean?];
t = [42, undefined, true];
u = [42, undefined, false];
p = [42, undefined, (true)];
c = [42, undefined, true as const];
s = [42, undefined, true satisfies boolean];
a = [42, undefined, true as boolean];
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    let ts2322_messages = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type '[number, undefined, true]'")),
        "Expected tuple source display to preserve true literal, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type '[number, undefined, false]'")),
        "Expected tuple source display to preserve false literal, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .filter(|message| message.contains("Type '[number, undefined, true]'"))
            .count()
            == 4,
        "Boolean-compatible tuple targets should preserve direct, parenthesized, const-asserted, and satisfies true literals, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type '[number, undefined, boolean]'")),
        "Expected explicit boolean assertion to display as boolean, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .filter(|message| message.contains("[number, undefined, boolean]"))
            .count()
            == 1,
        "Only explicit boolean assertions should display widened boolean literal elements, got: {diagnostics:#?}"
    );
}

#[test]
fn exact_optional_tuple_source_display_uses_contextual_literal_policy_per_element() {
    let source = r#"
declare let primitiveString: [number, boolean?, string?];
declare let literalString: [number, boolean?, "x"?];
declare let literalNumber: [number, boolean?, 1?];
primitiveString = [42, undefined, "x"];
literalString = [42, undefined, "x"];
literalNumber = [42, undefined, 1];
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    let ts2322_messages = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type '[number, undefined, string]'")),
        "Expected primitive contextual string to display as string, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type '[number, undefined, \"x\"]'")),
        "Expected literal contextual string to stay literal, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type '[number, undefined, 1]'")),
        "Expected literal contextual number to stay literal, got: {diagnostics:#?}"
    );
}

#[test]
fn variadic_tuple_source_display_maps_middle_positions_to_rest_before_suffix() {
    let source = r#"
declare let target: [number, ...boolean[], string];
target = [1, true, false, true, false];
"#;
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    let ts2322_messages = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type '[number, true, false, true, false]'")),
        "Expected variadic boolean tuple slots to preserve literal source display, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .all(|message| !message.contains("Type '[number, true, false, boolean, boolean]'")),
        "Middle positions in variadic+suffix tuples must not map to trailing fixed suffix slots, got: {diagnostics:#?}"
    );
}

#[test]
fn non_exact_optional_tuple_elements_still_accept_present_undefined() {
    let source = r#"
declare let t: [number, string?, boolean?];
t[1] = undefined;
t = [1, undefined];
t = [1, "ok", undefined];
t = [1, undefined, undefined];
"#;
    let options = CheckerOptions {
        exact_optional_property_types: false,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected non-exact optional tuple slots to accept present undefined, got: {diagnostics:#?}"
    );
}

#[test]
fn duplicate_block_scoped_and_var_reference_uses_first_value_declaration_type() {
    let source = r#"
declare const duplicateValue: string | boolean | undefined;
declare var duplicateValue: { a: number; b?: string | undefined };
declare var stringNumberMap: { [x: string]: number | string };
stringNumberMap = duplicateValue;
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    assert!(
        diagnostics
            .iter()
            .any(|(code, _)| { *code == diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE }),
        "Expected duplicate declarations to still emit TS2451, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("Type 'string | boolean | undefined'")
                && message.contains("'{ [x: string]: string | number; }'")
        }),
        "Expected assignment to use the first value declaration's union type, got: {diagnostics:#?}"
    );
}

#[test]
fn exact_optional_tuple_inference_preserves_explicit_undefined_element() {
    let source = r#"
declare let tx2: [string | undefined];
declare let tx4: [(string | undefined)?];
declare function f12<T>(x: [T?]): T;
declare function f13<T>(x: Partial<T>): T;
f12(tx2);
f12(tx4);
f13(tx2);
f13(tx4);
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    assert!(
        diagnostics.iter().all(|(code, _)| *code
            != diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE),
        "Expected generic optional tuple inference to preserve explicit undefined, got: {diagnostics:#?}"
    );
}

#[test]
fn jsdoc_typedef_body_display_alias_does_not_expand_ts2322_target() {
    let source = r#"
/**
 * @typedef {{ value: number }} MyAlias
 */

/** @type {MyAlias} */
const a = 1;
"#;
    let diagnostics = with_lib_contexts(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for the typedef target, got: {diagnostics:#?}"
    );
    let message = &ts2322[0].1;
    assert!(
        message.contains("type 'MyAlias'"),
        "TS2322 target display should preserve the JSDoc typedef name, got: {message:?}"
    );
    assert!(
        !message.contains("type '{ value: number; }'"),
        "TS2322 target display should not expand the JSDoc typedef body, got: {message:?}"
    );
}

/// Regression test for `widen_fresh_object_literal_properties_for_display`:
/// the helper must only widen literal property types when the outer object
/// is itself a *fresh* object literal. Annotated types like `{ a: "x" }`
/// carry the user's intent and must not have their literal property types
/// widened away in TS2741/TS2345 diagnostics.
///
/// Before the fix, `widen_fresh_object_literal_properties_for_display`
/// always widened all literal properties regardless of freshness, so the
/// annotated parameter type `{ a: "x" }` was rendered as `{ a: string; }`
/// in TS2345/TS2741 diagnostics — diverging from `tsc`, which preserves
/// `{ a: "x" }` because the user wrote it that way.
#[test]
fn test_ts2741_annotated_literal_target_preserves_literal_property() {
    let source = r#"
const fn1 = (s: { a: "x" }) => {};
fn1({});
"#;
    let diagnostics = get_all_diagnostics(source);
    let target_messages: Vec<&str> = diagnostics
        .iter()
        .map(|(_, message)| message.as_str())
        .collect();
    assert!(
        target_messages
            .iter()
            .any(|m| m.contains("'{ a: \"x\"; }'")),
        "expected annotated literal target `{{ a: \"x\"; }}` to be preserved verbatim, got: {target_messages:#?}"
    );
    assert!(
        !target_messages
            .iter()
            .any(|m| m.contains("'{ a: string; }'")),
        "annotated `{{ a: \"x\" }}` must not be widened to `{{ a: string; }}` in diagnostics, got: {target_messages:#?}"
    );
}

/// Regression for `inferenceShouldFailOnEvolvingArrays.ts`:
///
/// Calling a generic function whose parameter type is `{ [K in U]: T }[U]`
/// (e.g. `function f<T extends string[], U extends string>(arg: { [K in U]: T }[U])`)
/// with an array literal whose element types violate the constraint of `T`
/// (e.g. `f([42])` against `T extends string[]`) should produce a TS2322
/// element-level error pointing at the offending element, matching tsc's
/// behavior. Previously the elaboration was suppressed because the *raw*
/// parameter type contains type parameters; the elaboration must still run
/// when the resolved source/target types are concrete.
#[test]
fn ts2322_array_element_elaborated_when_generic_param_resolves_to_concrete_constraint() {
    let source = r#"
function logFirstLength<T extends string[], U extends string>(arg: { [K in U]: T }[U]): T {
    return arg;
}
logFirstLength([42]);
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);

    let ts2322_count = diagnostic_count(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    let ts2345_count = diagnostics
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .count();

    assert!(
        ts2322_count >= 1,
        "Expected at least one TS2322 element elaboration for array-literal arg in generic call, got 0. Diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2345_count, 0,
        "Expected no TS2345 on the whole array argument once element-level TS2322 is emitted. Diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, msg)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && msg.contains("'number'")
                && msg.contains("'string'")
        }),
        "Expected TS2322 message mentioning 'number' and 'string' for the array element, got: {diagnostics:#?}"
    );
}

#[test]
fn no_infer_wrapped_weak_type_in_intersection_target_emits_ts2559() {
    // `NoInfer<T>` is a transparent wrapper for shape extraction. An
    // intersection like `NoInfer<W> & { prop?: unknown }` where `W` is a
    // weak type must still trigger TS2559 when the source has no
    // overlapping properties.
    let source = r#"
        type W = { alpha?: unknown; beta?: unknown };
        declare const weakObj: W;
        declare const someObj: { x: string };
        declare function callee<T>(a: T, b: NoInfer<T> & { prop?: unknown }): void;
        callee(weakObj, someObj);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for {{ x: string }} against NoInfer<T> & {{ prop?: unknown }} target. Got: {diagnostics:?}"
    );
}

#[test]
fn no_infer_intersection_of_two_no_infers_emits_ts2559() {
    // `NoInfer<U> & NoInfer<V>` is weak when both inner types are weak.
    // Use distinct generic parameter names to confirm the rule is
    // structural, not name-based.
    let source = r#"
        type WA = { alpha?: unknown; beta?: unknown };
        type WB = { gamma?: unknown; delta?: unknown };
        declare const weakA: WA;
        declare const weakB: WB;
        declare const someObj: { x: string };
        declare function callee<U, V>(
            a: U,
            b: V,
            c: NoInfer<U> & NoInfer<V>,
        ): void;
        callee(weakA, weakB, someObj);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for {{ x: string }} against NoInfer<U> & NoInfer<V> target. Got: {diagnostics:?}"
    );
}

#[test]
fn test_const_destructured_computed_property_not_narrowed_by_flow() {
    // Const destructured bindings with computed property keys should keep
    // their full declared type (union of all possible values) and not be
    // narrowed through flow-dependent computed property key variables.
    //
    // Rule: const bindings declared via destructuring have their type fixed
    // at declaration time. Flow analysis must not recompute the type from
    // the destructuring source using already-narrowed types of other
    // variables, because that discards union members introduced by
    // default-initializer widening.
    let source = r#"
let a: 0 | 1 = 1;
const [{ [a]: b } = [a = 0, 9] as const] = [[8, 9] as const];
const bb: 0 | 8 = b;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_msgs: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert_eq!(
        ts2322_msgs.len(),
        1,
        "Expected exactly one TS2322 for wrong assignment, got: {diagnostics:#?}"
    );

    // The source type must be '0 | 8 | 9' (full union), not '9' (narrowed).
    assert!(
        ts2322_msgs[0].contains("0 | 8 | 9"),
        "Expected source type '0 | 8 | 9' (full union), got: {}",
        ts2322_msgs[0],
    );
    assert!(
        ts2322_msgs[0].contains("0 | 8"),
        "Expected target type '0 | 8', got: {}",
        ts2322_msgs[0],
    );
}

#[test]
fn test_destructured_parameter_in_const_fn_is_not_treated_as_const() {
    // Regression: `is_const_symbol` walked past PARAMETER/ARROW_FUNCTION
    // boundaries to the enclosing `const fn = …` VARIABLE_DECLARATION,
    // wrongly classifying the parameter as const. This caused
    // `analyze_loop_fixed_point` to skip the iteration and emit stale
    // narrowed types for parameters reassigned inside loops.
    //
    // The walk must terminate when it encounters PARAMETER, FUNCTION_*,
    // CLASS_*, or SOURCE_FILE — those are scope boundaries past which the
    // symbol is no longer the variable being declared.
    let source = r#"
const fn = ({ x }: { x: number | string }) => {
    while (Math.random() < 0.5) {
        x = "next";
    }
    const y: number | string = x;
    return y;
};
"#;
    let diagnostics = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert_eq!(
        ts2322, 0,
        "Destructured parameter in const-fn must not be skipped by fixed-point iteration; \
         got TS2322 diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2322_too_many_parameters_emits_chained_target_signature_elaboration() {
    // When a function-typed source has more required parameters than the target
    // accepts, tsc emits TS2322 with a chained sub-message:
    //
    //   error TS2322: Type '...' is not assignable to type '...'.
    //     Target signature provides too few arguments. Expected N or more, but got M.
    //
    // The chained message has its own diagnostic code (TS2849), but is rendered
    // as related-information on the parent TS2322 so the final output matches
    // tsc's `messageText` chain. Without the elaboration the user only sees the
    // top-level "Type X is not assignable to Y" message, which is harder to
    // diagnose for callback / mapped-type contextual mismatches.
    let source = r#"
        type Selector<S, R> = (state: S) => R;
        const f: Selector<string, number> = (state: string, props: string) => 1;
    "#;

    let diags = diagnostics_for_source(source);
    let mismatch = diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("expected TS2322, got: {diags:#?}"));

    assert!(
        mismatch
            .related_information
            .iter()
            .any(|r| r.code == diagnostic_codes::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT
                && r.message_text.contains("Target signature provides too few arguments")
                && r.message_text.contains("Expected 2 or more, but got 1")),
        "expected chained TS2849 'Target signature provides too few arguments' \
         elaboration with counts (2,1); got: {:#?}",
        mismatch.related_information
    );
}

#[test]
fn test_reverse_mapped_contextual_target_display_uses_inferred_application_args() {
    let source = r#"
        type Selector<S, R> = (state: S) => R;

        declare function createStructuredSelector<S, T>(
            selectors: {[K in keyof T]: Selector<S, T[K]>},
        ): Selector<S, T>;

        const editable = () => ({});

        const mapStateToProps = createStructuredSelector({
            editable: (state: any, props: any) => editable(),
        });
    "#;

    let diags = diagnostics_for_source(source);
    let mismatch = diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("expected TS2322, got: {diags:#?}"));

    assert!(
        mismatch.message_text.contains("Selector<unknown, {}>"),
        "expected contextual target display to use inferred application args; got: {mismatch:#?}"
    );
    assert!(
        !mismatch
            .message_text
            .contains("Selector<S, T[\"editable\"]>"),
        "target display should not expose unresolved reverse-mapped type parameters; got: {mismatch:#?}"
    );
}

#[test]
fn test_reverse_mapped_contextual_target_display_is_structural_for_renamed_params() {
    let source = r#"
        type PickResult<Store, Result> = (store: Store) => Result;

        declare function buildSelectors<Store, Shape>(
            selectors: {[Key in keyof Shape]: PickResult<Store, Shape[Key]>},
        ): PickResult<Store, Shape>;

        const getTitle = () => "title";

        const selectors = buildSelectors({
            title: (store: any, extra: any) => getTitle(),
        });
    "#;

    let diags = diagnostics_for_source(source);
    let mismatch = diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("expected TS2322, got: {diags:#?}"));

    assert!(
        mismatch
            .message_text
            .contains("PickResult<unknown, string>"),
        "expected contextual target display to use inferred application args; got: {mismatch:#?}"
    );
    assert!(
        !mismatch.message_text.contains("Shape[\"title\"]"),
        "target display should not expose unresolved reverse-mapped indexed access; got: {mismatch:#?}"
    );
}

// =============================================================================
// @ts-nocheck / @ts-check pragma: must only honour directives in comments
// (issue #2821)
// =============================================================================

#[test]
fn test_ts_nocheck_in_string_literal_does_not_suppress_ts2322() {
    // A string literal containing "@ts-nocheck" must NOT suppress checking.
    // Only a `// @ts-nocheck` or `/* @ts-nocheck */` comment in the leading
    // trivia of the file suppresses diagnostics.
    let source = r#"const marker = "@ts-nocheck";
const n: number = "not a number";
"#;
    let diags = compile_with_options(source, "test.ts", CheckerOptions::default());
    assert!(
        has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "@ts-nocheck inside a string literal must not suppress TS2322; got: {diags:?}"
    );
}

#[test]
fn test_ts_nocheck_in_real_comment_suppresses_checking() {
    // Sanity check: a genuine `// @ts-nocheck` leading comment should still
    // suppress diagnostics (the pre-existing behaviour must be preserved).
    let source = r#"// @ts-nocheck
const n: number = "not a number";
"#;
    let diags = compile_with_options(source, "test.ts", CheckerOptions::default());
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "// @ts-nocheck in leading comment should suppress TS2322; got: {diags:?}"
    );
}

#[test]
fn test_ts_nocheck_after_code_does_not_suppress_ts2322() {
    // A `// @ts-nocheck` comment that appears *after* real code is not
    // a leading-trivia directive and must not suppress subsequent errors.
    let source = r#"const marker = 1;
// @ts-nocheck
const n: number = "not a number";
"#;
    let diags = compile_with_options(source, "test.ts", CheckerOptions::default());
    assert!(
        has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "@ts-nocheck after real code must not suppress TS2322; got: {diags:?}"
    );
}

