/// Regression: property call on intersection type exercises callable shape
/// extraction via boundary queries (not direct intersection member inspection).
#[test]
fn property_call_intersection_callable_boundary() {
    let diags = check_source_diagnostics(
        r#"
interface Logger {
    log(msg: string): void;
}
interface Formatter {
    format(data: number): string;
}

declare const obj: Logger & Formatter;
obj.log("test");
const s: string = obj.format(42);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 || d.code == 2345 || d.code == 2322)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for intersection property calls, got: {:?}",
        diagnostic_messages(&errors)
    );
}

/// Regression: overload resolution where only later signatures match exercises
/// iteration through `get_overload_call_signatures` without internal TypeKey/TypeData.
#[test]
fn overload_later_signature_match() {
    let diags = check_source_diagnostics(
        r#"
declare function choose(x: string): string;
declare function choose(x: number): number;
declare function choose(x: boolean): boolean;

const r: boolean = choose(true);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors when later overload signature matches, got: {:?}",
        diagnostic_messages(&errors)
    );
}

/// Regression: generic call with contextual callback where param type contains
/// intersection with type parameter exercises `contains_type_parameters` and
/// `intersection_members` boundary queries.
#[test]
fn generic_call_callback_intersection_type_param_boundary() {
    let diags = check_source_diagnostics(
        r#"
interface Base {
    id: number;
}

declare function withBase<T extends Base>(init: (item: T & { extra: string }) => void): void;
withBase<Base>((item) => {
    const id: number = item.id;
    const extra: string = item.extra;
});
"#,
    );

    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for callback with intersection type param, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

/// Regression: property call on method returning generic application exercises
/// `evaluate_application_type` and `resolve_lazy_type` boundary paths.
/// The return type is inferred and assigned; property access on the result
/// must resolve through query boundaries without direct TypeData inspection.
#[test]
fn property_call_generic_application_return_type() {
    let diags = check_source_diagnostics(
        r#"
interface Container<T> {
    value: T;
}
interface Factory {
    create<T>(val: T): Container<T>;
}

declare const factory: Factory;
const c = factory.create(42);
const v: number = c.value;
"#,
    );

    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on generic application return, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

/// Regression: overloaded method call with type predicate exercises
/// `extract_predicate_signature` and `is_valid_union_predicate` boundary queries.
#[test]
fn overloaded_method_type_predicate_boundary() {
    let diags = check_source_diagnostics(
        r#"
interface Guard {
    check(x: unknown): x is string;
    check(x: unknown, strict: boolean): x is number;
}

declare const g: Guard;
declare const val: unknown;
if (g.check(val)) {
    const s: string = val;
}
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for overloaded method type predicate, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn block_body_contextual_callback_return_mismatch_reports_ts2345() {
    let diags = check_source_diagnostics(
        r#"
declare function f(g: (x: number) => number[]): void;
f((x) => { return x.toFixed(); });
"#,
    );

    let ts2345 = diagnostics_with_code(&diags, 2345);
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2345.len(),
        1,
        "Expected one outer TS2345 for block-body callback return mismatch, got: {diags:?}"
    );
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no inner TS2322 for block-body callback return mismatch, got: {diags:?}"
    );
}

#[test]
fn expression_body_contextual_callback_return_mismatch_stays_ts2322() {
    let diags = check_source_diagnostics(
        r#"
declare function f(g: (x: number) => number[]): void;
f((x) => x.toFixed());
"#,
    );

    let ts2345 = diagnostics_with_code(&diags, 2345);
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2345.len(),
        0,
        "Expected no outer TS2345 for expression-body callback return mismatch, got: {diags:?}"
    );
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one inner TS2322 for expression-body callback return mismatch, got: {diags:?}"
    );
}

#[test]
fn block_body_callback_with_fewer_parameters_does_not_report_ts2769() {
    let diags = check_source_diagnostics(
        r#"
interface Collection<T, U> {
    length: number;
    add(x: T, y: U): void;
    remove(x: T, y: U): boolean;
}
interface Combinators {
    map<T, U, V>(c: Collection<T, U>, f: (x: T, y: U) => V): Collection<T, V>;
    map<T, U>(c: Collection<T, U>, f: (x: T, y: U) => any): Collection<any, any>;
}
declare const c2: Collection<number, string>;
declare const _: Combinators;
const rf1 = (x: number) => { return x.toFixed(); };
_.map(c2, rf1);
"#,
    );

    let ts2769 = diagnostics_with_code(&diags, 2769);
    assert_eq!(
        ts2769.len(),
        0,
        "Expected no TS2769 for fewer-parameter block-body callback, got: {diags:?}"
    );
}

