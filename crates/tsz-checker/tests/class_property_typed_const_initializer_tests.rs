//! Tests for class property type inference when the initializer is an
//! identifier reference to a `const` with an explicit type annotation.
//!
//! Conformance test:
//! `TypeScript/tests/cases/compiler/classPropertyInferenceFromBroaderTypeConst.ts`
//!
//! Before this fix, two stacked bugs combined to widen the property type:
//!   1. Mutable class property initializer types were unconditionally
//!      `widen_literal_type`d, so `const X: 'A' | 'B'` -> `string` after widening.
//!   2. `get_type_of_identifier` returned the flow-narrowed value type
//!      ('A') instead of the declared annotation ('A' | 'B'). Combined,
//!      `class C { D = X; }` produced `D: string` instead of `D: 'A' | 'B'`.
//!
//! Fix:
//!   - Skip widening when the initializer is not a fresh literal expression.
//!   - During class-property-initializer evaluation, set
//!     `use_declared_type_for_identifier`; in `get_type_of_identifier`,
//!     return `declared_type` when the symbol's value declaration has an
//!     explicit type annotation.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
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
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// `D = DEFAULT` where `const DEFAULT: AB = 'A'` should infer `D: AB`,
/// not `D: 'A'` (flow-narrowed) or `D: string` (widened).
#[test]
fn class_property_inherits_typed_const_declared_type() {
    let source = r#"
type AB = 'A' | 'B';
const DEFAULT: AB = 'A';
class C {
    D = DEFAULT;
}
declare const c: C;
declare function expectAB(x: AB): void;
expectAB(c.D);
c.D = 'B';
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "no diagnostics expected — `c.D` should be `AB`, accepting both `expectAB(c.D)` and `c.D = 'B'`. Got: {diags:?}"
    );
}

/// Static class property mirrors the same rule.
#[test]
fn static_class_property_inherits_typed_const_declared_type() {
    let source = r#"
type AB = 'A' | 'B';
const DEFAULT: AB = 'A';
class D {
    static SD = DEFAULT;
}
D.SD = 'B';
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "no diagnostics expected — `D.SD` should be `AB`, accepting `D.SD = 'B'`. Got: {diags:?}"
    );
}

/// Switch on the property in a method body should accept both literal cases.
#[test]
fn class_method_switch_on_property_typed_const_accepts_both_cases() {
    let source = r#"
type AB = 'A' | 'B';
const DEFAULT: AB = 'A';
class C {
    D = DEFAULT;
    method() {
        switch (this.D) {
            case 'A': break;
            case 'B': break;
        }
    }
}
"#;
    let diags = check_strict(source);
    let ts2678: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2678).collect();
    assert!(
        ts2678.is_empty(),
        "TS2678 should not fire on `case 'B'` — `this.D` is `AB`, so both branches are reachable. Got: {diags:?}"
    );
}

/// Sanity: a bare-literal initializer (fresh) still widens. `class C { x = 'a' }`
/// should widen `x` to `string`, so `c.x = 'b'` is allowed (not narrowed to `'a'`).
#[test]
fn class_property_with_fresh_literal_initializer_widens() {
    let source = r#"
class C {
    x = 'a';
}
declare const c: C;
c.x = 'b';
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "fresh literal initializer should widen — `c.x = 'b'` must be allowed. Got: {diags:?}"
    );
}

/// Sanity: readonly class property keeps its literal type even with a typed
/// const initializer; the change does not relax readonly semantics.
#[test]
fn readonly_class_property_typed_const_keeps_declared_type() {
    let source = r#"
type AB = 'A' | 'B';
const DEFAULT: AB = 'A';
class C {
    readonly D = DEFAULT;
}
declare const c: C;
"#;
    let diags = check_strict(source);
    // Readonly path is independent — we only verify no spurious diagnostics.
    let ts2540: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2540).collect();
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2540.is_empty() && ts2322.is_empty(),
        "no readonly-related diagnostics expected on declaration. Got: {diags:?}"
    );
}
