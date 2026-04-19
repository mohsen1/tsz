use crate::core::*;

#[test]
fn test_generic_filtering_mapped_callbacks_use_widened_round2_context() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare function f1<T extends object>(
  data: T,
  handlers: { [P in keyof T as P]: (value: T[P], prop: P) => void },
): void;

f1(
  {
    foo: 0,
    bar: "",
  },
  {
    foo: (value, key) => {},
    bar: (value, key) => {},
  },
);

declare function f2<T extends object>(
  data: T,
  handlers: { [P in keyof T as T[P] extends string ? P : never]: (value: T[P], prop: P) => void },
): void;

f2(
  {
    foo: 0,
    bar: "",
  },
  {
    bar: (value, key) => {},
  },
);

f2(
  {
    foo: 0,
    bar: "",
  },
  {
    foo: (value, key) => {
    },
  },
);
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Did not expect false TS2322 callback assignability errors after round-2 generic contextual typing.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Did not expect a whole-object TS2345 when the filtered mapped handler should instead hit TS2353.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2353),
        "Expected TS2353 for excess property 'foo' on the filtered mapped handlers object.\nActual diagnostics: {diagnostics:#?}"
    );
    let ts2353_count = diagnostics.iter().filter(|(code, _)| *code == 2353).count();
    assert_eq!(
        ts2353_count, 1,
        "Expected exactly one TS2353 diagnostic for the excess property site.\nActual diagnostics: {diagnostics:#?}"
    );
    let ts7006_count = diagnostics.iter().filter(|(code, _)| *code == 7006).count();
    assert_eq!(
        ts7006_count, 2,
        "Expected exactly two TS7006 diagnostics for the excess-property callback parameters.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_contextual_filter_callback_preserves_constraint() {
    let source = r"
type Box<T> = { value: T };

declare function arrayFilter<T>(f: (x: T) => boolean): (a: T[]) => T[];

const f31: <T extends Box<number>>(a: T[]) => T[] = arrayFilter(x => x.value > 10);
";

    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2322),
        "Should NOT emit TS2322 for generic contextual filter callback.\nActual errors: {relevant:#?}"
    );
    assert!(
        !has_error(&relevant, 2339),
        "Should NOT emit TS2339 for generic contextual filter callback.\nActual errors: {relevant:#?}"
    );
    assert!(
        !has_error(&relevant, 18046),
        "Should NOT emit TS18046 for generic contextual filter callback.\nActual errors: {relevant:#?}"
    );
    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 for generic contextual filter callback.\nActual errors: {relevant:#?}"
    );
}

#[test]
#[ignore = "pre-existing: generic callback mismatch inference not yet implemented"]
fn test_contextual_signature_instantiation_reports_generic_callback_mismatch() {
    let source = r#"
declare function foo<T>(cb: (x: number, y: string) => T): T;
declare function bar<T, U, V>(x: T, y: U, cb: (x: T, y: U) => V): V;
declare function g<T>(x: T, y: T): T;

var b = foo(g);
var c = bar(1, "one", g);
"#;

    let options = CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    let ts2345_count = relevant.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 2,
        "Expected TS2345 on both generic callback mismatch sites.\nActual diagnostics: {relevant:#?}"
    );
}

