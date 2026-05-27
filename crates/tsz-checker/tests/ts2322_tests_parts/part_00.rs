#[test]
fn callable_interface_call_signature_returning_this_preserves_members() {
    let source = r#"
interface Chainable {
  (): this;
  value: number;
}

declare const chain: Chainable;
const c = chain();
const _c: Chainable = c;
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);

    assert!(
        diagnostics.is_empty(),
        "Callable interface `this` return should preserve interface members. Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn iterator_result_with_undefined_return_rejects_required_value_target() {
    let diagnostics = with_lib_contexts(
        r#"
interface IteratorYieldResult<TYield> {
    done?: false;
    value: TYield;
}
interface IteratorReturnResult<TReturn> {
    done: true;
    value: TReturn;
}
type IteratorResult<T, TReturn = any> =
    | IteratorYieldResult<T>
    | IteratorReturnResult<TReturn>;

interface Next<A> {
    readonly done?: boolean;
    readonly value: A;
}

declare const result: IteratorResult<number, undefined>;
const r: Next<number> = result;
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "Expected IteratorResult<number, undefined> to reject Next<number>, got: {diagnostics:?}"
    );
}

#[test]
fn custom_iterator_result_name_does_not_trigger_iterator_result_required_value_rule() {
    let diagnostics = with_lib_contexts(
        r#"
interface MyIteratorYieldResult<TYield> {
    done?: false;
    value: TYield;
}
interface MyIteratorReturnResult<TReturn> {
    done: true;
    value: TReturn;
}
type MyIteratorResult<T, TReturn = any> =
    | MyIteratorYieldResult<T>
    | MyIteratorReturnResult<TReturn>;

interface Next<A> {
    readonly done?: boolean;
    readonly value: A;
}

declare const result: MyIteratorResult<number, undefined>;
const r: Next<number> = result;
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "Renamed IteratorResult-like aliases should still reject through normal structural assignability, got: {diagnostics:?}"
    );
}

#[test]
fn promise_suffixed_generic_wrapper_does_not_suppress_nested_argument_mismatch() {
    let diagnostics = get_all_diagnostics(
        r#"
interface NotPromise<T> {
    value: T;
}

declare const nested: NotPromise<NotPromise<number>>;

const flattened: NotPromise<number> = nested;
flattened;
"#,
    );

    let ts2322 = diagnostics.iter().find(|(code, message)| {
        *code == 2322
            && message.contains("NotPromise<NotPromise<number>>")
            && message.contains("NotPromise<number>")
    });
    assert!(
        ts2322.is_some(),
        "expected TS2322 for ordinary Promise-suffixed generic wrapper, got: {diagnostics:?}"
    );
}

#[test]
fn local_symbol_call_initializer_uses_local_return_type() {
    let diagnostics = get_all_diagnostics(
        r#"
function test() {
    const Symbol = () => "local";

    const value = Symbol();

    const asSymbol: symbol = value;
    const asString: string = value;

    asSymbol;
    asString;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'string' is not assignable to type 'symbol'.")
        }),
        "expected local Symbol() to infer string and reject symbol assignment, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("unique symbol") && message.contains("string")
        }),
        "local Symbol() should not infer unique symbol, got: {diagnostics:?}"
    );
}

#[test]
fn symbol_primitive_methods_report_assignability_errors() {
    let diagnostics = get_all_diagnostics(
        r#"
declare const sym: symbol;
const s: string = sym.valueOf();
const n: number = sym.toString();
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("Type 'symbol' is not assignable to type 'string'.")
        }),
        "expected symbol.valueOf() to reject string assignment, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("Type 'string' is not assignable to type 'number'.")
        }),
        "expected symbol.toString() to reject number assignment, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_index_accesses_with_distinct_key_type_parameters() {
    let diagnostics = get_all_diagnostics(
        r#"
        declare namespace JSX {
            interface IntrinsicElements {
                div: { divOnly?: string };
                span: { spanOnly?: string };
            }
        }

        class I<
            T1 extends keyof JSX.IntrinsicElements,
            T2 extends keyof JSX.IntrinsicElements
        > {
            M() {
                let c1: JSX.IntrinsicElements[T1] = {};
                const c2: JSX.IntrinsicElements[T2] = c1;
            }
        }
    "#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains(
                    "Type 'IntrinsicElements[T1]' is not assignable to type 'IntrinsicElements[T2]'.",
                )
        }),
        "Expected TS2322 for independent JSX.IntrinsicElements indexed accesses, got: {diagnostics:#?}"
    );
}

