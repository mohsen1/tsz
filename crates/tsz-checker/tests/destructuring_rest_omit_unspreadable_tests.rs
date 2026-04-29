//! Locks tsc-parity for destructuring-rest `Omit<T, K>` construction
//! when the source is a generic type parameter constrained by a class
//! that has non-spreadable prototype members (methods, getters, setters).
//!
//! Regression target: `destructuringUnspreadableIntoRest.ts`.
//!
//! tsc's `getSpreadType` excludes prototype properties (methods + accessors)
//! from the rest type. For a generic source `<T extends A>`, this is rendered
//! as `Omit<T, "method" | "getter" | "setter" | <explicit destructured>>`.
//! The K order is methods first, then accessors in source declaration order.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_common::checker_options::CheckerOptions;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn checker_diagnostics(source: &str) -> Vec<(u32, String)> {
    let file_name = "test.ts";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions::default();

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn ts2339_messages(diags: &[(u32, String)]) -> Vec<String> {
    diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .map(|(_, m)| m.clone())
        .collect()
}

#[test]
fn rest_from_type_parameter_omits_prototype_method_names_with_no_explicit_excludes() {
    // <T extends A> with `const { ...rest } = x` — even with NO explicit
    // destructured property, K should include the public prototype member
    // names. tsc renders this as `Omit<T, "method" | "getter" | "setter">`.
    let source = r#"
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type Pick<T, K extends keyof T> = { [P in K]: T[P]; };
type Exclude<T, U> = T extends U ? never : T;

class A {
    publicProp: string;
    get getter(): number { return 1; }
    set setter(_v: number) {}
    method(): void {}
}

function destructure<T extends A>(x: T) {
    const { ...rest } = x;
    // Accessing a prototype member should fail — those are excluded from
    // the rest type via Omit's K. The diagnostic message must render the
    // rest type as `Omit<T, "method" | "getter" | "setter">`.
    rest.method;
}
"#;
    let diags = checker_diagnostics(source);
    let msgs = ts2339_messages(&diags);
    // The exact tsc fingerprint: `Omit<T, "method" | "getter" | "setter">`
    // (methods first, then accessors in source order).
    assert!(
        msgs.iter().any(|m| {
            m.contains("Omit<T,")
                && m.contains("\"method\"")
                && m.contains("\"getter\"")
                && m.contains("\"setter\"")
        }),
        "Expected Omit<T, \"method\" | \"getter\" | \"setter\">. Got: {msgs:?}"
    );
}

#[test]
fn rest_from_type_parameter_combines_explicit_excludes_with_prototype_names() {
    // <T extends A> with `const { publicProp: _, ...rest } = x` — K should
    // include both the explicit `publicProp` and the prototype member names.
    let source = r#"
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type Pick<T, K extends keyof T> = { [P in K]: T[P]; };
type Exclude<T, U> = T extends U ? never : T;

class A {
    publicProp: string;
    get getter(): number { return 1; }
    set setter(_v: number) {}
    method(): void {}
}

function destructure<T extends A>(x: T) {
    const { publicProp: _, ...rest } = x;
    rest.publicProp;
}
"#;
    let diags = checker_diagnostics(source);
    let msgs = ts2339_messages(&diags);
    assert!(
        msgs.iter().any(|m| {
            m.contains("Omit<T,")
                && m.contains("\"method\"")
                && m.contains("\"getter\"")
                && m.contains("\"setter\"")
                && m.contains("\"publicProp\"")
        }),
        "Expected Omit<T, ...> to include all of method/getter/setter/publicProp. Got: {msgs:?}"
    );
}

#[test]
fn rest_from_type_parameter_with_no_class_prototype_members_returns_t_unchanged() {
    // For a constraint with no public prototype members, the type-param
    // branch should NOT wrap T in Omit — preserve T's identity as before.
    let source = r#"
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type Pick<T, K extends keyof T> = { [P in K]: T[P]; };
type Exclude<T, U> = T extends U ? never : T;

interface I { a: string; b: number; }

function destructure<T extends I>(x: T) {
    const { ...rest } = x;
    // No diagnostic expected — `rest` retains T's full surface.
    rest.a;
    rest.b;
}
"#;
    let diags = checker_diagnostics(source);
    let msgs = ts2339_messages(&diags);
    assert!(
        msgs.is_empty(),
        "No TS2339 expected when constraint has no prototype members. Got: {msgs:?}"
    );
}
