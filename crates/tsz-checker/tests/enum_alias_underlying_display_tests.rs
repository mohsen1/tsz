//! Display fences for type aliases whose body is a bare enum or enum-member
//! reference.
//!
//! tsc attaches an `aliasSymbol` only to freshly-constructed types. A bare
//! reference to an enum or an enum member resolves to the declaration's shared
//! nominal type, so the alias name never survives into diagnostics: tsc renders
//! `Mode.A` for a member, `Mode` for the enum itself, and the bare enum name
//! for a single-member enum's member. Every expectation here is oracle-pinned
//! against `tsc` (6.0.2) on the same source.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        strict_function_types: true,
        ..CheckerOptions::default()
    }
}

fn expect_ts2322(source: &str, expected: &str) {
    let diagnostics = check_source(source, "test.ts", strict_options());
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected a TS2322 diagnostic");
    assert!(
        diag.message_text.contains(expected),
        "expected message containing {expected:?}, got: {diag:?}"
    );
}

#[test]
fn enum_member_alias_source_renders_member_not_alias() {
    expect_ts2322(
        r#"
enum Mode { A, B }
type MA = Mode.A;
declare const ma: MA;
const t: Mode.B = ma;
"#,
        "Type 'Mode.A' is not assignable to type 'Mode.B'.",
    );
}

#[test]
fn single_member_enum_member_alias_renders_bare_enum_name() {
    // A single-member enum's member type IS the enum type in tsc, so the
    // member alias renders the bare enum name, never `One.Only`.
    expect_ts2322(
        r#"
enum One { Only }
type MO = One.Only;
declare const mo: MO;
const s: string = mo;
"#,
        "Type 'One' is not assignable to type 'string'.",
    );
}

#[test]
fn alias_chain_to_enum_member_renders_member() {
    expect_ts2322(
        r#"
enum Mode { A, B }
type MA = Mode.A;
type MB = MA;
declare const mb: MB;
const t: Mode.B = mb;
"#,
        "Type 'Mode.A' is not assignable to type 'Mode.B'.",
    );
}

#[test]
fn whole_enum_alias_renders_enum_name() {
    expect_ts2322(
        r#"
enum Mode { A, B }
type M = Mode;
declare const m: M;
const s: string = m;
"#,
        "Type 'Mode' is not assignable to type 'string'.",
    );
}

#[test]
fn string_enum_member_alias_renders_member() {
    expect_ts2322(
        r#"
enum Color { Red = "red", Blue = "blue" }
type CR = Color.Red;
declare const cr: CR;
const t: Color.Blue = cr;
"#,
        "Type 'Color.Red' is not assignable to type 'Color.Blue'.",
    );
}

#[test]
fn renamed_binders_still_render_member() {
    expect_ts2322(
        r#"
enum Zq9 { Kx, Vy }
type Wq = Zq9.Kx;
declare const wq: Wq;
const t: Zq9.Vy = wq;
"#,
        "Type 'Zq9.Kx' is not assignable to type 'Zq9.Vy'.",
    );
}

#[test]
fn const_enum_member_alias_renders_member() {
    expect_ts2322(
        r#"
const enum CE { P, Q }
type CP = CE.P;
declare const cp: CP;
const t: CE.Q = cp;
"#,
        "Type 'CE.P' is not assignable to type 'CE.Q'.",
    );
}

#[test]
fn target_position_enum_member_alias_renders_member() {
    expect_ts2322(
        r#"
enum Mode { A, B }
type MB = Mode.B;
declare const m: Mode.A;
const t: MB = m;
"#,
        "Type 'Mode.A' is not assignable to type 'Mode.B'.",
    );
}

#[test]
fn ts2345_argument_position_enum_member_alias_renders_member() {
    let diagnostics = check_source(
        r#"
enum Mode { A, B }
type MA = Mode.A;
declare const ma: MA;
declare function take(x: Mode.B): void;
take(ma);
"#,
        "test.ts",
        strict_options(),
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2345)
        .expect("expected a TS2345 diagnostic");
    assert!(
        diag.message_text
            .contains("Argument of type 'Mode.A' is not assignable to parameter of type 'Mode.B'."),
        "expected member display in argument position, got: {diag:?}"
    );
}

#[test]
fn object_alias_negative_control_keeps_alias_name() {
    // An alias of an object type literal is a freshly-constructed structural
    // type: tsc stamps the alias symbol, so `Obj` must keep its name while the
    // nested member mismatch still renders the member.
    let diagnostics = check_source(
        r#"
enum Mode { A, B }
type Obj = { m: Mode.A };
declare const o: Obj;
const t: { m: Mode.B } = o;
"#,
        "test.ts",
        strict_options(),
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected a TS2322 diagnostic");
    assert!(
        diag.message_text
            .contains("Type 'Obj' is not assignable to type '{ m: Mode.B; }'."),
        "object alias must keep its name, got: {diag:?}"
    );
    assert!(
        diag.related_information.iter().any(|related| {
            related
                .message_text
                .contains("Type 'Mode.A' is not assignable to type 'Mode.B'.")
        }),
        "nested member mismatch must render the member, got: {diag:?}"
    );
}
