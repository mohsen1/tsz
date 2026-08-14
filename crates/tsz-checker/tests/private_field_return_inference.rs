//! Regression tests for #17430: a method whose body is exactly
//! `return this.#privateField` must infer the field's type, not `any`.
//!
//! During class-instance construction the class-statement checker drops the
//! `class_instance_type_cache` entry so member bodies observe the checked
//! shape. Resolving `this.#field` in an un-annotated method body then requests
//! a fresh instance build while that body is on the resolution stack; rebuilding
//! re-infers the same method from its transient cycle-guard `ERROR` placeholder
//! and bakes `any` into the method signature. The receiver type is fine; only
//! the (re-entrant) rebuild is wrong, so the fix reuses the already-registered
//! instance type instead of rebuilding.
//!
//! Binder names are varied deliberately (the fix must not key off any
//! identifier): the class, field, and method names differ across cases.

use tsz_binder::BinderState;
use tsz_checker::{context::CheckerOptions, diagnostics::Diagnostic, state::CheckerState};
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn collect_diagnostics(source: &str) -> Vec<Diagnostic> {
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
            target: ScriptTarget::ES2022,
            strict: true,
            ..Default::default()
        },
    );
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn ts2322_count(source: &str) -> usize {
    collect_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2322)
        .count()
}

/// The reported witness: a bare `return this.#field` method infers the field
/// type, so assigning its result to an incompatible annotation is TS2322.
#[test]
fn bare_private_field_return_infers_field_type() {
    let src = r#"
class Widget {
  #count = 1;
  direct() { return this.#count; }
}
const w = new Widget();
const s: string = w.direct();
"#;
    assert_eq!(
        ts2322_count(src),
        1,
        "expected TS2322 for `string = number-returning method`"
    );
}

/// Every return shape from the issue's adjacent matrix must agree with the
/// bare form and infer the field type. Each form is checked in its OWN
/// single-member class: a combined multi-member class is subject to
/// construction-order artifacts (#16309), so the isolated form is the
/// authoritative witness. The un-annotated getter is included because its
/// return type is body-inferred exactly like a method's — the same re-entrant
/// rebuild would otherwise poison it.
#[test]
fn private_field_return_forms_each_infer_field_type_in_isolation() {
    // (member declaration, expression used at the `const s: string = ...` site)
    let forms: &[(&str, &str)] = &[
        ("direct() { return this.#total; }", "g.direct()"),
        ("paren() { return (this.#total); }", "g.paren()"),
        (
            "cond(flag: boolean) { return flag ? this.#total : this.#total; }",
            "g.cond(true)",
        ),
        (
            "viaLocal() { const v = this.#total; return v; }",
            "g.viaLocal()",
        ),
        ("get acc() { return this.#total; }", "g.acc"),
        (
            "annotated(): number { return this.#total; }",
            "g.annotated()",
        ),
    ];
    for (member, expr) in forms {
        let src = format!(
            "class Gadget {{\n  #total = 1;\n  {member}\n}}\nconst g = new Gadget();\nconst s: string = {expr};\n"
        );
        assert_eq!(
            ts2322_count(&src),
            1,
            "form `{member}` must infer `number` (one TS2322), got a different count"
        );
    }
}

/// A static private member returned from a static method resolves the same way.
#[test]
fn static_private_field_return_infers_field_type() {
    let src = r#"
class Vault {
  static #secret = 1;
  static reveal() { return Vault.#secret; }
}
const s: string = Vault.reveal();
"#;
    assert_eq!(ts2322_count(src), 1, "static private return infers number");
}

/// A public field in the same bare-return shape was never affected; keep it
/// covered so the private path stays aligned with the public one.
#[test]
fn bare_public_field_return_infers_field_type() {
    let src = r#"
class Panel {
  value = 1;
  read() { return this.value; }
}
const p = new Panel();
const s: string = p.read();
"#;
    assert_eq!(
        ts2322_count(src),
        1,
        "public field bare return infers number"
    );
}

/// Negative: a genuinely absent private member must still be rejected — the fix
/// resolves declared fields, it does not fabricate members.
#[test]
fn absent_private_member_still_errors() {
    let src = r#"
class Store {
  #real = 1;
  broken() { return this.#missing; }
}
"#;
    let diags = collect_diagnostics(src);
    assert!(
        diags.iter().any(|d| d.code == 2339),
        "expected TS2339 for an undeclared private member, got: {diags:#?}"
    );
}

/// Two private-returning methods in one class must each infer their own field
/// type independently (guards against a single-shot / order artifact).
#[test]
fn multiple_private_returning_methods_are_independent() {
    let src = r#"
class Ledger {
  #amount = 1;
  #label = "x";
  amount() { return this.#amount; }
  label() { return this.#label; }
}
const l = new Ledger();
const a: string = l.amount();
const b: number = l.label();
"#;
    assert_eq!(
        ts2322_count(src),
        2,
        "amount() is number and label() is string; both mismatches fire"
    );
}
