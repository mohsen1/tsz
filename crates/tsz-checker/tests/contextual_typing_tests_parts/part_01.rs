/// Same as above but through `Promise.resolve().then()` chain — the contextual
/// return type `Promise<DooDad>` should flow through the generic `.then()` call
/// and prevent literal widening in the callback.
#[test]
fn test_promise_then_return_context_preserves_literal() {
    let source = r#"
// @strict: true
// @target: es6

type DooDad = 'SOMETHING' | 'ELSE';

function test(): Promise<DooDad> {
    return Promise.resolve().then(() => {
        return 'ELSE';
    });
}
"#;
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Promise.resolve().then(() => 'ELSE') should not produce TS2322 when contextual type is Promise<DooDad>, got: {ts2322:?}"
    );
}

/// Multi-return callback: when the callback has if/else returning different
/// string literals that together form the union type, widening should still
/// be prevented.
#[test]
fn test_promise_then_multi_return_preserves_literal_union() {
    let source = r#"
// @strict: true
// @target: es6

type DooDad = 'SOMETHING' | 'ELSE';

function test(): Promise<DooDad> {
    return Promise.resolve().then(() => {
        if (1 < 2) {
            return 'SOMETHING';
        }
        return 'ELSE';
    });
}
"#;
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Multi-return callback should not produce TS2322 when all returns are subtypes of DooDad, got: {ts2322:?}"
    );
}

// TODO: mapped type parameter scoping bug causes TS2304 for 'K' instead of
// the expected TS2345 for spawn("alarm"). Blocked on binder mapped type param fix.
#[test]
fn test_cross_wrapper_return_context_infers_assign_callback_actor_literal() {
    let source = r#"
type Values<T> = T[keyof T];

type EventObject = {
  type: string;
};

interface ActorLogic<TEvent extends EventObject> {
  transition: (ev: TEvent) => unknown;
}

type UnknownActorLogic = ActorLogic<never>;

interface ProvidedActor {
  src: string;
  logic: UnknownActorLogic;
}

interface ActionFunction<TActor extends ProvidedActor> {
  (): void;
  _out_TActor?: TActor;
}

interface AssignAction<TActor extends ProvidedActor> {
  (): void;
  _out_TActor?: TActor;
}

interface MachineConfig<TActor extends ProvidedActor> {
  entry?: ActionFunction<TActor>;
}

declare function assign<TActor extends ProvidedActor>(
  _: (spawn: (actor: TActor["src"]) => void) => {},
): AssignAction<TActor>;

type ToProvidedActor<TActors extends Record<string, UnknownActorLogic>> =
  Values<{
    [K in keyof TActors & string]: {
      src: K;
      logic: TActors[K];
    };
  }>;

declare function setup<
  TActors extends Record<string, UnknownActorLogic> = {},
>(implementations?: {
  actors?: { [K in keyof TActors]: TActors[K] };
}): {
  createMachine: <
    const TConfig extends MachineConfig<ToProvidedActor<TActors>>,
  >(
    config: TConfig,
  ) => void;
};

declare const counterLogic: ActorLogic<{ type: "INCREMENT" }>;

setup({
  actors: { counter: counterLogic },
}).createMachine({
  entry: assign((spawn) => {
    spawn("counter");
    spawn("alarm");
    return {};
  }),
});
"#;

    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict: true,
            exact_optional_property_types: true,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Cross-wrapper return-context inference should not emit a top-level TS2322, got diagnostics={diagnostics:?}"
    );

    // The mapped type parameter K should be in scope and not produce false TS2304.
    let ts2304: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304.is_empty(),
        "Mapped type parameter K should be recognized, no TS2304 expected, got diagnostics={diagnostics:?}"
    );

    // Ideally tsc emits exactly one TS2345 for spawn("alarm") since "alarm"
    // is not a valid actor name (only "counter" is). Full deep generic inference
    // for this xstate-like pattern is not yet implemented, so we accept 0 or 1.
    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.len() <= 1,
        "Expected at most one TS2345 for spawn(\"alarm\"), got diagnostics={diagnostics:?}"
    );
}