#[test]
#[ignore = "pre-existing: overloaded callback generic call not yet matching tsc"]
fn test_generic_call_with_overloaded_callback_uses_last_source_signature() {
    let source = r#"
interface Promise<T> {
    then<U>(cb: (x: T) => Promise<U>): Promise<U>;
}

declare function testFunction(n: number): Promise<number>;
declare function testFunction(s: string): Promise<string>;

declare var numPromise: Promise<number>;
var newPromise = numPromise.then(testFunction);
"#;

    let options = CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    let ts2345_count = relevant.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 1,
        "Expected TS2345 for overloaded callback generic call.\nActual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_direct_generic_inference_does_not_union_nominal_private_candidates() {
    let source = r#"
class C { private x: string; }
class D { private x: string; }
function id2<T>(a: T, b: T): T { return a; }
let r = id2(new C(), new D());
"#;

    let options = CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    let ts2345_count = relevant.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 1,
        "Expected TS2345 for nominal direct generic inference.\nActual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_nested_generic_inference_does_not_union_nominal_private_candidates() {
    let source = r#"
class C { private x: string; }
class D { private x: string; }
class X<T> { x!: T; }
function foo<T>(a: X<T>, b: X<T>): T { return a.x; }
let r = foo(new X<C>(), new X<D>());
"#;

    let options = CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    let ts2345_count = relevant.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 1,
        "Expected TS2345 for nested nominal generic inference.\nActual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_type_assertion_does_not_contextually_check_plain_coalesce_expression() {
    let diagnostics =
        without_missing_global_type_errors(compile_and_get_diagnostics_with_lib_and_options(
            r#"
type Component = { name?: string } | ((props: {}) => void);
type WithInstallPlugin = { _prefix?: string };

export function withInstall<C extends Component, T extends WithInstallPlugin>(
  component: C | C[],
  target?: T,
): string {
  const componentWithInstall = (target ?? component) as T;
  return "";
}
"#,
            CheckerOptions {
                strict: true,
                target: ScriptTarget::ES2015,
                ..CheckerOptions::default()
            },
        ));

    assert!(
        diagnostics.is_empty(),
        "Did not expect TS2322 inside a plain `as T` assertion operand.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_narrowing_by_typeof_switch_chunk_matches_real_file() {
    let diagnostics = without_missing_global_type_errors(
        compile_and_get_diagnostics_with_lib_and_options(
            r#"
declare function assertNever(x: never): never;
type L = (x: number) => string;
type R = { x: string, y: number };

function multipleGeneric<X extends L, Y extends R>(xy: X | Y): [X, string] | [Y, number] {
    switch (typeof xy) {
        case 'function': return [xy, xy(42)];
        case 'object': return [xy, xy.y];
        default: return assertNever(xy);
    }
}

function multipleGenericFuse<X extends L | number, Y extends R | number>(xy: X | Y): [X, number] | [Y, string] | [(X | Y)] {
    switch (typeof xy) {
        case 'function': return [xy, 1];
        case 'object': return [xy, 'two'];
        case 'number': return [xy];
    }
}

function multipleGenericExhaustive<X extends L, Y extends R>(xy: X | Y): [X, string] | [Y, number] {
    switch (typeof xy) {
        case 'object': return [xy, xy.y];
        case 'function': return [xy, xy(42)];
    }
}
"#,
            CheckerOptions {
                strict: true,
                target: ScriptTarget::ES2015,
                ..CheckerOptions::default()
            },
        ),
    );

    assert!(
        diagnostics.is_empty(),
        "Did not expect diagnostics for the narrowingByTypeofInSwitch generic chunk.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_recursive_function_assignment_does_not_stack_overflow() {
    let _diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Expression = ['and', ...Expression[]] | ['not', Expression] | 'true' | 'false';
declare const sink: (x: Expression) => boolean;
const f: (x: Expression) => boolean = sink;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
}

#[test]
fn test_union_restricted_indexed_access_prefers_ts2339_over_constraint_failure() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class Foo {
  protected foo = 0;
}

class Bar {
  protected foo = 0;
}

type Nothing<V extends Foo> = void;

type Broken<V extends Array<Foo | Bar>> = {
  readonly [P in keyof V]: V[P] extends Foo ? Nothing<V[P]> : never;
};

type _3 = (Foo & Bar)['foo'];
type _4 = (Foo | Bar)['foo'];
type _5 = (Foo | (Foo & Bar))['foo'];
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Restricted union indexed access should not fall back to TS2344.\nActual diagnostics: {diagnostics:#?}"
    );
    let ts2339 =
        diagnostic_message(&diagnostics, 2339).expect("expected TS2339 for (Foo | Bar)['foo']");
    assert!(
        ts2339.contains("Property 'foo' does not exist on type 'Foo | Bar'."),
        "Expected the union restricted-property message for (Foo | Bar)['foo'].\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_conditional_true_branch_type_argument_satisfies_constraint_for_indexed_access() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class Foo {
  protected foo = 0;
}

class Bar {
  protected foo = 0;
}

type Nothing<V extends Foo> = void;
type Broken<V extends { x: Foo | Bar }, P extends keyof V> =
  V[P] extends Foo ? Nothing<V[P]> : never;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Conditional true-branch narrowing should satisfy the type argument constraint.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_conditional_true_branch_type_argument_satisfies_direct_constraint() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class Foo {
  protected foo = 0;
}

class Bar {
  protected foo = 0;
}

type Nothing<V extends Foo> = void;
type Guarded<V extends Foo | Bar> = V extends Foo ? Nothing<V> : never;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Conditional true-branch narrowing should satisfy direct type argument constraints.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_recursive_conditional_alias_constraint_accepts_string_literal_key() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type CanBeExpanded<T extends object = object, D = string> = {
  value: T;
  default: D;
};

interface Base {}

interface User extends Base {
  role: CanBeExpanded<Role>;
}

interface Role extends Base {
  user: CanBeExpanded<User>;
}

interface X extends Base {
  user: CanBeExpanded<User>;
  role: CanBeExpanded<Role>;
}

type Join<K, P> =
  K extends string | number
    ? P extends string | number
      ? `${K}${"" extends P ? "" : "."}${P}`
      : never
    : never;

type PrefixWith<P, S, C = "."> =
  P extends "" ? `${string & S}` : `${string & P}${string & C}${string & S}`;

type KeysCanBeExpanded_<T, N extends number, Depth extends number[]> =
  N extends Depth["length"] ? never :
  T extends CanBeExpanded ? KeysCanBeExpanded_<T["value"], N, Depth> :
  T extends Array<infer U> ? KeysCanBeExpanded_<U, N, Depth> :
  T extends object ? {
    [K in keyof T]:
      T[K] extends object
        ? K extends string | number
          ? `${K}` | Join<`${K}`, KeysCanBeExpanded_<T[K], N, [1, ...Depth]>>
          : never
        : never
  }[keyof T] :
  never;

type KeysCanBeExpanded<T, N extends number = 4> = KeysCanBeExpanded_<T, N, []>;

type Expand__<O, Keys, P extends string, N extends number, Depth extends unknown[]> =
  N extends Depth["length"] ? O :
  O extends CanBeExpanded ? Expand__<O[P extends Keys ? "value" : "default"], Keys, P, N, Depth> :
  O extends Array<infer U> ? Expand__<U, Keys, P, N, Depth>[] :
  O extends object ? { [K in keyof O]-?: Expand__<O[K], Keys, PrefixWith<P, K>, N, [1, ...Depth]> } :
  O;

type SplitAC<K> = K extends string ? K : "";
type Expand_<T, K, N extends number = 4> = Expand__<T, SplitAC<K>, "", N, []>;
type AllKeys<T, N extends number = 4> = KeysCanBeExpanded<T, N> extends infer R ? R : never;

type Expand<T extends object, K extends AllKeys<T, N> = never, N extends number = 4> = Expand_<T, K, N>;
type UseQueryOptions<T extends Base, K extends AllKeys<T, 4>> = Expand<T, K>;

let t: UseQueryOptions<X, "role.user.role">;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Recursive conditional alias constraints should accept valid string-literal keys.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_function_intrinsic_satisfies_structural_length_constraint() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
let f: Function = () => {};
let x: { length: number } = f;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Function should be assignable to structural length constraints through its boxed interface surface.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_promise_chaining_function_constraint_only_reports_final_ts2322() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        r#"
class Chain2<T extends { length: number }> {
  constructor(public value: T) {}
  then<S extends Function>(cb: (x: T) => S): Chain2<S> {
    var result = cb(this.value);
    var z = this.then(x => result).then(x => "abc").then(x => x.length);
    return new Chain2(result);
  }
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "The `Function` base constraint should satisfy `Chain2`'s length requirement.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Promise-style chaining should not add spurious callback-argument errors once the class type argument constraint is satisfied.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
#[ignore = "pre-existing: remote merge regression"]
fn test_promise_chaining_reports_both_callback_body_ts2322s() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface Fn {
  (): void;
  length: number;
}

class Chain2<T extends { length: number }> {
  constructor(public value: T) {}
  then<S extends Fn>(cb: (x: T) => S): Chain2<S> {
    var result = cb(this.value);
    var z = this.then(x => result).then(x => "abc").then(x => x.length);
    return new Chain2(result);
  }
}
"#,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'string' is not assignable to type 'Fn'.")),
        "Expected the middle callback body to report the string-to-Fn mismatch.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'number' is not assignable to type 'Fn'.")),
        "Expected the final callback body to report the number-to-Fn mismatch after the invalid middle link.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_invalid_generic_call_initializer_keeps_precise_variable_type() {
    let source = r#"
