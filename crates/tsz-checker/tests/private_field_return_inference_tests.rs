//! Regression tests for return-type inference of a method whose body reads a
//! private field (`return this.#field`).
//!
//! A method (or getter) with no return-type annotation infers its return type
//! from the body. When that body reads a private instance field, resolving the
//! `this.#field` self-access must NOT re-enter the declaring class's instance
//! construction (which would re-infer the very method whose signature is in
//! flight and bake a provisional `error`/`any`). These tests pin that the
//! inferred return type is the field's type at the call site, that widening and
//! the negative (undeclared-private) path still behave, and that binder names
//! do not drive the outcome.

use tsz_binder::BinderState;
use tsz_checker::{context::CheckerOptions, diagnostics::Diagnostic, state::CheckerState};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn diagnostics_for(source: &str) -> Vec<Diagnostic> {
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
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    checker.ctx.diagnostics.clone()
}

fn count_code(source: &str, code: u32) -> usize {
    diagnostics_for(source)
        .iter()
        .filter(|d| d.code == code)
        .count()
}

/// The canonical repro: an un-annotated instance private field returned from an
/// un-annotated method infers the field's type (`number`), so assigning the
/// call result to `string` reports TS2322 instead of silently accepting `any`.
#[test]
fn unannotated_private_field_returning_method_infers_field_type() {
    let source = r#"
        class C {
          #x = 1;
          direct() { return this.#x; }
        }
        const c = new C();
        const a: string = c.direct();
    "#;
    assert_eq!(
        count_code(source, 2322),
        1,
        "expected TS2322 (number vs string)"
    );
}

/// The outcome must not depend on the binder-chosen identifiers.
#[test]
fn private_field_returning_method_is_binder_name_independent() {
    let source = r#"
        class Widget {
          #handle = 1;
          read() { return this.#handle; }
        }
        const w = new Widget();
        const s: string = w.read();
    "#;
    assert_eq!(
        count_code(source, 2322),
        1,
        "expected TS2322 with renamed binders"
    );
}

/// An explicitly annotated private field also infers correctly (the annotation
/// path and the initializer path both resolve without re-entering construction).
#[test]
fn annotated_private_field_returning_method_infers_field_type() {
    let source = r#"
        class C {
          #x: number = 1;
          read() { return this.#x; }
        }
        const c = new C();
        const s: string = c.read();
    "#;
    assert_eq!(count_code(source, 2322), 1);
}

/// Indirect return forms (getter, parenthesized, ternary, via-local) resolve
/// the same field type — none should collapse to `any`.
#[test]
fn indirect_private_field_return_forms_infer_field_type() {
    for body in [
        "get val() { return this.#x; }",
        "read() { return (this.#x); }",
        "read(b: boolean) { return b ? this.#x : this.#x; }",
        "read() { const v = this.#x; return v; }",
    ] {
        let accessor = if body.starts_with("get ") {
            "const s: string = c.val;"
        } else if body.contains("b: boolean") {
            "const s: string = c.read(true);"
        } else {
            "const s: string = c.read();"
        };
        let source = format!("class C {{ #x = 1; {body} }}\nconst c = new C();\n{accessor}");
        assert_eq!(
            count_code(&source, 2322),
            1,
            "expected TS2322 for body: {body}"
        );
    }
}

/// A fresh literal field widens in the inferred return type just as it does in
/// the field's own declared type (`#s = \"hi\"` -> `string`).
#[test]
fn private_field_return_widens_like_the_field_declaration() {
    let source = r#"
        class C {
          #s = "hi";
          read() { return this.#s; }
        }
        const c = new C();
        const n: number = c.read();
    "#;
    // `read()` is `string`, so assigning to `number` is the mismatch.
    assert_eq!(count_code(source, 2322), 1);
}

/// Negative case: reading a private name that is NOT declared in the class must
/// still report TS2339 — the fix must not turn every private read compatible.
#[test]
fn undeclared_private_read_in_returning_method_still_reports_ts2339() {
    let source = r#"
        class C {
          #a = 1;
          read() { return this.#b; }
        }
    "#;
    assert_eq!(
        count_code(source, 2339),
        1,
        "expected TS2339 for undeclared #b"
    );
    // And no spurious extra property errors.
    assert_eq!(count_code(source, 2322), 0);
}

/// Control: a static private field returned from a static method already worked
/// and must keep working (the fix is scoped to instance self-access).
#[test]
fn static_private_field_returning_method_infers_field_type() {
    let source = r#"
        class C {
          static #s = 1;
          static read() { return C.#s; }
        }
        const a: string = C.read();
    "#;
    assert_eq!(count_code(source, 2322), 1);
}

/// Control: a public field returned from a method is unaffected.
#[test]
fn public_field_returning_method_infers_field_type() {
    let source = r#"
        class C {
          x = 1;
          read() { return this.x; }
        }
        const c = new C();
        const s: string = c.read();
    "#;
    assert_eq!(count_code(source, 2322), 1);
}
