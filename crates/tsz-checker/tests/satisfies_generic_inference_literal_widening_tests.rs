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

fn diagnostics_with_code(source: &str, code: u32) -> Vec<(u32, String)> {
    diags(source)
        .into_iter()
        .filter(|diag| diag.0 == code)
        .collect()
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
fn satisfies_preserves_const_asserted_property_literal_in_typeof_alias() {
    let source = r##"
type Theme = { primary: string };

const theme = {
  primary: "#ff0000" as const,
} satisfies Theme;

type PrimaryType = typeof theme.primary;
const wrong: PrimaryType = "#0000ff";
"##;

    let all = diags(source);
    let ds: Vec<_> = all.iter().filter(|diag| diag.0 == 2322).collect();
    assert_eq!(
        ds.len(),
        1,
        "Expected TS2322 because `as const` property should stay literal through `satisfies`, got: {all:?}",
    );
    assert!(
        ds[0]
            .1
            .contains("Type '\"#0000ff\"' is not assignable to type '\"#ff0000\"'"),
        "Expected TS2322 to compare against the preserved literal, got: {ds:?}",
    );
}

#[test]
fn satisfies_preserves_non_widening_identifier_property_in_typeof_alias() {
    let source = r#"
type Limits = { retries: number };

const retryCount = 3 as const;
const config = {
  retries: retryCount,
} satisfies Limits;

type RetryCount = typeof config.retries;
const wrong: RetryCount = 4;
"#;

    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected TS2322 because non-widening identifier property should stay literal through `satisfies`, got: {ds:?}",
    );
    assert!(
        ds[0].1.contains("Type '4' is not assignable to type '3'"),
        "Expected TS2322 to compare against the preserved numeric literal, got: {ds:?}",
    );
}

#[test]
fn satisfies_accepts_nested_const_asserted_tuple_for_mutable_tuple_target() {
    let source = r##"
type ColorConfig = {
  red?: {
    hex: string;
    rgb: [number, number, number];
  };
};

const exactPalette = {
  red: { hex: "#ff0000" as const, rgb: [255, 0, 0] as const },
} satisfies ColorConfig;
"##;

    let ts1360 = diagnostics_with_code(source, 1360);
    assert!(
        ts1360.is_empty(),
        "Expected no TS1360 for nested readonly tuple satisfying mutable tuple target, got: {ts1360:?}",
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
