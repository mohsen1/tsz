//! Tests for #6255 — satisfies-operand literal widening through generic
//! inference.
//!
//! ## Structural rule
//!
//! When an expression is the operand of `satisfies T`, object-literal property
//! widening follows tsc's `isLiteralOfContextualType` per-property gate
//! (checker.ts ~26880): a property value's literal kind is preserved only when
//! the property's contextual type accepts that kind (e.g. `true` against
//! `boolean = true | false`, or `5` against literal `5`), and widened to its
//! primitive otherwise (e.g. `5` against the primitive `number`). The
//! resulting type then flows through generic-call inference unchanged, because
//! `satisfies T` is recognized as a "type-annotated source" alongside `as T`
//! and typed identifiers — the deep-widen pass that compensates for tsz's
//! coarser non-satisfies widening must not run a second time on a satisfies
//! operand.
//!
//! The rule is keyed on structure, not on identifier spelling: renaming the
//! type parameter, the property names, or the satisfies-type alias must not
//! change the decision (§25 ANTI-HARDCODING DIRECTIVE).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn diags(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn ts2322_diags(source: &str) -> Vec<(u32, String)> {
    diags(source).into_iter().filter(|d| d.0 == 2322).collect()
}

#[test]
fn boolean_literal_preserved_through_generic_identity_with_satisfies() {
    // Canonical repro from the issue.
    let source = r#"
function createConfig<T>(config: T): T {
  return config;
}

const myConfig = createConfig({
  debug: true,
} satisfies { debug: boolean });

const _mcd: true = myConfig.debug;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 — `true` preserved against `boolean` contextual, got: {ds:?}",
    );
}

#[test]
fn number_literal_widens_against_primitive_number_contextual() {
    // Sibling of the boolean case: tsc widens `5` to `number` because the
    // contextual `number` is a primitive without a `NumberLiteral` flag.
    let source = r#"
function createConfig<T>(config: T): T {
  return config;
}

const myConfig = createConfig({
  level: 5,
} satisfies { level: number });

const _five: 5 = myConfig.level;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 — `5` widens to `number` against primitive contextual, got: {ds:?}",
    );
}

#[test]
fn number_literal_preserved_when_contextual_is_literal_number() {
    // When the contextual type IS the literal `5`, the literal kind matches
    // and the value is preserved.
    let source = r#"
function id<T>(x: T): T { return x; }
const cfg = id({ level: 5 } satisfies { level: 5 });
const _five: 5 = cfg.level;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 — `5` preserved against literal `5` contextual, got: {ds:?}",
    );
}

#[test]
fn string_literal_widens_against_primitive_string_contextual() {
    let source = r#"
function id<T>(x: T): T { return x; }
const cfg = id({ name: "foo" } satisfies { name: string });
const _foo: "foo" = cfg.name;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 — string literal widens against primitive `string` contextual, got: {ds:?}",
    );
}

#[test]
fn string_literal_preserved_against_literal_union_contextual() {
    let source = r#"
function id<T>(x: T): T { return x; }
const cfg = id({ kind: "a" } satisfies { kind: "a" | "b" | "c" });
const _a: "a" = cfg.kind;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 — string literal preserved against literal-union contextual, got: {ds:?}",
    );
}

#[test]
fn boolean_literal_preserved_with_renamed_type_parameter_and_property() {
    // Renaming the type parameter (`T`→`X`), the property (`debug`→`enabled`),
    // and the satisfies-type alias name must not change the decision: the rule
    // is structural, not spelling-based.
    let source = r#"
function wrap<X>(value: X): X { return value; }

const result = wrap({
  enabled: false,
} satisfies { enabled: boolean });

const _f: false = result.enabled;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 with renamed parameters/properties, got: {ds:?}",
    );
}

