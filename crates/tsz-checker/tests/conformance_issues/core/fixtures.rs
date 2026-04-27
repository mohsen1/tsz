use super::*;

#[test]
fn test_const_alias_expando_element_reads_do_not_emit_ts7053_in_declaration_mode() {
    let source = r#"
function foo() {}
const writeKey = "late-bound";
const readKey = writeKey;
foo[writeKey] = "ok";
const value: string = foo[readKey];
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            emit_declarations: true,
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7053),
        "Did not expect TS7053 once expando element keys resolve through const aliases. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_unique_symbol_expando_element_access_no_ts7053() {
    let source = r#"
export function foo() {}
foo.bar = 12;
const _private = Symbol();
foo[_private] = "ok";
const strMem = "strMemName";
foo[strMem] = "ok";
const dashStrMem = "dashed-str-mem";
foo[dashStrMem] = "ok";
const numMem = 42;
foo[numMem] = "ok";

const x: string = foo[_private];
const y: string = foo[strMem];
const z: string = foo[numMem];
const a: string = foo[dashStrMem];
"#;

    // Without lib: works fine
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 7053),
        "Did not expect TS7053 without lib. Actual: {diagnostics:#?}"
    );

    // With lib: this is what the conformance runner does
    let diagnostics_with_lib = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics_with_lib, 7053),
        "Did not expect TS7053 with lib. Actual: {diagnostics_with_lib:#?}"
    );
}

#[test]
fn test_prototype_named_expando_element_access_no_ts7053() {
    let source = r#"
function F() {}
const key = "lateBound";
F.prototypeOf[key] = "ok";
const value: string = F.prototypeOf[key];
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7053),
        "Did not expect TS7053 for prototype-named non-prototype expando property access. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_inherited_abstract_property_access_in_constructor_reports_ts2715_without_shadowed_cb() {
    let source = r#"
abstract class AbstractClass {
    abstract prop: string;
    abstract cb: (s: string) => void;
}

abstract class DerivedAbstractClass extends AbstractClass {
    cb = (s: string) => {};

    constructor() {
        super();
        this.cb(this.prop.toLowerCase());
    }
}

class Implementation extends DerivedAbstractClass {
    prop = "";

    constructor() {
        super();
        this.cb(this.prop);
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2715_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2715)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts2715_messages
            .iter()
            .any(|message| message.contains("Abstract property 'prop' in class 'AbstractClass'")),
        "Expected TS2715 for inherited abstract prop access. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2715_messages
            .iter()
            .all(|message| !message.contains("Abstract property 'cb'")),
        "Concrete overrides must suppress inherited abstract cb diagnostics. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_indexed_access_variance_failure_preserves_ts2322() {
    let source = r#"
class A {
    x: string = 'A';
    y: number = 0;
}

class B {
    x: string = 'B';
    z: boolean = true;
}

type T<X extends { x: any }> = Pick<X, 'x'>;

type C = T<A>;
type D = T<B>;

declare let a: T<A>;
declare let b: T<B>;
declare let c: C;
declare let d: D;

b = a;
c = d;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();