/// Adjacent-case test matrix for the `S[T1]` vs `S[T2]` distinct-type-parameter
/// elaboration (issue #7647).
///
/// Structural rule: when an index access `S[T1]` is assigned to `S[T2]` and
/// `T1`, `T2` are distinct type parameters with relatable constraints, tsc
/// emits the TS2322 chain:
///
/// ```text
/// Type 'S[T1]' is not assignable to type 'S[T2]'.
///   Type 'T1' is not assignable to type 'T2'.
///     'T1' is assignable to the constraint of type 'T2', but 'T2' could be
///     instantiated with a different subtype of constraint '<constraint>'.
/// ```
///
/// The expected elaboration is independent of the chosen parameter names —
/// the tests below rename the parameters and the object type to prove
/// the rule isn't keyed on identifier spelling (anti-hardcoding §25).
mod index_access_type_parameter_mismatch_elaboration {
    use super::*;
    use tsz_checker::test_utils::check_source_diagnostics;

    fn assert_index_access_elaboration_chain(
        source: &str,
        source_param: &str,
        target_param: &str,
        constraint_display: &str,
    ) {
        let diagnostics = check_source_diagnostics(source);
        let primary = diagnostics
            .iter()
            .find(|d| {
                d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                    && d.message_text
                        .contains("is not assignable to type")
                    && !d.message_text.contains(&format!("Type '{source_param}'"))
            })
            .unwrap_or_else(|| {
                panic!(
                    "Expected primary TS2322 diagnostic for index-access mismatch; got: {diagnostics:#?}"
                )
            });
        let related: Vec<(u32, &str)> = primary
            .related_information
            .iter()
            .map(|r| (r.code, r.message_text.as_str()))
            .collect();
        let inner_msg =
            format!("Type '{source_param}' is not assignable to type '{target_param}'.");
        assert!(
            related.iter().any(|(code, msg)| {
                let expected_code = if source_param == target_param {
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY
                } else {
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                };
                *code == expected_code && msg.contains(&inner_msg)
            }),
            "Expected related parameter mismatch with `{inner_msg}`; got related: {related:?}"
        );
        if source_param == target_param {
            assert!(
                related.iter().any(|(_, msg)| msg
                    .contains("Two different types with this name exist, but they are unrelated.")),
                "Expected same-spelling distinct parameters to mention unrelated names; got related: {related:?}"
            );
        }
        let ts5075_msg = format!(
            "'{source_param}' is assignable to the constraint of type '{target_param}', but '{target_param}' could be instantiated with a different subtype of constraint '{constraint_display}'."
        );
        assert!(
            related.iter().any(|(_, msg)| msg.contains(&ts5075_msg)),
            "Expected TS5075 elaboration `{ts5075_msg}`; got related: {related:?}"
        );
    }

    /// Reported repro shape: `JSX.IntrinsicElements[T1]` vs `[T2]`.
    #[test]
    fn jsx_intrinsic_elements_index_access_emits_elaboration_chain() {
        assert_index_access_elaboration_chain(
            r#"
            declare namespace JSX {
                interface IntrinsicElements {
                    div: { divOnly?: string };
                    span: { spanOnly?: string };
                }
            }
            class I<
                T1 extends keyof JSX.IntrinsicElements,
                T2 extends keyof JSX.IntrinsicElements
            > {
                M() {
                    let c1: JSX.IntrinsicElements[T1] = {};
                    const c2: JSX.IntrinsicElements[T2] = c1;
                }
            }
            "#,
            "T1",
            "T2",
            "keyof IntrinsicElements",
        );
    }

    /// Same rule with renamed parameters proves the elaboration is structural,
    /// not hardcoded to the `T1`/`T2` spelling.
    #[test]
    fn renamed_parameters_still_emit_elaboration_chain() {
        assert_index_access_elaboration_chain(
            r#"
            declare namespace JSX {
                interface IntrinsicElements {
                    div: { divOnly?: string };
                    span: { spanOnly?: string };
                }
            }
            class Holder<
                Source extends keyof JSX.IntrinsicElements,
                Target extends keyof JSX.IntrinsicElements
            > {
                M() {
                    let a: JSX.IntrinsicElements[Source] = {};
                    const b: JSX.IntrinsicElements[Target] = a;
                }
            }
            "#,
            "Source",
            "Target",
            "keyof IntrinsicElements",
        );
    }