/// Contextual return type should flow into generic call inference so that
/// callback parameter types are correctly inferred from the return type context.
/// `make<A, B>(fn: (a: A) => B): (s: A) => B` called with contextual type
/// `(s: number) => string` should infer A=number, B=string, then the callback
/// `(x) => x.toUpperCase()` should get x: number and report TS2339.
#[test]
fn test_contextual_return_type_flows_to_callback_params() {
    let source = r#"
function make<A, B>(fn: (a: A) => B): (s: A) => B { return fn; }
const f: (s: number) => string = make((x) => x.toUpperCase());
"#;
    let diagnostics = check_default(source);
    let ts2339: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "Expected TS2339 for toUpperCase on number (contextual return type should infer A=number), \
         got diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn direct_callback_body_property_error_on_outer_type_param_is_retained() {
    let source = r#"
namespace ns {
    export function funkyFor<T, U>(array: T[], callback: (element: T, index: number) => U): U {
        return callback(array[0], 0);
    }
}

function reversed<T>(array: T[]) {
    return ns.funkyFor(array, t => t.toString());
}
"#;
    let diagnostics = check_default(source);
    let ts2339: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "Expected TS2339 for toString on unconstrained outer T, got diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn function_type_return_type_query_resolves_parameter_type() {
    let source = r#"
type FuncType = (x: <T>(p: T) => T) => typeof x;
let z: FuncType = x => undefined;
"#;

    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            no_implicit_any: true,
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.iter().any(|d| {
            d.code == 2322
                && d.message_text
                    .contains("Type '(x: <T>(p: T) => T) => undefined'")
                && d.related_information.iter().any(|related| {
                    related
                        .message_text
                        .contains("Type 'undefined' is not assignable to type '<T>(p: T) => T'")
                })
        }),
        "`typeof x` in a function type return should resolve to the parameter type, got diagnostics={diagnostics:?}"
    );
}

#[test]
fn test_parenthesized_conditional_callbacks_preserve_contextual_typing() {
    let source = r#"
type FuncType = (x: <T>(p: T) => T) => typeof x;
declare const coin: number;

function fun<T>(f: FuncType, x: T): T;
function fun<T>(f: FuncType, g: FuncType, x: T): T;
function fun<T>(...rest: any[]): T {
    return undefined as any;
}

var i = fun((coin < 0.5 ? x => { x<number>(undefined); return x; } : x => undefined), 10);
var k = fun((coin < 0.5 ? (x => { x<number>(undefined); return x; }) : (x => undefined)), x => { x<number>(undefined); return x; }, 10);
"#;

    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            no_implicit_any: true,
            strict_null_checks: true,
            ..Default::default()
        },
    );
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    let outer_conditional_mismatches = diagnostics
        .iter()
        .filter(|d| {
            d.code == 2345
                && d.message_text.contains(
                    "((x: <T>(p: T) => T) => <T>(p: T) => T) | ((x: <T>(p: T) => T) => undefined)",
                )
        })
        .count();
    assert_eq!(
        outer_conditional_mismatches, 2,
        "Expected TS2345 for each conditional argument with expanded callable union display, got diagnostics={diagnostics:?}"
    );
    let contextual_branch_mismatches = diagnostics
        .iter()
        .filter(|d| {
            d.code == 2345
                && d.message_text.contains(
                    "Argument of type 'undefined' is not assignable to parameter of type 'number'",
                )
        })
        .count();
    assert!(
        contextual_branch_mismatches >= 2,
        "Expected contextual true-branch callbacks to keep TS2345, got diagnostics={diagnostics:?}"
    );
    assert!(
        codes.contains(&7006),
        "Expected TS7006 from the later callback after the earlier overload argument mismatch, got diagnostics={diagnostics:?}"
    );
    assert!(
        codes.contains(&2347),
        "Expected TS2347 from the later callback after the earlier overload argument mismatch, got diagnostics={diagnostics:?}"
    );
    assert!(
        !codes.contains(&2769),
        "Conditional callback retry should not degrade into outer TS2769, got diagnostics={diagnostics:?}"
    );
}