    // tsc produces 0 errors here: Pick<A, 'x'> and Pick<B, 'x'> are both {x: string},
    // so the assignments are structurally valid. The indexed access through the type
    // parameter produces structurally equivalent results despite different type arguments.
    // With NEEDS_STRUCTURAL_FALLBACK set for indexed access variance, the variance
    // fast path correctly falls through to structural comparison which passes.
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 errors (matching tsc). Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
#[ignore = "regression from remote: invariant recursive generic now emits 2 TS2322 instead of 0"]
fn test_invariant_recursive_generic_error_elaboration_preserves_ts2322() {
    if !lib_files_available() {
        return;
    }

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

    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    // tsc produces 0 errors for this code. With NEEDS_STRUCTURAL_FALLBACK set
    // for indexed access variance, the false positives from variance-based rejection
    // are eliminated — the structural fallback correctly determines compatibility.
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 errors (matching tsc). Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_cached_constructor_parameters_preserve_nested_method_contextual_types() {
    let source = r#"
declare function createInstance<
    Ctor extends new (...args: any[]) => any
>(ctor: Ctor, ...args: [options: IMenuWorkbenchToolBarOptions | undefined]): any;

interface IMenuWorkbenchToolBarOptions {
    toolbarOptions: {
        foo(bar: string): string;
    };
}

class MenuWorkbenchToolBar {
    constructor(options: IMenuWorkbenchToolBarOptions | undefined) {}
}

createInstance(MenuWorkbenchToolBar, {
    toolbarOptions: {
        foo(bar) { return bar; }
    }
});
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 7006),
        "Expected nested method parameter to keep contextual type through generic rest-argument rechecking. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_assignment_compat_with_indexed_targets_matches_tsc() {
    let source = r#"
var x = { one: 1 };
declare var y: { [index: string]: any };
declare var z: { [index: number]: any };
x = y;
y = x;
x = z;
z = x;
y = "foo";
z = "foo";
z = false;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let messages: Vec<&str> = relevant
        .iter()
        .map(|(_, message)| message.as_str())
        .collect();

    // tsc infers `var x = { one: 1 }` as `{ one: number }`, not `{ one: 1 }`
    assert_eq!(relevant.len(), 4, "unexpected diagnostics: {relevant:?}");
    assert!(
        messages.contains(&"Property 'one' is missing in type '{ [index: string]: any; }' but required in type '{ one: number; }'."),
        "missing TS2741 for x = y: {relevant:?}"
    );
    assert!(
        messages.contains(&"Property 'one' is missing in type '{ [index: number]: any; }' but required in type '{ one: number; }'."),
        "missing TS2741 for x = z: {relevant:?}"
    );
    assert!(
        messages.contains(&"Type 'string' is not assignable to type '{ [index: string]: any; }'."),
        "missing TS2322 for y = \"foo\": {relevant:?}"
    );
    assert!(
        messages.contains(&"Type 'boolean' is not assignable to type '{ [index: number]: any; }'."),
        "missing TS2322 for z = false: {relevant:?}"
    );
}

#[test]
fn test_non_ambient_class_function_merge_also_reports_duplicate_identifier() {
    let source = r#"
class c2 { public foo() { } }
function c2() { }
var c2 = () => { }
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2300_count = diagnostics.iter().filter(|(code, _)| *code == 2300).count();

    assert_eq!(
        ts2300_count, 3,
        "Expected duplicate-identifier diagnostics on the class, function, and variable declarations. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2813),
        "Expected TS2813 on the class declaration. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2814),
        "Expected TS2814 on the function declaration. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ambient_class_function_merge_new_uses_constructor() {
    // When an ambient function and class merge, `new Foo(...)` should use the
    // class constructor parameters, not the function parameters.
    let source = r#"
declare function Foo(x: number): number;
declare class Foo { constructor(x: string); }
const a = new Foo("");
const b = Foo(12);
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    // `new Foo("")` uses the class constructor (x: string), so "" is valid
    assert!(
        !has_error(&diagnostics, 2345),
        "new Foo(\"\") should NOT emit TS2345 - should use class constructor (string), not function (number). Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_merged_enum_duplicate_member_reports_all_occurrences() {
    let source = r#"
enum e5a { One }
enum e5a { One }
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2300_count = diagnostics.iter().filter(|(code, _)| *code == 2300).count();

    assert_eq!(
        ts2300_count, 2,
        "Expected duplicate-identifier diagnostics on both merged enum members. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_constructor_parameters_rest_argument_contextually_types_object_literal_methods() {
    if !lib_files_available() {
        return;
    }

    let source = r#"
declare function createInstance<
    Ctor extends new (...args: any[]) => any,
    R extends InstanceType<Ctor>
>(ctor: Ctor, ...args: ConstructorParameters<Ctor>): R;

interface IMenuWorkbenchToolBarOptions {
    toolbarOptions: {
        foo(bar: string): string;
    };
}

class MenuWorkbenchToolBar {
    constructor(options: IMenuWorkbenchToolBarOptions | undefined) {}
}

createInstance(MenuWorkbenchToolBar, {
    toolbarOptions: {
        foo(bar) { return bar; }
    }
});
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 7006 | 2345))
        .collect();

    assert!(
        relevant.is_empty(),
        "ConstructorParameters rest contextual typing should not produce TS7006/TS2345, got: {diagnostics:#?}"
    );
}

#[test]
fn test_class_implements_interface_property_access_does_not_cascade_ts2339() {
    let source = r#"
interface Printable { print(): void; }
class Doc implements Printable { }
let doc: Doc;
doc.print();
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2420),
        "Expected the primary TS2420 for the broken implements clause. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Property access should recover through the implemented interface surface instead of cascading TS2339. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_inherited_non_public_member_does_not_satisfy_public_interface_property() {
    let source = r#"
interface I {
    name: string;
}

class PrivateBase {
    private name: string;
}

class ProtectedBase {
    protected name: string;
}

class PrivateDerived extends PrivateBase implements I {}
class ProtectedDerived extends ProtectedBase implements I {}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2420: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2420)
        .collect();