    /// A user-defined object type (no JSX namespace) and single-letter
    /// rename: the constraint display follows the *target's* constraint.
    #[test]
    fn user_defined_object_index_access_uses_target_constraint() {
        assert_index_access_elaboration_chain(
            r#"
            interface Map {
                a: { x: number };
                b: { y: string };
            }
            class Pair<
                K1 extends keyof Map,
                K2 extends keyof Map
            > {
                M() {
                    let lhs: Map[K1] = { x: 0, y: "" } as any;
                    const rhs: Map[K2] = lhs;
                }
            }
            "#,
            "K1",
            "K2",
            "keyof Map",
        );
    }

    /// Same spelling in nested scopes still represents distinct type parameters.
    #[test]
    fn nested_same_spelling_parameters_still_emit_elaboration_chain() {
        assert_index_access_elaboration_chain(
            r#"
            interface Dict {
                a: { aOnly: string };
                b: { bOnly: number };
            }

            function outer<K extends keyof Dict>(x: Dict[K]) {
                function inner<K extends keyof Dict>(y: Dict[K]) {
                    y = x;
                }
            }
            "#,
            "K",
            "K",
            "keyof Dict",
        );
    }

    /// Negative case: same type parameter on both sides is reflexive — no diagnostic.
    /// Proves the structural rule fires only on *distinct* type parameters.
    #[test]
    fn same_type_parameter_does_not_emit_elaboration() {
        let source = r#"
            declare namespace JSX {
                interface IntrinsicElements {
                    div: { divOnly?: string };
                }
            }
            class Same<T extends keyof JSX.IntrinsicElements> {
                M() {
                    let a: JSX.IntrinsicElements[T] = {};
                    const b: JSX.IntrinsicElements[T] = a;
                }
            }
        "#;
        let diagnostics = check_source_diagnostics(source);
        let ts2322_count = diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            ts2322_count, 0,
            "Same type-parameter index accesses must not emit TS2322; got: {diagnostics:#?}"
        );
    }
}

#[test]
fn test_ts2322_identifier_literal_initializer_display_for_literal_sensitive_targets() {
    let diagnostics = get_all_diagnostics(
        r#"
var x = true;
var n: number = x;
var u: typeof undefined = x;
enum E { A }
var e: E = x;
var s = "value";
var su: typeof undefined = s;
var i = 1;
var iu: typeof undefined = i;
"#,
    );
    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'boolean' is not assignable to type 'number'.")),
        "expected widened boolean display for non-literal target, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'true' is not assignable to type 'undefined'.")),
        "expected literal initializer display for undefined target, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'true' is not assignable to type 'E'.")),
        "expected literal initializer display for enum target, got: {ts2322:#?}"
    );
    assert!(
        ts2322.iter().any(
            |message| message.contains("Type 'string' is not assignable to type 'undefined'.")
        ),
        "expected string initializer display to remain widened, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'number' is not assignable to type 'undefined'.")),
        "expected numeric initializer display to remain widened, got: {ts2322:#?}"
    );
}

#[test]
fn typeof_mutable_object_property_widens_literal_value() {
    let source = r#"
const obj = { a: 1, b: "x" };
type ObjAType = typeof obj.a;
const _oa: ObjAType = 42;

const objConst = { a: 1 } as const;
type ObjConstAType = typeof objConst.a;
const _oc: ObjConstAType = 2;
"#;

    let diagnostics = with_lib_contexts(source, "test.ts", CheckerOptions::default());
    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 2322).count(),
        1,
        "expected only the as-const property assignment to fail, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].1.contains("not assignable to type '1'"),
        "expected as-const property to remain literal, got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2322_type_parameter_union_display_preserves_declaration_order() {
    let diagnostics = get_all_diagnostics(
        r#"
function diamondTop<Top>() {
    function diamondMiddle<T, U>() {
        let top!: Top;
        let middle!: Top | T | U;
        top = middle;
    }
}
"#,
    );

    let message = diagnostics
        .iter()
        .find_map(|(code, message)| {
            (*code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("is not assignable to type 'Top'."))
            .then_some(message.as_str())
        })
        .expect("expected TS2322 diagnostic for top = middle assignment");

    assert!(
        message.contains("Type 'Top | T | U' is not assignable to type 'Top'."),
        "expected declaration-order union display, got: {message}"
    );
}