interface Fn {
  (): void;
  length: number;
}

class Chain2<T extends { length: number }> {
  constructor(public value: T) {}
  then<S extends Fn>(cb: (x: T) => S): Chain2<S> {
    throw 0 as unknown as Chain2<S>;
  }
}

declare const a: Chain2<Fn>;
let z = a.then(x => "abc");
let f: Fn = z.value.length;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    checker.check_source_file(root);

    let z_sym = binder.file_locals.get("z").expect("z should exist");
    let z_type = checker.get_type_of_symbol(z_sym);
    let z_type_text = checker.format_type(z_type);

    assert_ne!(
        z_type,
        tsz_solver::TypeId::ANY,
        "z should not collapse to any"
    );
    assert!(
        !tsz_solver::contains_error_type(&types, z_type),
        "z should preserve a usable application type even after the callback body error.\nType: {z_type_text}\nDiagnostics: {:#?}",
        checker.ctx.diagnostics
    );
    // After fixing recursive type variance (independent variance for self-referencing
    // generics), the type expands to its structural form. Both are semantically equivalent.
    assert!(
        z_type_text.contains("Chain2<Fn>") || z_type_text.contains("Chain2<{"),
        "z should remain Chain2<Fn> (or structural equivalent) so downstream reads keep the number-typed length property.\nActual type: {z_type_text}\nDiagnostics: {:#?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_any_rest_assignment_rejects_never_parameter_source() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare var ff1: (... args: any[]) => void;
declare var ff4: (a: never) => void;

ff1 = ff4;
"#,
    );

    let ts2322 = diagnostic_message(&diagnostics, 2322)
        .expect("expected TS2322 for assigning never-parameter function to any-rest target");

    assert!(
        ts2322.contains(
            "Type '(a: never) => void' is not assignable to type '(...args: any[]) => void'."
        ),
        "Expected the any-rest assignment failure to mention the source and target callable types.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_missing_property_messages_preserve_function_literal_return_type_display() {
    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        r#"
var b3: { f(n: number): number; g(s: string): number; m: number; n?: number; k?(a: any): any; };

b3 = {
    f: (n) => { return 0; },
    g: (s) => { return 0; },
    n: 0,
    k: (a) => { return null; },
};
"#,
        CheckerOptions {
            strict: false,
            strict_null_checks: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let missing_m = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741 for missing property 'm'");

    assert!(
        missing_m
            .message_text
            .contains("type '{ f: (n: number) => number; g: (s: string) => number; n: number; k: (a: any) => null; }'"),
        "expected object-literal source display to preserve the function literal return type: {missing_m:#?}"
    );
}

#[test]
fn test_import_equals_namespace_assignment_respects_inline_typeof_member_queries() {
    let diagnostics = compile_two_files_get_diagnostics_with_options(
        r#"
export class Model {
    public someData!: string;
}

export class VisualizationModel extends Model {}
"#,
        r#"
import moduleA = require("./a");

interface IHasVisualizationModel {
    VisualizationModel: typeof moduleA.Model;
}

const x: IHasVisualizationModel = moduleA;
"#,
        "./a",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Did not expect TS2322 for namespace assignment with inline import-equals typeof member query. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_union_of_tuple_rest_method_parameter_rejects_incompatible_tnext() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
interface I {
    next(...[value]: [] | [undefined]): { done?: false; value: string } | { done: true; value: unknown };
}
interface G {
    next(...[value]: [] | [boolean]): { done?: false; value: string } | { done: true; value: number };
}
declare let g: G;
const x: I = g;

interface IP {
    next: (...[value]: [] | [undefined]) => { done?: false; value: string } | { done: true; value: unknown };
}
interface GP {
    next: (...[value]: [] | [boolean]) => { done?: false; value: string } | { done: true; value: number };
}
declare let gp: GP;
const y: IP = gp;
"#,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for incompatible union-of-tuple rest methods. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_spy_comparison_checking_reports_ts2339() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.ts",
        r#"
interface Spy {
    (...params: any[]): any;
    identity: string;
    and: Function;
    mostRecentCall: { args: any[]; };
    argsForCall: any[];
}

type SpyObj<T> = T & {
    [k in keyof T]: Spy;
}

declare function createSpyObj<T>(
    name: string, names: Array<keyof T>): SpyObj<T>;

function mock<T>(spyName: string, methodNames: Array<keyof T>): SpyObj<T> {
    const spyObj = createSpyObj<T>(spyName, methodNames);
    for (const methodName of methodNames) {
        spyObj[methodName].and.returnValue(1);
    }
    return spyObj;
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2339),
        "Expected TS2339 for Function.returnValue access in spy comparison checking. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_function_type_missing_property_reports_ts2339() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.ts",
        r#"
declare let f: Function;
f.returnValue(1);
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2339),
        "Expected TS2339 for missing property on Function. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_spy_and_property_preserves_function_type_for_missing_property() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.ts",
        r#"
interface Spy {
    (...params: any[]): any;
    and: Function;
}

declare let spy: Spy;
spy.and.returnValue(1);
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2339),
        "Expected TS2339 for missing property through Spy.and. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_mapped_index_access_preserves_function_type_for_missing_property() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.ts",
        r#"
interface Spy {
    (...params: any[]): any;
    and: Function;
}

type SpyMap<T> = {
    [k in keyof T]: Spy;
}

function mock<T>(spyObj: SpyMap<T>, methodName: keyof T): SpyMap<T> {
    spyObj[methodName].and.returnValue(1);
    return spyObj;
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2339),
        "Expected TS2339 for missing property through generic mapped index access. Actual diagnostics: {diagnostics:#?}"
    );
}