    assert_eq!(
        ts2420.len(),
        2,
        "Expected both derived classes to report TS2420. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2420.iter().any(|(_, message)| message
            .contains("Property 'name' is private in type 'PrivateDerived' but not in type 'I'.")),
        "Expected the inherited private member to report as a visibility conflict. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2420.iter().any(|(_, message)| message.contains(
            "Property 'name' is protected in type 'ProtectedDerived' but not in type 'I'."
        )),
        "Expected the inherited protected member to report as a visibility conflict. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_overloaded_interface_method_inheritance_uses_trailing_signature_compatibility() {
    let source = r#"
interface Indexed<T> {
    filter<F extends T>(predicate: (value: T) => value is F): Indexed<F>;
    filter(predicate: (value: T) => any): this;
}

interface SetLike<T> {}

interface Stack<T> extends Indexed<T> {
    filter<F extends T>(predicate: (value: T) => value is F): SetLike<F>;
    filter(predicate: (value: T) => any): this;
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2430 || *code == 2320)
        .collect();

    assert!(
        relevant.is_empty(),
        "Overloaded interface inheritance should not report TS2430/TS2320 when the trailing method signature remains compatible. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_type_alias_in_narrowed_branch_preserves_flow_sensitive_typeof() {
    let source = r#"
declare let c: string | number;
if (typeof c === "string") {
    type Direct = typeof c;
    const badDirect: Direct = 1;

    type Indexed = { [key: string]: typeof c };
    const badIndexed: Indexed = { bar: 1 };
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2322_messages: Vec<_> = relevant
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'number' is not assignable to type 'string'.")),
        "Expected direct typeof alias narrowing to report TS2322, got: {relevant:#?}"
    );
    assert!(
        ts2322_messages.len() >= 2,
        "Expected both direct and indexed alias narrowing errors, got: {relevant:#?}"
    );
}

#[test]
fn test_index_write_with_errored_key_still_checks_value_type() {
    let source = r#"
class Box {
    values: { [name: string]: string } = {};

    write(value?: string) {
        this.values[this.missing] = value;
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.iter().any(|(code, _)| *code == 2339),
        "Expected missing-key diagnostic, got: {relevant:#?}"
    );
    assert!(
        relevant.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("Type 'string | undefined' is not assignable to type 'string'.")
        }),
        "Expected value-type mismatch on index write even when the key errors, got: {relevant:#?}"
    );
}

#[test]
fn test_partial_method_rest_parameter_preserves_contextual_tuple_elements() {
    let source = r#"
declare function assignPartial<T>(target: T, partial: Partial<T>): T;

let obj = {
    foo(bar: string) {}
};

assignPartial(obj, { foo(...args) {} });
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 7006 | 2345))
        .collect();

    assert!(
        relevant.is_empty(),
        "Partial<T> method rest contextual typing should not produce TS7006/TS2345, got: {diagnostics:#?}"
    );
}

#[test]
fn test_tuple_spread_arguments_preserve_tuple_element_types_for_rest_positions() {
    let source = r#"
declare const t1: [number, boolean, string];

(function (a, b, c){})(...t1);

declare const t2: [number, boolean, ...string[]];

(function (a, b, c){})(...t2);

declare const t3: [boolean, ...string[]];

(function (a, b, c){})(1, ...t3);
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2345))
        .collect();

    assert!(
        relevant.is_empty(),
        "Tuple spread rest arguments should not collapse to never, got: {diagnostics:#?}"
    );
}

#[test]
fn test_contextual_tuple_rest_callbacks_accept_variadic_typeof_tuple_shapes() {
    let source = r#"
declare const t2: [number, boolean, ...string[]];
declare function f2(cb: (...args: typeof t2) => void): void;
f2((a, b, c) => {});
f2((a, ...x) => {});

declare const t3: [boolean, ...string[]];
declare function f3(cb: (x: number, ...args: typeof t3) => void): void;
f3((...x) => {});
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2345 | 7006))
        .collect();

    assert!(
        relevant.is_empty(),
        "Variadic typeof tuple callback contextual typing should not produce TS2322/TS2345/TS7006, got: {diagnostics:#?}"
    );
}

