//! TS2339 must fire on property access through an `IndexAccess` whose
//! evaluated receiver is a closed union, even though the display form
//! retains the `E[K]` shape.
//!
//! Pre-fix: `diagnostic_display_type_for_missing_property` returned the
//! `IndexAccess` `narrowed` form when the apparent type was a union, then
//! `error_property_not_exist_at` blanket-suppressed all `IndexAccess` types,
//! silencing the diagnostic on a genuine missing-property situation.
//!
//! Mirrors the `quickinfoTypeAtReturnPositionsInaccurate.ts` conformance
//! reproducer.

use tsz_checker::CheckerState;
use tsz_common::checker_options::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check(source: &str) -> Vec<Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let opts = CheckerOptions::default();
    let interner = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &interner,
        "test.ts".to_string(),
        opts,
    );
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// `class Store<E extends { [k: string]: A | B }>` with a method that
/// reads `this.entries[k]: E[K]`. Property access on the resolved union
/// `A | B` should emit `TS2339` for properties absent on every member.
#[test]
fn ts2339_fires_on_indexed_access_resolving_to_closed_union_missing_property() {
    let source = r#"
class A { foo(): void {} }
class B { bar(): void {} }
class Store<E extends { [k: string]: A | B }> {
  private entries = {} as E;
  get<K extends keyof E>(k: K) {
    let entry = this.entries[k];
    entry.notexist();
  }
}
"#;
    let diags = check(source);
    assert!(
        diags.iter().any(|d| {
            d.code == 2339
                && d.message_text.contains("notexist")
                && d.message_text.contains("type 'A | B'")
        }),
        "Expected TS2339 for `entry.notexist()` on `A | B` (resolved E[K]), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Bare type-parameter receivers — no `IndexAccess` wrapping — should keep
/// emitting TS2339 as before. Pin the unrelated branch.
#[test]
fn ts2339_fires_on_bare_type_parameter_constrained_to_union() {
    let source = r#"
class A { foo(): void {} }
class B { bar(): void {} }
function f<T extends A | B>(x: T) {
  x.notexist();
}
"#;
    let diags = check(source);
    assert!(
        diags
            .iter()
            .any(|d| { d.code == 2339 && d.message_text.contains("notexist") }),
        "Expected TS2339 on bare T extends A|B receiver, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