#[test]
fn test_ts2322_narrowed_string_literal_residual_union_to_never_display() {
    let diagnostics = get_all_diagnostics(
        r#"
type Variants = "a" | "b" | "c" | "d";

function fx1(x: Variants) {
    if (x === "a" || x === "b") {
    } else {
        const y: never = x;
    }
}
"#,
    );

    let message = diagnostics
        .iter()
        .find_map(|(code, message)| {
            (*code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("is not assignable to type 'never'."))
            .then_some(message.as_str())
        })
        .expect("expected TS2322 diagnostic for narrowed residual union assigned to never");

    assert!(
        message.contains(r#"Type '"d" | "c"' is not assignable to type 'never'."#),
        "expected residual string-literal union display to match tsc, got: {message}"
    );
}

#[test]
fn test_ts2322_numeric_literal_union_alias_source_display_preserved() {
    let diagnostics = get_all_diagnostics(
        r#"
type Single = 1;
type Count = 1 | 2 | 3;
type Offset = 0 | 1 | 2;

function assign(single: Single, count: Count, offset: Offset) {
    single = count;
    single = offset;
}
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'Count' is not assignable to type '1'.")),
        "numeric union source aliases should survive TS2322 display, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'Offset' is not assignable to type '1'.")),
        "numeric union source aliases should survive TS2322 display, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .all(|message| { !message.contains("2 | 3 | 1") && !message.contains("0 | 2 | 1") }),
        "numeric union canonicalization must not expand preserved source aliases: {ts2322:#?}"
    );
}

#[test]
fn test_ts2322_same_enum_member_union_source_display_collapses_to_enum() {
    let diagnostics = get_all_diagnostics(
        r#"
enum E {
    A = "a",
    B = "b",
}
declare let both: E.A | E.B;
let onlyA: E.A = both;
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("is not assignable to type 'E.A'.")
        })
        .expect("expected TS2322 for assigning E.A | E.B to E.A");

    let message = ts2322.1.as_str();
    assert!(
        message.contains("Type 'E' is not assignable to type 'E.A'."),
        "expected same-enum member union source to collapse to parent enum, got: {message}"
    );
}

#[test]
fn test_ts2322_enum_member_union_proper_subset_renders_member_union() {
    // `E.A | E.B` is a proper subset of the three-member enum `E`, so tsc
    // renders the target as the member union, not the bare enum name `E`.
    let messages = ts2322_messages(
        r#"
enum E { A, B, C }
declare const e: E;
const x: E.A | E.B = e;
"#,
    );

    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type 'E' is not assignable to type 'E.A | E.B'.")),
        "expected proper-subset enum member union target to render as 'E.A | E.B', got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|m| !m.contains("is not assignable to type 'E'.")),
        "proper-subset enum member union must not collapse to bare 'E', got: {messages:#?}"
    );
}

#[test]
fn test_ts2345_enum_member_union_proper_subset_renders_member_union() {
    // Same rule on the TS2345 parameter path.
    let messages: Vec<String> = get_all_diagnostics(
        r#"
enum E { A, B, C }
declare const e: E;
function f(x: E.A | E.B) {}
f(e);
"#,
    )
    .into_iter()
    .filter_map(|(code, message)| (code == 2345).then_some(message))
    .collect();

    assert!(
        messages
            .iter()
            .any(|m| m.contains("parameter of type 'E.A | E.B'")),
        "expected TS2345 parameter to render as 'E.A | E.B', got: {messages:#?}"
    );
}

#[test]
fn test_ts2322_enum_member_union_covering_all_members_collapses_to_enum() {
    // A union covering every member of the enum may collapse to the bare
    // enum name, matching tsc.
    let messages = ts2322_messages(
        r#"
enum E { A, B, C }
declare const e: E;
const x: E.A | E.B | E.C = "nope";
"#,
    );

    assert!(
        messages
            .iter()
            .any(|m| m.contains("is not assignable to type 'E'.")),
        "expected full-coverage enum member union to collapse to bare 'E', got: {messages:#?}"
    );
}