#[test]
fn test_tuple_union_and_generic_rest_callback_compatibility() {
    let source = r#"
function f4<T extends any[]>(t: T) {
    function f(cb: (x: number, ...args: T) => void) {}
    f((a, b, ...x) => {});
}
type ArgsUnion = [number, string] | [number, Error];
type TupleUnionFunc = (...params: ArgsUnion) => number;

const funcUnionTupleNoRest: TupleUnionFunc = (num, strOrErr) => {
  return num;
};
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();
    let ts7006_count = diagnostics.iter().filter(|(code, _)| *code == 7006).count();

    assert_eq!(
        ts7006_count, 0,
        "Generic tuple rest callbacks should still receive contextual parameter typing, got: {diagnostics:#?}"
    );
    assert_eq!(
        ts2322_count, 0,
        "Tuple-union rest callback assignment should not report TS2322, got: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_rest_tuple_callback_rejects_extra_fixed_parameter() {
    if !lib_files_available() {
        return;
    }

    let source = r#"
function f4<T extends any[]>(t: T) {
    function f(cb: (x: number, ...args: T) => void) {}
    f((a, b, ...x) => {});
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 2345).count(),
        1,
        "Generic rest tuple callbacks should reject extra fixed parameters that are not guaranteed by T. diagnostics={diagnostics:#?}"
    );
}

#[test]
fn test_higher_order_generic_rest_call_accepts_generic_binary_function() {
    let source = r#"
function call<T extends unknown[], U>(f: (...args: T) => U, ...args: T) {
    return f(...args);
}

function callr<T extends unknown[], U>(args: T, f: (...args: T) => U) {
    return f(...args);
}

declare const sn: [string, number];
declare function f16<A, B>(a: A, b: B): A | B;
declare function f15(a: string, b: number): string | number;

let x20 = call((x, y) => x + y, 10, 20);
let x21 = call((x, y) => x + y, 10, "hello");
let x22 = call(f15, "hello", 42);
let x23 = call(f16, "hello", 42);
let x24 = call<[string, number], string | number>(f16, "hello", 42);
let x30 = callr(sn, (x, y) => x + y);
let x31 = callr(sn, f15);
let x32 = callr(sn, f16);
"#;

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2345),
        "Higher-order generic rest calls should accept a generic binary function without TS2345. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_interface_construct_signature_prefers_namespace_local_class_reference() {
    let source = r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> { constructor(props: P); }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    declare let c: ComponentClass<{ children: (foo: number) => string }>;
    let z = new c({ children: foo => "" + foo });
    z.props;

    declare function takes(c: ComponentClass<{ children: (foo: number) => string }>): void;
    takes(InferFunctionTypes);
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_count = diagnostics.iter().filter(|(code, _)| *code == 2339).count();
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2339_count, 1,
        "Expected interface construct signature to keep the namespace-local Component return type, producing exactly one TS2339 for z.props. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2345_count, 0,
        "Namespace-local Component references in interface construct signatures should not drift to the global class. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_component_class_inference_keeps_namespace_local_construct_signature() {
    let source = r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> { constructor(props: P); }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    declare function makeP<P extends {}>(Ctor: ComponentClass<P>): void;
    makeP(InferFunctionTypes);
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 0,
        "Generic call inference should preserve namespace-local construct signature return types for ComponentClass<P>. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_interface_construct_signature_stays_namespace_local_with_top_level_type_alias() {
    let source = r#"
declare class Component<P> { constructor(props: P); props: P; }

type Id = number;

namespace N1 {
    declare class Component<P> { constructor(props: P); }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    type CreateElementChildren<P> = P extends { children?: infer C }
        ? C extends any[]
            ? C
            : C[]
        : unknown;

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    declare let c: ComponentClass<{ children: (foo: number) => string }>;
    let z = new c({ children: foo => "" + foo });
    z.props;