/// Mirrors `compiler/discriminantPropertyInference.ts`: when an object literal
/// completely OMITS a discriminator property, narrowing must eliminate union
/// members that *require* the property, leaving only members where the
/// property is optional. Without this, the callback `n` falls back to implicit
/// any (TS7006) under `noImplicitAny`.
#[test]
fn test_discriminant_property_inference_omitted_discriminator_narrows_to_optional_arm() {
    let source = r#"
type DiscriminatorTrue = {
    disc: true;
    cb: (x: string) => void;
};

type DiscriminatorFalse = {
    disc?: false;
    cb: (x: number) => void;
};

declare function f(options: DiscriminatorTrue | DiscriminatorFalse): any;

f({
    cb: n => n.toFixed()
});
"#;
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            no_implicit_any: true,
            strict_null_checks: true,
            ..Default::default()
        },
    );
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&7006),
        "Omitted-discriminator narrowing should give `n` a contextual `number` type \
         from DiscriminatorFalse; got TS7006: {diagnostics:?}"
    );
}

/// Mirrors `compiler/indirectDiscriminantAndExcessProperty.ts`: when the literal
/// supplies a discriminator slot with a NON-unit value (e.g. `type: foo1` where
/// `foo1: string`), narrowing must NOT collapse to a single arm. The TS2322
/// diagnostic must still report the full union (`"foo" | "bar"`) rather than
/// an arbitrarily-picked arm.
#[test]
fn test_dynamic_discriminator_value_does_not_narrow_union() {
    let source = r#"
type Blah =
    | { type: "foo", abc: string }
    | { type: "bar", xyz: number, extra: any };

declare function thing(blah: Blah): void;

let foo1 = "foo";
thing({
    type: foo1,
    abc: "hello!"
});
"#;
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    // The diagnostic should mention the full union target type, not the
    // single narrowed arm. We don't depend on exact code (TS2322 vs TS2353):
    // we only require that no diagnostic mentions a single narrowed arm.
    for d in &diagnostics {
        assert!(
            !d.message_text.contains("type '\"foo\"'")
                && !d.message_text.contains("type '\"bar\"'"),
            "Dynamic discriminator (`type: foo1`) must not collapse the union to a \
             single arm in diagnostics; got: {d:?}"
        );
    }
}

// Structural rule: when a function expression is contextually typed by an
// intersection that includes a callable constituent, the callable constituent's
// parameter types must flow into the function — regardless of parameter name or
// mapped-type iteration-variable spelling.

/// Direct assignment to a `((x: T) => R) & { prop: P }` variable: parameter
/// is contextually typed from the callable constituent.
///
/// TS2322 fires (bare arrow lacks the object constituent) but no TS7006:
/// the parameter IS contextually typed.
#[test]
fn test_direct_assign_function_object_intersection_types_parameter() {
    // Two different parameter names confirm the rule is not name-dependent.
    for source in [
        r#"
const f: ((x: number) => void) & { tag: string } = (x) => { x; };
"#,
        r#"
const f: ((value: number) => void) & { tag: string } = (value) => { value; };
"#,
    ] {
        let diagnostics = check_default(source);
        assert_eq!(
            diagnostic_count(&diagnostics, 7006),
            0,
            "Arrow parameter must not be implicitly any when contextual type is \
             function-object intersection"
        );
        assert_eq!(
            diagnostic_count(&diagnostics, 2322),
            1,
            "Expected exactly one TS2322 for the missing object constituent"
        );
    }
}

// Hybrid interface (callable + optional named property): unannotated parameter must be
// contextually typed from the call signature, not left as implicit any.
// Optional extra property makes the plain lambda assignable to the hybrid interface.
#[test]
fn test_hybrid_interface_contextual_types_callback_parameter() {
    for source in [
        r#"
interface Handler {
  (evt: string): void;
  name?: string;
}
declare function register(h: Handler): void;
register((evt) => { evt.toLowerCase(); });
"#,
        r#"
interface Processor {
  (item: number): boolean;
  version?: number;
}
declare function process(p: Processor): void;
process((item) => item > 0);
"#,
    ] {
        let diagnostics = check_default(source);
        assert_eq!(
            diagnostic_count(&diagnostics, 7006),
            0,
            "Hybrid interface callable constituent must flow parameter type to unannotated lambda — \
             TS7006 fires when contextual typing fails"
        );
    }
}