#[test]
fn test_ts2322_enum_member_union_subset_renamed_enum() {
    // The rule is structural, not keyed on the spelling `E`/`A`/`B`.
    let messages = ts2322_messages(
        r#"
enum Color { Red, Green, Blue }
declare const c: Color;
const x: Color.Red | Color.Green = c;
"#,
    );

    assert!(
        messages.iter().any(
            |m| m.contains("Type 'Color' is not assignable to type 'Color.Red | Color.Green'.")
        ),
        "expected renamed enum subset to render as 'Color.Red | Color.Green', got: {messages:#?}"
    );
}

#[test]
fn test_ts2322_numeric_literal_union_alias_source_display_preserved_for_property_assignment() {
    let diagnostics = get_all_diagnostics(
        r#"
interface Slot {
    value: 10;
}
type Choices = 10 | 20 | 30;

function write(slot: Slot, choices: Choices) {
    slot.value = choices;
}
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type 'Choices' is not assignable to type '10'.")),
        "property assignment should also preserve the numeric union source alias, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .all(|message| !message.contains("20 | 30 | 10")),
        "property assignment should not expand the preserved alias into reordered numeric members: {ts2322:#?}"
    );
}

fn compile_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    with_lib_contexts(source, file_name, options)
}

fn compile_with_libs_for_ts(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn test_object_source_missing_date_properties_not_downgraded_to_ts2322() {
    let source = r#"
function isDate(x: object) {
  return x instanceof Date;
}

function flakyIsDate(x: object) {
  return x instanceof Date && Math.random() > 0.5;
}

declare let maybeDate: object;
if (isDate(maybeDate)) {
  let t: Date = maybeDate;
} else {
  let t: object = maybeDate;
}

if (flakyIsDate(maybeDate)) {
  let t: Date = maybeDate;
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        }
        .apply_strict_defaults(),
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
                && message
                    .contains("Type '{}' is missing the following properties from type 'Date'")
        }),
        "expected object-source Date mismatch to use TS2740 missing-properties display; diagnostics={diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("Type 'object' is not assignable to type 'Date'")
        }),
        "object-source Date mismatch should not be downgraded to TS2322; diagnostics={diagnostics:#?}"
    );
}