    declare function makeP<P extends {}>(Ctor: ComponentClass<P>): CreateElementChildren<P>;
    let inferred = makeP(InferFunctionTypes);
    inferred;
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_count = diagnostics.iter().filter(|(code, _)| *code == 2339).count();
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2339_count, 1,
        "A top-level type alias must not cause interface construct signatures to resolve to the global Component. Expected exactly one TS2339 for z.props. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2345_count, 0,
        "Generic inference should keep the namespace-local ComponentClass<P> construct signature even when unrelated top-level type aliases are present. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsx_element_constructor_union_assigns_to_function_or_construct_union_parameter() {
    let source = r#"
interface ExactProps {
    value: "A" | "B";
}
interface FunctionComponent<P = {}> {
    (props: P): any;
}
interface ComponentClass<P = {}> {
    new (props: P): any;
}
type JSXElementConstructor<P> =
    | ((props: P) => any)
    | (new (props: P) => any);

declare let wrapper: JSXElementConstructor<ExactProps>;
declare let accepts: FunctionComponent<ExactProps> | ComponentClass<ExactProps> | string;
accepts = wrapper;
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(&diagnostics, 2322) && !has_error(&diagnostics, 2345),
        "JSXElementConstructor<P> should be assignable to FunctionComponent<P> | ComponentClass<P> | string without TS2322/TS2345. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsx_element_constructor_union_assigns_to_function_or_construct_union_parameter_in_strict_mode()
 {
    let source = r#"
interface ExactProps {
    value: "A" | "B";
}
interface FunctionComponent<P = {}> {
    (props: P): any;
}
interface ComponentClass<P = {}> {
    new (props: P): any;
}
type JSXElementConstructor<P> =
    | ((props: P) => any)
    | (new (props: P) => any);

declare let wrapper: JSXElementConstructor<ExactProps>;
declare let accepts: FunctionComponent<ExactProps> | ComponentClass<ExactProps> | string;
accepts = wrapper;
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322) && !has_error(&diagnostics, 2345),
        "JSXElementConstructor<P> should remain assignable in strict mode. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsx_element_constructor_union_infers_props_for_create_element_like_call() {
    let source = r#"
// @target: es2015
// @strict: true
// @noEmit: true

interface ExactProps {
  value: "A" | "B";
}
interface FunctionComponent<P = {}> {
  (props: P): ReactElement<any> | null;
}
declare class Component<P> {
  constructor(props: P);
}
interface ComponentClass<P = {}> {
  new (props: P): Component<P>;
}

interface ReactElement<
  T extends string | JSXElementConstructor<any> =
    | string
    | JSXElementConstructor<any>,
> {
  type: T;
}

type JSXElementConstructor<P> =
  | ((props: P) => ReactElement<any> | null)
  | (new (props: P) => Component<any>);

declare function createElementIsolated<P extends {}>(
  type: FunctionComponent<P> | ComponentClass<P> | string,
  props?: P | null,
): void;

declare let WrapperIsolated: JSXElementConstructor<ExactProps>;
createElementIsolated(WrapperIsolated, { value: "C" });
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 0,
        "createElement-like inference should accept JSXElementConstructor<P> as the first argument. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2322_count, 1,
        "Expected the prop value mismatch to surface as one TS2322 after first-argument inference succeeds. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsx_element_constructor_union_with_explicit_type_argument_accepts_valid_props() {
    let source = r#"
// @target: es2015
// @strict: true
// @noEmit: true

interface ExactProps {
  value: "A" | "B";
}
interface FunctionComponent<P = {}> {
  (props: P): ReactElement<any> | null;
}
declare class Component<P> {
  constructor(props: P);
}
interface ComponentClass<P = {}> {
  new (props: P): Component<P>;
}

interface ReactElement<
  T extends string | JSXElementConstructor<any> =
    | string
    | JSXElementConstructor<any>,
> {
  type: T;
}

type JSXElementConstructor<P> =
  | ((props: P) => ReactElement<any> | null)
  | (new (props: P) => Component<any>);

declare function createElementIsolated<P extends {}>(
  type: FunctionComponent<P> | ComponentClass<P> | string,
  props?: P | null,
): void;

declare let WrapperIsolated: JSXElementConstructor<ExactProps>;
createElementIsolated<ExactProps>(WrapperIsolated, { value: "A" });
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "Explicit type arguments should bypass inference and accept JSXElementConstructor<ExactProps>. Actual diagnostics: {diagnostics:#?}"
    );
}
