//! Tests for the arena-direct fast path in
//! `extract_simple_type_params_from_decl_in_arena` covering the
//! "class or interface with no AST type-parameters" case. Previously
//! the class case fell through to a `with_parent_cache_attributed`
//! child checker just to run a JSDoc @template scan and return
//! `Some(Vec::new())`; the interface case should stay an arena-only
//! empty parameter list.
//!
//! The structural rule under test:
//!
//! > When a class declaration has no `<...>` AST type-parameter list,
//! > its type-parameter set is either (a) the names declared by a
//! > leading JSDoc `@template` tag, or (b) empty. When an interface has
//! > no `<...>` list, its type-parameter set is empty. Both can be
//! > computed from the arena alone, without constructing a checker.
//!
//! These tests are integration-level (`check_source`) and use two
//! distinct identifier names (`T` and `K`) for the bound variable so
//! a regression that hardcodes the name `T` cannot pass.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_js_source_code_messages_with_options, check_source_code_messages,
};

fn check_js(source: &str) -> Vec<(u32, String)> {
    check_js_source_code_messages_with_options(source, "test.js", CheckerOptions::default())
}

fn check_ts(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

#[test]
fn plain_class_no_type_params_resolves_empty() {
    // A plain TypeScript class with no AST type parameters and no JSDoc.
    // Must not emit TS2314 ("Generic type 'X' requires 0 type argument(s)")
    // for downstream usage. This exercises the arena-direct branch that
    // returns Some(Vec::new()) without constructing a child checker.
    let source = r#"
class Foo {}
let f: Foo = new Foo();
"#;
    let diags = check_ts(source);
    assert!(
        diags.is_empty(),
        "expected no diagnostics for plain class, got: {diags:?}",
    );
}

#[test]
fn plain_interface_no_type_params_resolves_empty() {
    // A plain interface with no AST type parameters should also resolve as
    // non-generic through the arena-direct path rather than falling back to a
    // child checker.
    let source = r#"
interface Box {
    value: string;
}
let b: Box = { value: "ok" };
"#;
    let diags = check_ts(source);
    assert!(
        diags.is_empty(),
        "expected no diagnostics for plain interface, got: {diags:?}",
    );
}

#[test]
fn interface_value_merge_preserves_interface_type_params() {
    // Some lib-style symbols merge a value declaration with an interface. The
    // value declaration candidate cannot contribute type params, but it also
    // must not block the later interface declaration from providing them.
    let source = r#"
interface Box<T> {
    value: T;
}
declare var Box: {
    new <T>(value: T): Box<T>;
};
let b: Box<string> = new Box("ok");
"#;
    let diags = check_ts(source);
    assert!(
        diags.is_empty(),
        "expected merged interface/value type params to resolve, got: {diags:?}",
    );
}

#[test]
fn class_at_template_t_visible_via_typeof() {
    // JS class with `@template T` — must be treated as a single-param
    // generic class. Cross-file usage via `/** @type {Foo<string>} */`
    // must not error.
    let source = r#"
/** @template T */
class Foo {
    /** @param {T} x */
    constructor(x) { this.value = x; }
}
/** @type {Foo<string>} */
let f;
"#;
    let diags = check_js(source);
    let bad: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(c, _)| *c == 2304 || *c == 2314)
        .collect();
    assert!(
        bad.is_empty(),
        "expected no TS2304/TS2314 for @template T, got: {diags:?}",
    );
}

#[test]
fn class_at_template_k_visible_via_typeof() {
    // Same as above but using `K` as the parameter name — if the fast
    // path hardcoded `T`, this test would fail.
    let source = r#"
/** @template K */
class Bag {
    /** @param {K} item */
    add(item) { this.item = item; }
}
/** @type {Bag<number>} */
let b;
"#;
    let diags = check_js(source);
    let bad: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(c, _)| *c == 2304 || *c == 2314)
        .collect();
    assert!(
        bad.is_empty(),
        "expected no TS2304/TS2314 for @template K, got: {diags:?}",
    );
}

#[test]
fn class_at_template_two_params() {
    // Two type parameters declared on one @template line — exercises the
    // multi-name parser path through the arena-direct extractor.
    let source = r#"
/** @template A, B */
class Pair {
    /**
     * @param {A} first
     * @param {B} second
     */
    constructor(first, second) {
        this.first = first;
        this.second = second;
    }
}
/** @type {Pair<string, number>} */
let p;
"#;
    let diags = check_js(source);
    let bad: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(c, _)| *c == 2304 || *c == 2314)
        .collect();
    assert!(
        bad.is_empty(),
        "expected no TS2304/TS2314 for two @template params, got: {diags:?}",
    );
}

#[test]
fn class_at_template_const_modifier_is_accepted() {
    // `@template const T` uses the same parser path as the arena-direct
    // extractor and must keep T visible for downstream type references.
    let source = r##"
/** @template const T */
class Box {
    /** @param {T} value */
    constructor(value) { this.value = value; }
}
/** @type {Box<"literal">} */
let box;
"##;
    let diags = check_js(source);
    let bad: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(c, _)| *c == 2304 || *c == 2314)
        .collect();
    assert!(
        bad.is_empty(),
        "expected no TS2304/TS2314 for @template const T, got: {diags:?}",
    );
}

#[test]
fn class_at_template_with_export_decl_anchor() {
    // The leading JSDoc on `export class` attaches before the `export`
    // keyword. The arena-direct path must walk up to the EXPORT_DECLARATION
    // wrapper to find it — same adjustment the slow path performs.
    let source = r#"
/** @template U */
export class Holder {
    /** @param {U} v */
    set(v) { this.v = v; }
}
/** @type {Holder<boolean>} */
let h;
"#;
    let diags = check_js(source);
    let bad: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(c, _)| *c == 2304 || *c == 2314)
        .collect();
    assert!(
        bad.is_empty(),
        "expected no TS2304/TS2314 for @template on export class, got: {diags:?}",
    );
}

#[test]
fn exported_interface_no_type_params_resolves_empty() {
    // Cover the exported declaration shape as a regression guard for
    // cross-file arena lookups that see an EXPORT_DECLARATION wrapper.
    let source = r#"
export interface Shape {
    name: string;
}
let shape: Shape = { name: "square" };
"#;
    let diags = check_ts(source);
    assert!(
        diags.is_empty(),
        "expected no diagnostics for exported plain interface, got: {diags:?}",
    );
}