/// Verify that TS2322 is emitted when indexing an intersection type with an
/// unconstrained type parameter (e.g., `(S & State<T>)["a"]`). Previously,
/// the checker incorrectly deferred the argument mismatch because both
/// actual and expected types contained type parameters.
///
/// Regression test for indexedAccessRelation.ts conformance failure.
#[test]
fn indexed_access_intersection_generic_call_emits_ts2322() {
    let diags = check_source_diagnostics(
        r#"
class Component<S> {
    setState<K extends keyof S>(state: Pick<S, K>) {}
}

export interface State<T> {
    a?: T;
}

class Foo {}

class Comp<T extends Foo, S> extends Component<S & State<T>>
{
    foo(a: T) {
        this.setState({ a: a });
    }
}
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for indexed access on intersection with unconstrained type param, got: {:?}",
        diagnostic_codes(&diags)
    );
}

/// Regression test: generic overloads with `ThisType` markers should not produce
/// false TS2339 on `this` property accesses inside object literal methods.
///
/// The issue was that during overload resolution, the first-pass argument
/// collection uses union-contextual types with unresolved type parameters.
/// The `ThisType`<Data & Readonly<Props> & Instance> marker extracted from the
/// callable had uninstantiated Data/Props, causing `this.bar` to fail. The
/// fix defers the hard-error rejection for generic overloads until after the
/// instantiated retry, which re-evaluates with concrete types.
#[test]
fn vue_like_this_type_inference_no_false_ts2339() {
    let diags = check_source_diagnostics(
        r#"
interface Instance {
    _instanceBrand: never
}

type DataDef<Data, Props> = (this: Readonly<Props> & Instance) => Data

type PropsDefinition<T> = {
    [K in keyof T]: T[K]
}

interface Options<
    Data = ((this: Instance) => object),
    PropsDef = {}
    > {
    data?: Data
    props?: PropsDef
    watch?: Record<string, WatchHandler<any>>
}

type WatchHandler<T> = (val: T, oldVal: T) => void;

type ThisTypedOptions<Data, Props> =
    Options<DataDef<Data, Props>, PropsDefinition<Props>> &
    ThisType<Data & Readonly<Props> & Instance>

declare function test<Data, Props>(fn: ThisTypedOptions<Data, Props>): void;
declare function test(fn: Options): void;

test({
    props: {
        foo: ''
    },

    data(): { bar: boolean } {
        return {
            bar: true
        }
    },

    watch: {
        foo(newVal: string, oldVal: string): void {
            this.bar = false
        }
    }
})
"#,
    );

    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for Vue-like ThisType inference, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

/// Suppress spurious TS2339 for property access on a type parameter whose
/// constraint failed to resolve (e.g., `T extends typeof a` where `a` is a
/// destructured parameter not in scope for type parameter constraints).
///
/// The two-pass type parameter resolution creates an initial unconstrained
/// TypeId and a refined constrained one. Destructured object binding elements
/// can cache the stale unconstrained TypeId, which then triggers a false
/// TS2339 "Property does not exist on type 'T'" even though the constraint
/// error (TS2552) already covers the diagnostic.
#[test]
fn no_false_ts2339_for_destructured_param_with_error_type_param_constraint() {
    let diags = check_source_diagnostics(
        r#"
function f0<T extends typeof a>(a: T) {
    a.b;
}
function f1<T extends typeof a>({a}: {a:T}) {
    a.b;
}
function f2<T extends typeof a>([a]: T[]) {
    a.b;
}
class A {
    m0<T extends typeof a>(a: T) {
        a.b
    }
    m1<T extends typeof a>({a}: {a:T}) {
        a.b
    }
    m2<T extends typeof a>([a]: T[]) {
        a.b
    }
}
"#,
    );

    // tsc emits only TS2552 for each `typeof a` in the type parameter constraint.
    // No TS2339 should be emitted for `a.b` in the body.
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on type param with error constraint, got: {:?}",
        diagnostic_messages(&ts2339)
    );

    // TS2552 should be emitted for each `typeof a` in the constraints.
    let ts2552 = diagnostics_with_code(&diags, 2552);
    assert!(
        ts2552.len() >= 6,
        "Expected at least 6 TS2552 for unresolved typeof in constraints, got {}",
        ts2552.len()
    );
}