#[test]
fn satisfies_operand_widens_property_per_property_under_multi_param_generic() {
    // The trivial single-type-param fast path doesn't apply for
    // `<T, U>(a: T, b: U): [T, U]` (T sits inside a tuple in the return type,
    // not at top level), so tsz's general inference still deep-widens fresh
    // object candidates here. The satisfies-time per-property widening must
    // still fire so `5` is `number` and `"x"` is `string` BEFORE inference
    // sees them — the bug for which this regression guard exists is that
    // those property values would otherwise be preserved as `5` / `"x"` and
    // then collapsed by the general path's deep-widen to `number` / `string`
    // with no observable difference, but more importantly: the satisfies
    // result's display would be wrong. Asserting against the widened types
    // shows the satisfies-time path runs and matches tsc.
    let source = r#"
function pair<T, U>(a: T, b: U): [T, U] { return [a, b]; }
const p = pair(
  { level: 5 } satisfies { level: number },
  { name: "x" } satisfies { name: string },
);
const _n: number = p[0].level;
const _s: string = p[1].name;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 — satisfies widens `5`/`\"x\"` per-property to `number`/`string`, got: {ds:?}",
    );
}

#[test]
fn nested_satisfies_inside_object_literal_at_outer_satisfies() {
    // The flag is set lexically through the satisfies handler so nested
    // satisfies operands inherit the widening regime correctly.
    let source = r#"
function id<T>(x: T): T { return x; }
const cfg = id(
  {
    outer: { inner: 5 } satisfies { inner: number },
  } satisfies { outer: { inner: number } }
);
const _n: number = cfg.outer.inner;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 in nested satisfies — inner widens, outer carries widened, got: {ds:?}",
    );
}

#[test]
fn satisfies_propagates_boolean_literal_into_typeof_alias() {
    // The variable-declaration path must also see the preserved literal so
    // that `typeof` of the call result reflects `true`, not `boolean`.
    let source = r#"
function id<T>(x: T): T { return x; }
const cfg = id({ flag: true } satisfies { flag: boolean });
type CfgFlag = typeof cfg.flag;
const f1: CfgFlag = true;
const f2: true = cfg.flag;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 — typeof of generic call result preserves boolean literal, got: {ds:?}",
    );
}

#[test]
fn satisfies_in_paren_wrapper_recognized_as_type_annotated_source() {
    let source = r#"
function id<T>(x: T): T { return x; }
const cfg = id(({ flag: true } satisfies { flag: boolean }));
const _t: true = cfg.flag;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 — parenthesized satisfies still type-annotated source, got: {ds:?}",
    );
}

#[test]
fn satisfies_applies_per_property_gate_to_shorthand_property_initializers() {
    // The shorthand-property path in object_literal/computation.rs goes through
    // a separate widening branch from named properties; both must honour the
    // satisfies per-property gate.
    let source = r#"
function id<T>(x: T): T { return x; }
const flag = true;
const cfg = id({ flag } satisfies { flag: boolean });
const _t: true = cfg.flag;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 — shorthand property with boolean literal preserved against `boolean`, got: {ds:?}",
    );
}

#[test]
fn satisfies_with_unknown_target_widens_all_property_literals() {
    // `unknown` has no literal flags, so every property value widens —
    // matching tsc's `isLiteralOfContextualType` returning false for unknown.
    let source = r#"
function id<T>(x: T): T { return x; }
const cfg = id({ a: 1, b: "s", c: true } satisfies unknown);
const _na: 1 = cfg.a;
const _nb: "s" = cfg.b;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        2,
        "Expected two TS2322 — `1` widens to `number` and `\"s\"` widens to `string` against unknown, got: {ds:?}",
    );
}

#[test]
fn non_satisfies_path_still_widens_via_generic_inference_deep_widen() {
    // Regression guard: outside a satisfies operand, the existing coarse
    // preservation at the object literal level plus the generic-call deep-widen
    // continues to widen literal property types. Renaming the type parameter
    // and the property must not change this.
    let source = r#"
function passthrough<U>(value: U): U { return value; }
const r = passthrough({ option: 7 });
const _seven: 7 = r.option;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected one TS2322 — without satisfies, `7` deep-widens to `number`, got: {ds:?}",
    );
}

#[test]
fn satisfies_return_type_inferred_as_constraint_not_literal() {
    // Issue #6798: the return type of a function returning a satisfies-constrained
    // const should use the constraint type, not the preserved literal type.
    // tsc infers { version: number; features: string[] } as the return type.
    let source = r#"
function createConfig() {
  const cfg = {
    version: 1,
    features: ["a", "b"]
  } satisfies { version: number; features: string[] };
  return cfg;
}

const cfg = createConfig();
const version: 1 = cfg.version;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected TS2322 on 'const version: 1 = cfg.version' (return type should be number, not 1), got: {ds:?}",
    );
}

