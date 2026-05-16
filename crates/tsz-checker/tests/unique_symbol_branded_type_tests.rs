//! Coverage for issue #6983: branded types with `unique symbol` in property types.
//!
//! Structural rule: when `unique symbol` appears as a property type in any type
//! declaration (type alias, interface, inline type literal), each distinct
//! declaration site creates a fresh nominal `UniqueSymbol` type.  Two such types
//! from different declaration sites are never mutually assignable, even when the
//! property name and overall shape are identical.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn has_error(source: &str) -> bool {
    !check_source(source, "test.ts", CheckerOptions::default()).is_empty()
}

fn has_ts2345(source: &str) -> bool {
    diag_codes(source).contains(&2345)
}

fn has_ts2322(source: &str) -> bool {
    diag_codes(source).contains(&2322)
}

// ── Reported repro: cross-brand assignment via function call ──────────────────

/// Primary repro from issue #6983: passing `OrderId` where `UserId` expected.
#[test]
fn cross_brand_call_reports_ts2345() {
    let source = r#"
type UserId = string & { readonly __brand: unique symbol };
type OrderId = string & { readonly __brand: unique symbol };

function getUserById(id: UserId): void {}

const orderId = "order-123" as OrderId;
getUserById(orderId);
"#;
    assert!(
        has_ts2345(source),
        "passing OrderId to UserId parameter must emit TS2345"
    );
}

/// Reversed direction: passing `UserId` where `OrderId` expected.
#[test]
fn reverse_cross_brand_call_reports_ts2345() {
    let source = r#"
type UserId = string & { readonly __brand: unique symbol };
type OrderId = string & { readonly __brand: unique symbol };

function getOrderById(id: OrderId): void {}

const userId = "user-123" as UserId;
getOrderById(userId);
"#;
    assert!(
        has_ts2345(source),
        "passing UserId to OrderId parameter must emit TS2345"
    );
}

/// Same-type assignment must remain valid.
#[test]
fn same_brand_assignment_is_valid() {
    let source = r#"
type UserId = string & { readonly __brand: unique symbol };

function getUserById(id: UserId): void {}

const userId = "user-123" as UserId;
getUserById(userId);
"#;
    assert!(
        !has_ts2345(source),
        "passing UserId to UserId parameter must not emit TS2345"
    );
}

// ── Rename invariance: different property names ───────────────────────────────

/// Same rule applies regardless of the brand property name.
#[test]
fn cross_brand_different_property_names_reports_error() {
    let source = r#"
type Euros = number & { readonly _euros: unique symbol };
type Dollars = number & { readonly _dollars: unique symbol };

declare function payInEuros(amount: Euros): void;
const dollars = 100 as Dollars;
payInEuros(dollars);
"#;
    assert!(
        has_ts2345(source),
        "cross-brand call with different property names must emit TS2345"
    );
}

// ── Interface declarations ────────────────────────────────────────────────────

/// `unique symbol` in an interface property type creates a fresh unique symbol.
#[test]
fn interface_unique_symbol_property_is_incompatible() {
    let source = r#"
interface BrandA { readonly __brand: unique symbol }
interface BrandB { readonly __brand: unique symbol }

declare const a: BrandA;
const b: BrandB = a;
"#;
    assert!(
        has_ts2322(source),
        "assigning BrandA to BrandB must emit TS2322 (unique symbol property differs)"
    );
}

/// Same interface assigned to itself must remain valid.
#[test]
fn interface_same_brand_is_compatible() {
    let source = r#"
interface BrandA { readonly __brand: unique symbol }

declare const a: BrandA;
const b: BrandA = a;
"#;
    assert!(
        !has_ts2322(source),
        "assigning BrandA to BrandA must not emit TS2322"
    );
}

// ── Unique-symbol widens to symbol ────────────────────────────────────────────

/// A branded type's `unique symbol` property is still assignable to `symbol`.
#[test]
fn unique_symbol_property_assignable_to_symbol() {
    let source = r#"
type Branded = string & { readonly __brand: unique symbol };
declare const b: Branded;
const s: symbol = (b as any).__brand;
"#;
    assert!(
        !has_error(source),
        "unique symbol is a subtype of symbol and must be assignable"
    );
}

// ── Three distinct brands ─────────────────────────────────────────────────────

/// Three different brands from three declarations are mutually incompatible.
#[test]
fn three_brands_mutually_incompatible() {
    let source = r#"
type A = { readonly tag: unique symbol };
type B = { readonly tag: unique symbol };
type C = { readonly tag: unique symbol };

declare const a: A;
declare const b: B;
declare const c: C;

const _ab: B = a;
const _ac: C = a;
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2322),
        "A→B and A→C assignments must emit TS2322; got codes: {codes:?}"
    );
}

// ── Existing const-variable unique symbols must still work ────────────────────

/// const-variable unique symbols still participate in narrowing and typeof.
#[test]
fn const_variable_unique_symbol_still_works() {
    let source = r#"
const sym1: unique symbol = Symbol("a");
const sym2: unique symbol = Symbol("b");

interface Action1 { type: typeof sym1; payload: string }
interface Action2 { type: typeof sym2; payload: number }

function f(action: Action1 | Action2) {
    if (action.type === sym1) {
        const s: string = action.payload;
    } else {
        const n: number = action.payload;
    }
}
"#;
    assert!(
        !has_ts2322(source),
        "const-variable unique symbol narrowing must not regress"
    );
}

/// Cross-type assignment with const-variable unique symbols is still rejected.
#[test]
fn const_variable_unique_symbols_remain_incompatible() {
    let source = r#"
const sym1: unique symbol = Symbol();
const sym2: unique symbol = Symbol();
const x: typeof sym1 = sym2;
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2322),
        "assigning sym2 (typeof sym2) to typeof sym1 must emit TS2322; got codes: {codes:?}"
    );
}
