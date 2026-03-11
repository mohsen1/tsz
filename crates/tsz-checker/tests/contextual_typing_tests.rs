//! Tests for the circular return-type assignability fix.
//!
//! When a function/getter has no explicit return type annotation, the checker
//! infers the return type from the body.  Previously it then re-checked the
//! return statement against that inferred type, which could cause false TS2322
//! errors (e.g. for nested array literals with different object shapes).
//!
//! The fix pushes `TypeId::ANY` as the return type context when the return type
//! is purely inferred, so `check_return_statement` skips the circular check.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper: parse, bind, check with default options.
fn check_default(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// Function returning nested array literals with different object shapes should
/// NOT produce false TS2322.  The return type is purely inferred so there is no
/// external constraint to check against.
#[test]
fn test_no_false_ts2322_for_inferred_return_with_nested_arrays() {
    let source = r#"
function f() {
    return [
        ['a', { x: 1 }],
        ['b', { y: 2 }]
    ];
}
"#;
    let diagnostics = check_default(source);
    let ts2322_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322_errors.is_empty(),
        "Inferred return type should not cause circular TS2322 check, got: {ts2322_errors:?}"
    );
}

/// Getter returning nested array literals without annotation should not produce
/// false TS2322 — same circular-check avoidance applies to getters.
#[test]
fn test_no_false_ts2322_for_getter_inferred_return() {
    let source = r#"
class C {
    get x() {
        return [
            ['a', { x: 1 }],
            ['b', { y: 2 }]
        ];
    }
}
"#;
    let diagnostics = check_default(source);
    let ts2322_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322_errors.is_empty(),
        "Getter with inferred return should not cause circular TS2322, got: {ts2322_errors:?}"
    );
}

/// When a generic function has a rest parameter whose type is a mapped type Application
/// (e.g., `...values: UnwrapContainers<T>`), the Application must be evaluated before
/// contextual parameter extraction and function subtype comparison. Otherwise, each callback
/// parameter gets the whole tuple type instead of individual elements, causing false TS2345.
#[test]
fn test_no_false_ts2345_for_mapped_tuple_rest_spread() {
    let source = r#"
type Container<T> = { value: T };
type UnwrapContainers<T extends Container<unknown>[]> = { [K in keyof T]: T[K]['value'] };

declare function createContainer<T extends unknown>(value: T): Container<T>;
declare function f<T extends Container<unknown>[]>(
    containers: [...T],
    callback: (...values: UnwrapContainers<T>) => void
): void;

const c1 = createContainer('hi');
const c2 = createContainer(2);

f([c1, c2], (value1, value2) => {
    value1;
    value2;
});
"#;
    let diagnostics = check_default(source);
    let ts2345_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345_errors.is_empty(),
        "Mapped tuple rest spread should not produce false TS2345, got: {ts2345_errors:?}"
    );
}

/// When a function HAS an explicit return type, the check should still work.
/// This ensures we didn't disable return type checking entirely.
#[test]
fn test_annotated_return_type_still_checked() {
    let source = r#"
function f(): number {
    return "hello";
}
"#;
    let diagnostics = check_default(source);
    let ts2322_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322_errors.is_empty(),
        "Annotated return type should still produce TS2322 for type mismatch"
    );
}