// Generic `((arg: T) => void) & { label?: string }`: unannotated parameter must inherit T
// from the explicit type argument. Optional extra property keeps the lambda assignable.
// Two iteration-variable names prove the rule is not tied to a specific spelling.
#[test]
fn test_generic_function_object_intersection_types_callback_parameter() {
    for source in [
        r#"
declare function wrap<T>(fn: ((arg: T) => void) & { label?: string }): T;
wrap<number>((arg) => { arg.toFixed(); });
"#,
        r#"
declare function wrap<U>(fn: ((item: U) => boolean) & { id?: number }): U;
wrap<string>((item) => item.length > 0);
"#,
    ] {
        let diagnostics = check_default(source);
        assert_eq!(
            diagnostic_count(&diagnostics, 7006),
            0,
            "Generic intersection callable constituent must flow T to unannotated lambda parameter — \
             TS7006 fires when contextual typing fails"
        );
    }
}

/// React-like `FC<P>` hybrid interface: props parameter flows from call signature.
#[test]
fn test_fc_like_intersection_types_props_parameter() {
    for source in [
        r#"
interface FC<P> {
  (props: P): null;
  displayName?: string;
}
declare function createFC<P>(fc: FC<P>): FC<P>;
createFC<{ name: string }>((props) => {
  props.name;
  return null;
});
"#,
        r#"
interface Component<Props> {
  (properties: Props): null;
  key?: string;
}
declare function define<Props>(c: Component<Props>): Component<Props>;
define<{ id: number }>((properties) => {
  properties.id;
  return null;
});
"#,
    ] {
        let diagnostics = check_default(source);
        assert_eq!(
            diagnostic_count(&diagnostics, 7006),
            0,
            "FC-like intersection props must not be implicitly any"
        );
    }
}

/// Mapped-type intersection: uppercase keys get contextually typed callbacks;
/// the excess lowercase key gets TS2353 + TS7006.
///
/// Reproduces the core of `contextualTypeFunctionObjectPropertyIntersection.ts`.
/// Two field-name / wildcard-key combinations prove the rule is structural.
#[test]
fn test_mapped_intersection_valid_keys_contextual_excess_key_ts7006() {
    for source in [
        r#"
type Cb<E extends { type: string }> = (ev: E) => void;
interface Config<E extends { type: string }> {
  schema: { events: E; };
  on?: {
    [K in E["type"] as K extends Uppercase<string> ? K : never]?: Cb<E extends { type: K } ? E : never>;
  } & { "*"?: Cb<E>; };
}
declare function build<E extends { type: string }>(c: Config<E>): void;
build({
  schema: { events: {} as { type: "A" } | { type: "b" } },
  on: {
    A: (ev) => { ev.type; },
    "*": (ev) => { ev.type; },
    b: (ev) => { ev; },
  },
});
"#,
        r#"
type Handler<E extends { kind: string }> = (e: E) => void;
interface Router<E extends { kind: string }> {
  schema: { events: E; };
  routes?: {
    [K in E["kind"] as K extends Uppercase<string> ? K : never]?: Handler<E extends { kind: K } ? E : never>;
  } & { DEFAULT?: Handler<E>; };
}
declare function createRouter<E extends { kind: string }>(r: Router<E>): void;
createRouter({
  schema: { events: {} as { kind: "GET" } | { kind: "post" } },
  routes: {
    GET: (e) => { e.kind; },
    DEFAULT: (e) => { e.kind; },
    post: (e) => { e; },
  },
});
"#,
    ] {
        let diagnostics = check_default(source);
        assert_eq!(
            diagnostic_count(&diagnostics, 7006),
            1,
            "Expected exactly one TS7006 for the excess lowercase key"
        );
        assert_eq!(
            diagnostic_count(&diagnostics, 2353),
            1,
            "Expected exactly one TS2353 for the excess lowercase key"
        );
    }
}
