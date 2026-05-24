//! Diagnostic-focused spread/rest tests split from `spread_rest_tests`.

use tsz_checker::test_utils::{
    check_source_diagnostics, check_source_with_libs,
    diagnostic_code_message_refs as diagnostic_code_messages, diagnostic_codes, diagnostic_count,
    diagnostic_messages_with_code, diagnostics_with_code, load_lib_files, strict_checker_options,
};

/// A variadic spread (array) after a variable-length tuple spread still emits TS1265.
#[test]
fn test_ts1265_still_fires_for_array_after_variable_length_tuple_spread() {
    let source = r#"
type T = [...[string, ...number[]], ...boolean[]];
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts1265_count = diagnostic_count(&diagnostics, 1265);
    assert_eq!(
        ts1265_count, 1,
        "TS1265 must fire when a variadic spread follows a variable-length tuple spread, got {ts1265_count}: {diagnostics:?}"
    );
}

/// TS2698 must NOT be emitted for rest elements in destructuring patterns.
/// `{ ...x }` on the LHS of `=` or in a for-of initializer is a rest
/// assignment target, not a value spread.
/// Regression test for conformance test `nestedObjectRest.ts`.
#[test]
fn test_no_ts2698_for_destructuring_rest_in_assignment() {
    let source = r#"
var x: any;
[{ ...x }] = [{ abc: 1 }];
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2698 = diagnostic_count(&diagnostics, 2698);
    assert_eq!(
        ts2698,
        0,
        "TS2698 should not be emitted for rest in destructuring assignment, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Same as above but for for-of loop destructuring patterns.
#[test]
fn test_no_ts2698_for_destructuring_rest_in_for_of() {
    let source = r#"
var y: any;
for ([{ ...y }] of [[{ abc: 1 }]]) ;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2698 = diagnostic_count(&diagnostics, 2698);
    assert_eq!(
        ts2698,
        0,
        "TS2698 should not be emitted for rest in for-of destructuring, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Regression: implicit-any variables (var x;) in destructuring rest must
/// also not trigger TS2698. Matches the original nestedObjectRest.ts test.
#[test]
fn test_object_rest_in_destructuring_implicit_any_no_ts2698() {
    let source = r#"
var x, y;
[{ ...x }] = [{ abc: 1 }];
for ([{ ...y }] of [[{ abc: 1 }]]) ;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2698_count = diagnostic_count(&diagnostics, 2698);
    assert_eq!(
        ts2698_count,
        0,
        "Expected no TS2698 for object rest in destructuring with implicit any, got {ts2698_count}. \
         Errors: {:?}",
        diagnostic_messages_with_code(&diagnostics, 2698)
    );
}

#[test]
fn test_ts2783_spread_overwrites_property_in_function_arg() {
    // When an object literal has an explicit property followed by a spread
    // that contains the same required property, TS2783 should fire.
    let source = r#"
interface Opts {
    editor: string;
    char?: string;
}
declare function run(opts: Opts): void;
function test(opts: Opts) {
    run({
        editor: "hello",
        ...opts,
    });
}
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2783_count = diagnostic_count(&diagnostics, 2783);
    assert!(
        ts2783_count >= 1,
        "Expected TS2783 for spread overwriting explicit property in function arg, got {ts2783_count}. Diagnostics: {:?}",
        diagnostic_code_messages(&diagnostics)
    );
}

#[test]
fn test_ts2783_spread_overwrites_in_this_context() {
    // TS2783 should fire when an object literal in a method body
    // has an explicit property that is overwritten by spreading
    // a property from `this`, where the this type provides concrete types.
    let source = r#"
interface Opts {
    editor: string;
    char?: string;
}
declare function create(config: {
    run: (this: { options: Opts }) => void;
}): void;
create({
    run() {
        const result = {
            editor: "hello",
            ...this.options,
        };
    }
});
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2783_count = diagnostic_count(&diagnostics, 2783);
    assert!(
        ts2783_count >= 1,
        "Expected TS2783 for spread from this.options overwriting explicit property, got {ts2783_count}. Diagnostics: {:?}",
        diagnostic_code_messages(&diagnostics)
    );
}

#[test]
fn test_ts2783_spread_overwrites_from_inferred_this_options_member() {
    // Regression for thislessFunctionsNotContextSensitive3.ts: the spread source
    // type is a lazily evaluated member of the contextual `this.options` type.
    let source = r#"
declare class Editor {
  private _editor;
}

declare class Plugin {
  private _plugin;
}

type Partial<T> = {
  [P in keyof T]?: T[P];
};
type Required<T> = {
  [P in keyof T]-?: T[P];
};
type Parameters<T> = T extends (...args: infer P) => any ? P : never;
type ReturnType<T> = T extends (...args: any) => infer R ? R : any;

type ParentConfig<T> = Partial<{
  [P in keyof T]: Required<T>[P] extends (...args: any) => any
    ? (...args: Parameters<Required<T>[P]>) => ReturnType<Required<T>[P]>
    : T[P];
}>;

interface ExtendableConfig<
  Options = any,
  Config extends
    | ExtensionConfig<Options>
    | ExtendableConfig<Options> = ExtendableConfig<Options, any>,
> {
  name: string;
  addOptions?: (this: {
    name: string;
    parent: ParentConfig<Config>["addOptions"];
  }) => Options;
  addProseMirrorPlugins?: (this: {
    options: Options;
    editor: Editor;
  }) => Plugin[];
}

interface ExtensionConfig<Options = any>
  extends ExtendableConfig<Options, ExtensionConfig<Options>> {}

declare class Extension<Options = any> {
  static create<O = any>(config: Partial<ExtensionConfig<O>>): Extension<O>;
}

interface SuggestionOptions {
  editor: Editor;
  char?: string;
}

declare function Suggestion(options: SuggestionOptions): Plugin;

Extension.create({
  name: "slash-command",
  addOptions() {
    return {
      suggestion: {
        char: "/",
      } as SuggestionOptions,
    };
  },
  addProseMirrorPlugins() {
    return [
      Suggestion({
        editor: this.editor,
        ...this.options.suggestion,
      }),
    ];
  },
});

Extension.create({
  name: "slash-command",
  addOptions: () => {
    return {
      suggestion: {
        char: "/",
      } as SuggestionOptions,
    };
  },
  addProseMirrorPlugins() {
    return [
      Suggestion({
        editor: this.editor,
        ...this.options.suggestion,
      }),
    ];
  },
});
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2783_count = diagnostic_count(&diagnostics, 2783);
    assert_eq!(
        ts2783_count,
        2,
        "Expected one TS2783 for each overwritten `editor` property, got {ts2783_count}. Diagnostics: {:?}",
        diagnostic_code_messages(&diagnostics)
    );
}

#[test]
fn test_builtin_partial_preserves_inferred_contextual_this_member() {
    let source = r#"
// @strict: true

interface BoxConfig<Options> {
  addOptions?: () => Options;
  run?: (this: { options: Options }) => void;
}

declare function createBox<O>(config: Partial<BoxConfig<O>>): void;

createBox({
  addOptions() {
    return { nested: { value: 1 } };
  },
  run() {
    this.options.nested.value;
  },
});

export {};
"#;
    let libs = load_lib_files(&["es5.d.ts"]);
    let diagnostics = check_source_with_libs(source, "test.ts", strict_checker_options(), &libs);
    assert_eq!(
        diagnostic_count(&diagnostics, 2339),
        0,
        "Contextual `this.options` should use the options inferred through built-in Partial; diagnostics: {:?}",
        diagnostic_code_messages(&diagnostics)
    );
}

/// A method that references `this` in its body must be treated as context-sensitive.
/// Variant of `test_ts2783_spread_overwrites_from_inferred_this_options_member` with
/// all identifiers renamed: same type machinery, different name spellings.
/// Confirms the rule is structural (not hardcoded to `addOptions`/`addProseMirrorPlugins`).
#[test]
fn test_method_with_this_reference_is_context_sensitive_for_inference() {
    let source = r#"
declare class Edtr { private _e: any; }
declare class Plgn { private _p: any; }

type Partial2<T> = { [P in keyof T]?: T[P]; };
type Required2<T> = { [P in keyof T]-?: T[P]; };
type Parameters2<T> = T extends (...args: infer P) => any ? P : never;
type ReturnType2<T> = T extends (...args: any) => infer R ? R : any;

type ParentConf2<T> = Partial2<{
  [P in keyof T]: Required2<T>[P] extends (...args: any) => any
    ? (...args: Parameters2<Required2<T>[P]>) => ReturnType2<Required2<T>[P]>
    : T[P];
}>;

interface ExtendableConf2<
  Options = any,
  Config extends
    | ExtensionConf2<Options>
    | ExtendableConf2<Options> = ExtendableConf2<Options, any>,
> {
  name: string;
  addSrc?: (this: {
    name: string;
    parent: ParentConf2<Config>["addSrc"];
  }) => Options;
  addPrc?: (this: {
    options: Options;
    device: Edtr;
  }) => Plgn[];
}
interface ExtensionConf2<Options = any>
  extends ExtendableConf2<Options, ExtensionConf2<Options>> {}

declare class Ext2<Options = any> {
  static create<O = any>(config: Partial2<ExtensionConf2<O>>): Ext2<O>;
}

interface DeviceOpts { device: Edtr; slot?: string; }
declare function mkPrc(options: DeviceOpts): Plgn;

Ext2.create({
  name: "my-ext",
  addSrc() {
    return {
      devConfig: { slot: "/" } as DeviceOpts,
    };
  },
  addPrc() {
    return [mkPrc({ device: this.device, ...this.options.devConfig })];
  },
});
"#;
    let diagnostics = check_source_diagnostics(source);
    // `addPrc` references `this`, so it must be context-sensitive (deferred to Round 2).
    // Round 1 infers O = { devConfig: DeviceOpts } from `addSrc()`. Round 2 types `addPrc`
    // with `this.options.devConfig: DeviceOpts`. Spreading DeviceOpts brings in `device`
    // which was already explicitly set → TS2783.
    let ts2783_count = diagnostic_count(&diagnostics, 2783);
    assert!(
        ts2783_count >= 1,
        "Expected TS2783 for method with `this` reference getting proper Round 2 this-type; \
         got {ts2783_count}. Diagnostics: {:?}",
        diagnostic_code_messages(&diagnostics)
    );
}

/// A thisless method (no `this` in body) must NOT be considered context-sensitive,
/// so it participates in Round 1 inference. Only `this`-using methods are deferred.
/// Variant with different names (`compute`/`V`) to confirm the rule is not name-driven.
#[test]
fn test_thisless_method_still_not_context_sensitive_after_this_fix() {
    let source = r#"
// @strict: true
type StateFn<V> = (s: V, ...args: any[]) => any;
type Opts<V> = {
  compute?: V | (() => V) | { (): V };
  actions?: Record<string, StateFn<V>>;
};
declare function register<V extends Record<string, unknown>>(opts: Opts<V>): void;

register({
  compute() { return { value: 42 }; },
  actions: { inc: (myState) => myState.value++ },
});
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts18046_count = diagnostic_count(&diagnostics, 18046);
    assert!(
        ts18046_count == 0,
        "Thisless method `compute()` should NOT be context-sensitive; `myState` must be \
         inferred as {{ value: number }} in Round 1. Got false TS18046 count: {ts18046_count}. \
         Diagnostics: {:?}",
        diagnostic_code_messages(&diagnostics)
    );
}

/// A function expression (non-arrow) that references `this` should be context-sensitive,
/// because `this` is resolved from the contextual type of the surrounding object.
/// Uses the same Extension-like type machinery but with `addPrc: function() {...}` syntax
/// (`PROPERTY_ASSIGNMENT` with `FUNCTION_EXPRESSION` value) to verify the rule applies there too.
#[test]
fn test_function_expression_with_this_is_context_sensitive() {
    let source = r#"
declare class Edtr3 { private _e3: any; }
declare class Plgn3 { private _p3: any; }

type Partial3<T> = { [P in keyof T]?: T[P]; };
type Required3<T> = { [P in keyof T]-?: T[P]; };
type Parameters3<T> = T extends (...args: infer P) => any ? P : never;
type ReturnType3<T> = T extends (...args: any) => infer R ? R : any;

type ParentConf3<T> = Partial3<{
  [P in keyof T]: Required3<T>[P] extends (...args: any) => any
    ? (...args: Parameters3<Required3<T>[P]>) => ReturnType3<Required3<T>[P]>
    : T[P];
}>;

interface ExtendableConf3<
  Options = any,
  Config extends
    | ExtensionConf3<Options>
    | ExtendableConf3<Options> = ExtendableConf3<Options, any>,
> {
  name: string;
  addSrc3?: (this: {
    name: string;
    parent: ParentConf3<Config>["addSrc3"];
  }) => Options;
  addPrc3?: (this: {
    options: Options;
    device: Edtr3;
  }) => Plgn3[];
}
interface ExtensionConf3<Options = any>
  extends ExtendableConf3<Options, ExtensionConf3<Options>> {}

declare class Ext3<Options = any> {
  static create<O = any>(config: Partial3<ExtensionConf3<O>>): Ext3<O>;
}

interface DeviceOpts3 { device: Edtr3; slot?: string; }
declare function mkPrc3(options: DeviceOpts3): Plgn3;

Ext3.create({
  name: "my-ext3",
  addSrc3() {
    return { devConfig: { slot: "/" } as DeviceOpts3 };
  },
  addPrc3: function() {
    return [mkPrc3({ device: this.device, ...this.options.devConfig })];
  },
});
"#;
    let diagnostics = check_source_diagnostics(source);
    // `addPrc3` is a function expression referencing `this`, so it must be context-sensitive.
    // Round 1 infers O = { devConfig: DeviceOpts3 } from `addSrc3()`. Round 2 types `addPrc3`
    // with `this.options.devConfig: DeviceOpts3`. Spreading DeviceOpts3 brings in `device`
    // which was already explicitly set → TS2783.
    let ts2783_count = diagnostic_count(&diagnostics, 2783);
    assert!(
        ts2783_count >= 1,
        "Expected TS2783 for function expression with `this` reference getting Round 2 \
         this-type; got {ts2783_count}. Diagnostics: {:?}",
        diagnostic_code_messages(&diagnostics)
    );
}

/// An arrow function that references `this` should NOT be made context-sensitive by the
/// `this` check: arrows inherit `this` lexically from the enclosing scope, so the
/// `this` type inside an arrow is NOT derived from the outer object's contextual type.
#[test]
fn test_arrow_with_this_still_not_context_sensitive_due_to_this_check() {
    let source = r#"
// @strict: true
type FnOf<X> = (s: X, ...args: any[]) => any;
type WrapOpts<X> = {
  init?: X | (() => X) | { (): X };
  handlers?: Record<string, FnOf<X>>;
};
declare function wrap<X extends Record<string, unknown>>(opts: WrapOpts<X>): void;

wrap({
  init: () => ({ score: 10 }),
  handlers: { add: (st) => st.score++ },
});
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts18046_count = diagnostic_count(&diagnostics, 18046);
    assert!(
        ts18046_count == 0,
        "Arrow function `init: () => ...` should NOT be context-sensitive due to `this` check; \
         `st` must be inferred as {{ score: number }} in Round 1. Got false TS18046 count: {ts18046_count}. \
         Diagnostics: {:?}",
        diagnostic_code_messages(&diagnostics)
    );
}

/// Confirm TS2698 still fires in expression context (spreading a non-object).
#[test]
fn test_object_spread_of_non_object_in_expression_emits_ts2698() {
    let source = r#"
var x: undefined;
var z = { ...x };
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2698_count = diagnostic_count(&diagnostics, 2698);
    assert!(
        ts2698_count >= 1,
        "Expected TS2698 for spreading undefined in expression context, got {ts2698_count}"
    );
}

/// When an array literal contains a spread element whose iterated value
/// type doesn't match the contextual array element type, tsc reports the
/// element-vs-element TS2322 anchored on the spread expression instead of
/// the whole-array TS2322 at the assignment.
///
/// Regression for TypeScript/tests/cases/conformance/es6/spread/iteratorSpreadInArray5.ts:
///   var array: number[] = [0, 1, ...new `SymbolIterator`];
/// Expected message at the spread expression:
///   `Type 'symbol' is not assignable to type 'number'`.
///
/// We use a hand-rolled iterable instead of `new SymbolIterator` so the
/// test does not depend on the lib-loaded `Symbol.iterator` machinery
/// (the test harness intentionally skips lib contexts).
#[test]
fn test_array_spread_iterator_element_mismatch_elaborates_to_spread() {
    let source = r#"
declare let strs: string[];
var array: number[] = [0, 1, ...strs];
"#;
    let diagnostics = check_source_diagnostics(source);

    // Should produce exactly one TS2322 — the elaborated element-level
    // message anchored at the spread expression, not the whole-array
    // fallback.
    let ts2322: Vec<_> = diagnostics_with_code(&diagnostics, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got {}: {:?}",
        ts2322.len(),
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
    let message = &ts2322[0].message_text;
    assert!(
        message.contains("'string'") && message.contains("'number'"),
        "expected per-element 'string' vs 'number' elaboration; got {message:?}"
    );
    assert!(
        !message.contains("(number | string)[]") && !message.contains("(string | number)[]"),
        "expected per-element elaboration, not whole-array message; got {message:?}"
    );
}

/// Custom interfaces extending `Array<T>` keep the whole-assignment TS2322
/// rather than per-element spread elaboration. This pins the negative case
/// for the spread-element elaboration path in
/// `try_elaborate_array_literal_elements`.
#[test]
fn test_array_spread_does_not_elaborate_for_custom_array_subtype() {
    let source = r#"
interface MyNumberArray extends Array<number> {}
declare let strs: string[];
var c: MyNumberArray = [...strs];
"#;
    let diagnostics = check_source_diagnostics(source);

    // We should not see a per-element `'string' is not assignable to 'number'`
    // (which would mean we drilled into the spread). Either no diagnostic
    // (lib not loaded — `MyNumberArray` resolves loosely) or a whole-array
    // TS2322 against MyNumberArray is acceptable.
    let drilled: Vec<_> = diagnostics_with_code(&diagnostics, 2322)
        .into_iter()
        .filter(|d| {
            d.message_text.contains("'string'")
                && d.message_text.contains("'number'")
                && !d.message_text.contains("string[]")
        })
        .collect();
    assert!(
        drilled.is_empty(),
        "expected NOT to drill into per-element spread error for custom \
         array-subtype target; got {:?}",
        drilled.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn test_destructuring_index_signature_only_emits_ts2488_in_es2015() {
    // Pinned by `destructuringArrayBindingPatternAndAssignment2.ts`. tsc emits
    // TS2488 for `var [c4, c5, c6] = foo(1)` when `foo` returns an interface
    // whose only iterable-shaped surface is a numeric index signature. Without
    // an actual `[Symbol.iterator]()` method, ES2015+ destructuring is not
    // permitted: a numeric index signature is not enough on its own.
    let source = r"
interface F {
    [idx: number]: boolean;
}
function foo(idx: number): F {
    return { 2: true };
}
var [c4, c5, c6] = foo(1);
";

    let diagnostics = check_source_diagnostics(source);
    let ts2488_count = diagnostic_count(&diagnostics, 2488);
    assert!(
        ts2488_count >= 1,
        "Expected at least 1 TS2488 for destructuring an interface with only a \
         numeric index signature in ES2015+, got {ts2488_count}: {:?}",
        diagnostic_code_messages(&diagnostics)
    );
}