#[test]
fn test_contextual_optional_parameter_question_token_in_named_function_expression() {
    let source = r#"
function acceptNum(num: number) {}

const f1: (a: string, b: number) => void = function self(a, b?) {
  acceptNum(b);
  self("");
  self("", undefined);
};

const f2: (a: string, b: number) => void = function self(a, b?: number) {
  acceptNum(b);
  self("");
  self("", undefined);
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    let diagnostics = checker.ctx.diagnostics.clone();
    let ts2345_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();

    assert_eq!(
        ts2345_errors.len(),
        2,
        "Expected two TS2345 errors for optional contextual parameters, got diagnostics={diagnostics:?}"
    );
}

#[test]
fn test_literal_source_display_through_object_literal_property_initializer() {
    let source = r#"
declare function test(
  arg: { a: () => "foo" } & {
    [k: string]: () => any;
  },
): unknown;

test({
  a: () => "bar",
});
"#;

    let diagnostics = check_default(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322, got diagnostics={diagnostics:?}"));

    assert!(
        ts2322.message_text.contains(r#""bar""#),
        "Expected literal source display in diagnostic, got {ts2322:?}"
    );
}

#[test]
fn test_optional_function_property_return_elaboration() {
    let source = r#"
interface IBookStyle {
    initialLeftPageTransforms?: (width: number) => NamedTransform[];
}

interface NamedTransform {
    [name: string]: Transform3D;
}

interface Transform3D {
    cachedCss: string;
}

var style: IBookStyle = {
    initialLeftPageTransforms: (width: number) => {
        return [
            {'ry': null }
        ];
    }
}
"#;

    let diagnostics = check_default(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322, got diagnostics={diagnostics:?}"));

    assert!(
        ts2322.message_text.contains("NamedTransform[]"),
        "Expected function-property diagnostic after elaboration, got {ts2322:?}"
    );
}

#[test]
fn test_deferred_mapped_intersection_preserves_contextual_property_types() {
    let source = r#"
type Action<TEvent extends { type: string }> = (ev: TEvent) => void;

interface MachineConfig2<TEvent extends { type: string }> {
  schema: {
    events: TEvent;
  };
  on?: {
    [K in TEvent["type"] as K extends Uppercase<string> ? K : never]?: Action<TEvent extends { type: K } ? TEvent : never>;
  } & {
    "*"?: Action<TEvent>;
  };
}

declare function createMachine2<TEvent extends { type: string }>(
  config: MachineConfig2<TEvent>
): void;

createMachine2({
  schema: {
    events: {} as { type: "FOO" } | { type: "bar" },
  },
  on: {
    FOO: (ev) => {
      ev.type;
    },
  },
});

createMachine2({
  schema: {
    events: {} as { type: "FOO" } | { type: "bar" },
  },
  on: {
    bar: (ev) => {
      ev;
    },
  },
});
"#;

    let diagnostics = check_default(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2353 || diag.code == 7006)
        .collect();

    // Current behavior: the compiler preserves enough contextual typing through the
    // mapped-type intersection that the invalid lowercase handler no longer falls back
    // to implicit `any`; only the real excess-property error remains.
    assert_eq!(
        relevant.iter().filter(|diag| diag.code == 7006).count(),
        0,
        "Expected no implicit-any diagnostics for the invalid lowercase handler, got diagnostics={relevant:?}"
    );
    assert_eq!(
        relevant.iter().filter(|diag| diag.code == 2353).count(),
        1,
        "Expected exactly one excess-property error for lowercase key, got diagnostics={relevant:?}"
    );
    let ts2353 = relevant
        .iter()
        .find(|diag| diag.code == 2353)
        .expect("expected TS2353 for lowercase key");
    assert!(
        ts2353.message_text.contains("'bar'"),
        "Expected TS2353 for lowercase key, got {ts2353:?}"
    );
    assert!(
        ts2353.message_text.contains("{ FOO?:")
            || ts2353.message_text.contains(r#"& { "*"?:"#)
            || ts2353.message_text.contains(r#"& { '*'?:"#),
        "Expected TS2353 target to mention the mapped intersection, got {ts2353:?}"
    );
}

#[test]
fn test_contextual_function_object_property_intersection_sequence() {
    let source = r#"
type Action<TEvent extends { type: string }> = (ev: TEvent) => void;

interface MachineConfig<TEvent extends { type: string }> {
  schema: {
    events: TEvent;
  };
  on?: {
    [K in TEvent["type"]]?: Action<TEvent extends { type: K } ? TEvent : never>;
  } & {
    "*"?: Action<TEvent>;
  };
}

declare function createMachine<TEvent extends { type: string }>(
  config: MachineConfig<TEvent>
): void;

createMachine({
  schema: {
    events: {} as { type: "FOO" } | { type: "BAR" },
  },
  on: {
    FOO: (ev) => {
      ev.type;
    },
  },
});

createMachine({
  schema: {
    events: {} as { type: "FOO" } | { type: "BAR" },
  },
  on: {
    "*": (ev) => {
      ev.type;
    },
  },
});

interface MachineConfig2<TEvent extends { type: string }> {
  schema: {
    events: TEvent;
  };
  on?: {
    [K in TEvent["type"] as K extends Uppercase<string> ? K : never]?: Action<TEvent extends { type: K } ? TEvent : never>;
  } & {
    "*"?: Action<TEvent>;
  };
}

declare function createMachine2<TEvent extends { type: string }>(
  config: MachineConfig2<TEvent>
): void;

createMachine2({
  schema: {
    events: {} as { type: "FOO" } | { type: "bar" },
  },
  on: {
    FOO: (ev) => {
      ev.type;
    },
  },
});

createMachine2({
  schema: {
    events: {} as { type: "FOO" } | { type: "bar" },
  },
  on: {
    "*": (ev) => {
      ev.type;
    },
  },
});

createMachine2({
  schema: {
    events: {} as { type: "FOO" } | { type: "bar" },
  },
  on: {
    bar: (ev) => {
      ev;
    },
  },
});
"#;

    let diagnostics = check_default(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2353 || diag.code == 7006)
        .collect();

    // Current behavior: the compiler preserves enough contextual typing through the
    // mapped-type intersection that the lowercase excess-property site no longer falls
    // back to implicit `any`; only the real excess-property error remains.
    assert_eq!(
        relevant.iter().filter(|diag| diag.code == 7006).count(),
        0,
        "Expected no implicit-any diagnostics in the full sequence, got diagnostics={relevant:?}"
    );
    assert_eq!(
        relevant.iter().filter(|diag| diag.code == 2353).count(),
        1,
        "Expected exactly one excess-property error for lowercase key, got diagnostics={relevant:?}"
    );
    let ts2353 = relevant
        .iter()
        .find(|diag| diag.code == 2353)
        .expect("expected TS2353 for lowercase key");
    assert!(
        ts2353.message_text.contains("'bar'"),
        "Expected TS2353 for lowercase key, got {ts2353:?}"
    );
    assert!(
        ts2353.message_text.contains("{ FOO?:")
            || ts2353.message_text.contains(r#"& { "*"?:"#)
            || ts2353.message_text.contains(r#"& { '*'?:"#),
        "Expected TS2353 target to mention the filtered mapped intersection, got {ts2353:?}"
    );
}

#[test]
fn test_validate_slice_case_reducers_does_not_fail_overload_resolution() {
    let source = r#"
declare function createSlice<T>(
  reducers: { [K: string]: (state: string) => void } & {
    [K in keyof T]: object;
  }
): void;

type SliceCaseReducers<State> = Record<string, (state: State) => State | void>;

type ValidateSliceCaseReducers<S, ACR extends SliceCaseReducers<S>> = ACR & {
  [T in keyof ACR]: ACR[T] extends {
    reducer(s: S, action?: infer A): any;
  }
    ? {
        prepare(...a: never[]): Omit<A, "type">;
      }
    : {};
};

declare function createSlice<
  State,
  CaseReducers extends SliceCaseReducers<State>
>(options: {
  initialState: State | (() => State);
  reducers: ValidateSliceCaseReducers<State, CaseReducers>;
}): void;

export const clientSlice = createSlice({
  initialState: {
    username: "",
    isLoggedIn: false,
    userId: "",
    avatar: "",
  },
  reducers: {
    onClientUserChanged(state) {},
  },
});
"#;

    let diagnostics = check_default(source);
    let overload_errors: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2769)
        .collect();

    assert!(
        overload_errors.is_empty(),
        "Expected ValidateSliceCaseReducers example to avoid overload failure, got diagnostics={diagnostics:?}"
    );
}