fn diagnostics_for_source(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let file_name = "test.ts".to_string();
    let mut parser = ParserState::new(file_name.clone(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();
    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name,
        CheckerOptions::default(),
    );
    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

// =============================================================================
// Return Statement Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_return_wrong_primitive() {
    let source = r#"
        function returnNumber(): number {
            return "string";
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_return_wrong_object_property() {
    let source = r#"
        function returnObject(): { a: number } {
            return { a: "string" };
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_return_wrong_array_element() {
    let source = r#"
        function returnArray(): number[] {
            return ["string"];
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_promise_is_assignable_to_promise_like_with_real_libs() {
    let libs = load_lib_files_for_test();
    if libs.is_empty() {
        return; // lib files not available
    }
    let source = r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#;

    let diagnostics = diagnostics_for_source(source);
    let relevant: Vec<_> = diagnostics.iter().filter(|d| d.code != 2318).collect();

    assert!(
        relevant.is_empty(),
        "Expected Promise<T> to be assignable to PromiseLike<T>, got: {relevant:?}"
    );
}

#[test]
fn unrelated_thenable_application_requires_compatible_then_signature() {
    let source = r#"
interface ExpectedThenable<T> {
    then<U>(cb: (value: T) => U): ExpectedThenable<U>;
}

interface BadThenable<T> {
    then(): void;
}

declare const bad: BadThenable<number>;
const target: ExpectedThenable<number> = bad;
"#;

    let diagnostics = diagnostics_for_source(source);

    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "Expected TS2322 for unrelated thenables with incompatible then signatures, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_return_alias_instantiation_mismatch() {
    let source = r#"
        type Box<T> = { value: T };

        function returnBox(): Box<number> {
            const box: Box<string> = { value: "x" };
            return box;
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn mapped_type_inference_from_apparent_type_reports_ts2322() {
    let source = r#"
type Obj = {
    [s: string]: number;
};

type foo = <T>(target: { [K in keyof T]: T[K] }) => void;
type bar = <U extends string[]>(source: { [K in keyof U]: Obj[K] }) => void;

declare let f: foo;
declare let b: bar;
b = f;
"#;

    assert!(
        has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "generic mapped assignment should preserve the apparent array constraint and report TS2322"
    );
}

#[test]
fn generic_signature_assignment_reports_expected_ts2322s() {
    let source = r#"
type A3 = <T>(x: T) => void;
type B3 = <T>(x: T) => T;
declare let a3: A3;
declare let b3: B3;
a3 = b3;
b3 = a3;

type A11 = <T>(x: { foo: T }, y: { foo: T; bar: T }) => void;
type B11 = <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => void;
declare let a11: A11;
declare let b11: B11;
a11 = b11;
b11 = a11;

type Base = { foo: string };
type A16 = <T extends Base>(x: { a: T; b: T }) => T[];
type B16 = <T>(x: { a: T; b: T }) => T[];
declare let a16: A16;
declare let b16: B16;
a16 = b16;
b16 = a16;
"#;

    let options = CheckerOptions {
        strict_null_checks: true,
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    };
    let diagnostics = with_lib_contexts(source, "test.ts", options);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse generic signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'A3' is not assignable to type 'B3'")),
        "Expected the void-return reverse assignment to surface as the A3/B3 TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'A11' is not assignable to type 'B11'")),
        "Expected the mismatched correlated generic assignment to surface as the A11/B11 TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'A16' is not assignable to type 'B16'")),
        "Expected the constrained generic reverse assignment to surface as the A16/B16 TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn recursive_generic_signature_assignment_reports_only_tsc_direction() {
    let source = r#"
interface I2<T> { p: T }
declare var x: <T extends I2<T>>(z: T) => void;
declare var y: <T extends I2<I2<T>>>(z: T) => void;
x = y;
y = x;
"#;

    let diagnostics = with_lib_contexts(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        1,
        "Expected only the reverse recursive generic assignment to report TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors[0].1.contains(
            "Type '<T extends I2<T>>(z: T) => void' is not assignable to type '<T extends I2<I2<T>>>(z: T) => void'"
        ),
        "Expected the y = x diagnostic to match TypeScript, got: {ts2322_errors:?}"
    );
}

#[test]
fn polymorphic_this_constraint_invariance_reports_ts2322() {
    let source = r#"
const wat: Runtype<any> = Num;
const Foo = Obj({ foo: Num })

interface Runtype<A> {
  constraint: Constraint<this>
  witness: A
}

interface Num extends Runtype<number> {
  tag: 'number'
}
declare const Num: Num

interface Obj<O extends { [_ in string]: Runtype<any> }> extends Runtype<{[K in keyof O]: O[K]['witness'] }> {}
declare function Obj<O extends { [_: string]: Runtype<any> }>(fields: O): Obj<O>;

interface Constraint<A extends Runtype<any>> extends Runtype<A['witness']> {
  underlying: A,
  check: (x: A['witness']) => void,
}
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    let ts2345_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        2,
        "Expected the assignment and object property to report TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2345_errors.is_empty(),
        "Expected no whole-argument TS2345 diagnostic, got: {diagnostics:?}"
    );
    assert!(
        ts2322_errors.iter().all(|(_, message)| {
            message.contains("Type 'Num' is not assignable to type 'Runtype<any>'")
        }),
        "Expected both TS2322 diagnostics to explain Num vs Runtype<any>, got: {diagnostics:?}"
    );
}

#[test]
fn generic_construct_signature_assignment_reports_expected_ts2322s() {
    let source = r#"
type Base = { foo: string };

type A3 = new <T>(x: T) => void;
type B3 = new <T>(x: T) => T;
declare let a3: A3;
declare let b3: B3;
a3 = b3;
b3 = a3;

type A11 = new <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
type B11 = new <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
declare let a11: A11;
declare let b11: B11;
a11 = b11;
b11 = a11;

type A16 = new <T extends Base>(x: { a: T; b: T }) => T[];
type B16 = new <U, V>(x: { a: U; b: V }) => U[];
declare let a16: A16;
declare let b16: B16;
a16 = b16;
b16 = a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse generic construct-signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn generic_interface_member_signature_assignments_report_ts2322s() {
    let source = r#"
type Base = { foo: string };

interface A {
    a3: <T>(x: T) => void;
    a11: <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
    a16: <T extends Base>(x: { a: T; b: T }) => T[];
}

declare let x: A;

declare let b3: <T>(x: T) => T;
x.a3 = b3;
b3 = x.a3;

declare let b11: <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
x.a11 = b11;
b11 = x.a11;

declare let b16: <T>(x: { a: T; b: T }) => T[];
x.a16 = b16;
b16 = x.a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse member-signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn generic_interface_member_construct_signature_assignments_report_ts2322s() {
    let source = r#"
type Base = { foo: string };

interface A {
    a3: new <T>(x: T) => void;
    a11: new <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
    a16: new <T extends Base>(x: { a: T; b: T }) => T[];
}

declare let x: A;

declare let b3: new <T>(x: T) => T;
x.a3 = b3;
b3 = x.a3;

declare let b11: new <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
x.a11 = b11;
b11 = x.a11;

declare let b16: new <T>(x: { a: T; b: T }) => T[];
x.a16 = b16;
b16 = x.a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse member construct-signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn callable_source_missing_call_signature_member_reports_only_ts2322() {
    let source = r#"
interface T {
    f(x: number): void;
}

let t: T;
t = () => 1;
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);

    assert!(
        has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected TS2322 for function-to-object assignment. Diagnostics: {diagnostics:#?}"
    );
    assert_no_missing_property_diagnostics(&diagnostics);
}

#[test]
fn callable_source_missing_construct_signature_member_reports_only_ts2322() {
    let source = r#"
interface T {
    f: new (x: number) => void;
}

let t: T;
t = () => 1;
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);

    assert!(
        has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected TS2322 for function-to-constructor-member assignment. Diagnostics: {diagnostics:#?}"
    );
    assert_no_missing_property_diagnostics(&diagnostics);
}

#[test]
fn callable_argument_missing_call_signature_member_has_no_missing_property_related() {
    let source = r#"
interface T {
    f(x: number): void;
}

declare function takesT(t: T): void;
takesT(() => 1);
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code
                == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE),
        "Expected TS2345 for function argument to object parameter. Diagnostics: {diagnostics:#?}"
    );
    assert_no_missing_property_diagnostics(&diagnostics);
}

#[test]
fn callable_argument_missing_construct_signature_member_has_no_missing_property_related() {
    let source = r#"
interface T {
    f: new (x: number) => void;
}

declare function takesT(t: T): void;
takesT(() => 1);
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code
                == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE),
        "Expected TS2345 for function argument to constructor-member object parameter. Diagnostics: {diagnostics:#?}"
    );
    assert_no_missing_property_diagnostics(&diagnostics);
}

#[test]
fn mapped_source_generic_call_reports_ts2345() {
    let source = r#"
type A = "number" | "null" | A[];

type F<T> = null extends T
    ? [F<NonNullable<T>>, "null"]
    : T extends number
    ? "number"
    : never;

type G<T> = { [k in keyof T]: F<T[k]> };

interface K {
    b: number | null;
}

const gK: { [key in keyof K]: A } = { b: ["number", "null"] };

function foo<T>(g: G<T>): T {
    return {} as any;
}

foo(gK);
"#;

    assert!(
        has_error_with_code(
            source,
            diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        ),
        "mapped source generic call should preserve concrete keys and report TS2345"
    );
}

#[test]
fn generic_function_identifier_argument_still_contextually_instantiates() {
    let source = r#"
declare function takesString(fn: (x: string) => string): void;
declare function id<T>(x: T): T;
takesString(id);
"#;

    let diagnostics = get_all_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        !relevant.iter().any(|(code, _)| {
            *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        }),
        "generic function identifiers should still use call-argument contextual instantiation, got: {relevant:?}"
    );
}

#[test]
fn template_literal_target_preserves_string_literal_source_display() {
    let source = r#"
type Foo1<T> = T extends `*${infer U}*` ? U : never;
type T02 = Foo1<'*hello*'>;

let x: `*${string}*`;
x = 'hello';
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322 = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    )
    .into_iter()
    .next()
    .expect("expected TS2322");

    assert!(
        ts2322
            .message_text
            .contains("Type '\"hello\"' is not assignable"),
        "expected literal source display, got {ts2322:?}"
    );
    assert!(
        !ts2322.message_text.contains("Foo1<"),
        "literal source display should not leak conditional alias provenance: {ts2322:?}"
    );
}