#[test]
fn satisfies_direct_return_inferred_as_constraint() {
    // Adjacent case: directly returning a satisfies expression.
    // tsc: return type { version: number }
    let source = r#"
function makeConfig() {
  return { version: 1, name: "x" } satisfies { version: number; name: string };
}
const c = makeConfig();
const v: 1 = c.version;
const n: "x" = c.name;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        2,
        "Expected 2 TS2322 (version: number not 1, name: string not 'x'), got: {ds:?}",
    );
}

#[test]
fn satisfies_through_renamed_variables_return_constraint() {
    // Renaming the binding and properties must not change widening behavior.
    let source = r#"
function build() {
  const result = { count: 5, label: "hello" } satisfies { count: number; label: string };
  return result;
}
const r = build();
const c: 5 = r.count;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected TS2322 — count should be number (not 5) in return type, got: {ds:?}",
    );
}

#[test]
fn satisfies_return_array_property_widened_in_return_type() {
    // Array literal properties in satisfies should also be widened in return type.
    let source = r#"
function createConfig() {
  const cfg = {
    version: 1,
    features: ["a", "b"]
  } satisfies { version: number; features: string[] };
  return cfg;
}

const cfg = createConfig();
const f: string[] = cfg.features;
"#;
    let ds = diags(source);
    let ts2322: Vec<_> = ds.iter().filter(|d| d.0 == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 — features should be string[] in return type, got: {ds:?}",
    );
}

#[test]
fn satisfies_return_boolean_property_preserved_as_literal() {
    // Boolean literal properties against `boolean` context should be PRESERVED (not widened).
    // tsc preserves `true` because boolean = true | false is a literal union.
    let source = r#"
function createConfig() {
  const cfg = { debug: true } satisfies { debug: boolean };
  return cfg;
}
const cfg = createConfig();
const d: true = cfg.debug;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 — boolean literals preserved through satisfies + function return, got: {ds:?}",
    );
}

#[test]
fn satisfies_return_nested_object_widened_in_return_type() {
    // Nested objects inside satisfies: number literals widen.
    let source = r#"
function make() {
  const cfg = { inner: { value: 42 } } satisfies { inner: { value: number } };
  return cfg;
}
const m = make();
const v: 42 = m.inner.value;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected TS2322 — nested value: number in return type, got: {ds:?}",
    );
}

#[test]
fn satisfies_with_type_alias_return_widened() {
    // Type alias for the satisfies constraint should behave the same way.
    let source = r#"
type Config = { version: number; name: string };
function build() {
  const cfg = { version: 3, name: "foo" } satisfies Config;
  return cfg;
}
const c = build();
const v: 3 = c.version;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected TS2322 (version: number not 3), got: {ds:?}",
    );
}

#[test]
fn satisfies_preserves_boolean_literal_through_let_variable_return() {
    // When the satisfies result is assigned to a let variable (not const),
    // the return type inference should still use widened types for non-boolean literals.
    let source = r#"
function f() {
  let cfg = { count: 7 } satisfies { count: number };
  return cfg;
}
const result = f();
const n: 7 = result.count;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected TS2322 — count should widen to number in return type even from let binding, got: {ds:?}",
    );
}

#[test]
fn satisfies_unconstrained_extra_property_literal_preserved() {
    // Properties without a satisfies constraint should still preserve their literal types.
    // tsc: `extra` property has type "abc" (no constraint to widen against).
    let source = r#"
function f() {
  const cfg = { version: 1, extra: "abc" } satisfies { version: number };
  return cfg;
}
const c = f();
const e: "abc" = c.extra;
"#;
    let ds = ts2322_diags(source);
    assert!(
        ds.is_empty(),
        "Expected no TS2322 — unconstrained property 'extra' preserves literal 'abc', got: {ds:?}",
    );
}

#[test]
fn satisfies_constrained_property_widened_unconstrained_preserved() {
    // Mixed: constrained properties widen, unconstrained properties preserve.
    let source = r#"
function f() {
  const cfg = { version: 1, tag: "alpha" } satisfies { version: number };
  return cfg;
}
const c = f();
const tag: "alpha" = c.tag;
const version: 1 = c.version;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly 1 TS2322 (version widened to number) but tag preserved as 'alpha', got: {ds:?}",
    );
}