#[test]
fn template_literal_number_union_expands_to_string_literals() {
    let source = r#"
        type Digit = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;
        type DigitString = `${Digit}`;
        declare const digit: DigitString;
        const check: "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" = digit;

        type Prefixed = `prefix-${0 | 1}-suffix`;
        declare const prefixed: Prefixed;
        const prefixedCheck: "prefix-0-suffix" | "prefix-1-suffix" = prefixed;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        ts2322.is_empty(),
        "numeric template literal unions should expand to string literal unions, got: {ts2322:#?}"
    );
}

#[test]
fn test_ts2322_generator_yield_missing_value() {
    let source = r"
        interface IterableIterator<T> {}

        function* g(): IterableIterator<number> {
            yield;
            yield 1;
        }
    ";

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_generator_yield_wrong_type() {
    let source = r#"
        interface IterableIterator<T> {}

        function* g(): IterableIterator<number> {
            yield "x";
            yield 1;
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// =============================================================================
// Variable Declaration Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_variable_declaration_wrong_type() {
    let source = r#"
        let x: number = "string";
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_variable_declaration_wrong_object_property() {
    let source = r#"
        let y: { a: number } = { a: "string" };
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_variable_declaration_wrong_array_element() {
    let source = r"
        let z: string[] = [1, 2, 3];
    ";

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn mapped_numeric_handler_context_does_not_falsely_drop_to_implicit_any() {
    let source = r#"
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
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        !relevant
            .iter()
            .any(|(code, _)| { *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE }),
        "mapped handler context should not be misclassified as a primitive-union overload case, got: {relevant:?}"
    );
}

#[test]
fn mapped_type_generic_indexed_access_no_ts2349() {
    // Repro from TypeScript#49338: element access with a generic key on a mapped
    // type should produce a callable result via solver template substitution,
    // not TS2349 "This expression is not callable".
    let source = r#"
type TypesMap = {
    [0]: { foo: 'bar' };
    [1]: { a: 'b' };
};

type P<T extends keyof TypesMap> = { t: T } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

declare const typeHandlers: TypeHandlers;
const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let diagnostics = compile_with_options(
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
        !has_diagnostic_code(&diagnostics, 2349),
        "generic indexed access into mapped type should be callable, got: {diagnostics:?}"
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2344),
        "generic indexed access into mapped type should preserve the `keyof TypesMap` constraint, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "mapped type object literal handlers should contextually type callback params, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_application_generic_indexed_call_preserves_key_correlation() {
    // Structural rule: indexing a homomorphic mapped alias application with a
    // generic key preserves the key in the callable template. The return type is
    // Model[Key], not the union Model[keyof Model].
    let source = r#"
type Readers<T> = { [K in keyof T]: (value: T[K]) => T[K] };

type Model = {
    alpha: { tag: "alpha"; value: number };
    beta: { tag: "beta"; value: string };
};

declare const model: Model;
declare const readers: Readers<Model>;

function read<Key extends keyof Model>(key: Key): Model[Key] {
    return readers[key](model[key]);
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

    assert!(
        !has_diagnostic_code(&diagnostics, 2322),
        "homomorphic mapped alias application indexed with a generic key should keep return correlation, got: {diagnostics:?}"
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2345),
        "homomorphic mapped alias application indexed with a generic key should keep argument correlation, got: {diagnostics:?}"
    );
}

#[test]
fn renamed_mapped_application_generic_indexed_call_preserves_key_correlation() {
    // Same rule with different type parameter and mapped variable names to guard
    // against spelling-based fixes.
    let source = r#"
type Accessors<Input> = { [Slot in keyof Input]: (item: Input[Slot]) => Input[Slot] };

type Store = {
    left: { side: "left"; count: number };
    right: { side: "right"; label: string };
};

declare const store: Store;
declare const accessors: Accessors<Store>;

function get<X extends keyof Store>(slot: X): Store[X] {
    return accessors[slot](store[slot]);
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

    assert!(
        !has_diagnostic_code(&diagnostics, 2322),
        "renamed homomorphic mapped alias application should keep return correlation, got: {diagnostics:?}"
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2345),
        "renamed homomorphic mapped alias application should keep argument correlation, got: {diagnostics:?}"
    );
}

#[test]
fn concrete_union_callable_still_rejects_uncorrelated_union_argument() {
    let source = r#"
declare const fnUnion:
    ((value: { tag: "alpha"; value: number }) => { tag: "alpha"; value: number })
    | ((value: { tag: "beta"; value: string }) => { tag: "beta"; value: string });
declare const value:
    { tag: "alpha"; value: number }
    | { tag: "beta"; value: string };

fnUnion(value);
"#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_diagnostic_code(&diagnostics, 2345),
        "uncorrelated concrete union calls should still be rejected, got: {diagnostics:?}"
    );
}

