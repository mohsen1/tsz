//! Covariant inference candidates branded by DIFFERENT enums never union.
//!
//! tsc's `getCommonSupertype` unions unit candidates only when
//! `literalTypesWithSameBaseType` holds — every candidate shares one base
//! type (`getBaseTypeOfLiteralType` maps an enum member to its parent enum).
//! Members of different enums fail that test, so control reaches the
//! `reduceLeft` leftmost-wins fallback and the later conflicting argument
//! reports `TS2345`. tsz previously unioned such candidates (`T = E1.X |
//! E2.X` accepted both arguments — the missing error on
//! `typeArgumentInferenceWithObjectLiteral.ts` line 37), and when the
//! candidates carried different priorities (a source-function-return
//! candidate is recorded at `ReturnType`, a naked argument at
//! `NakedTypeVariable`) the priority filter could fix `T` to a LATER
//! argument's candidate and invert the reported mismatch onto the first
//! argument.
//!
//! Every case is oracle-pinned against `typescript@6.0.2` (default options).
//!
//! Harness fidelity: these route through the shared-`DefinitionStore` checker
//! construction (`check_source_with_libs_shared_def_store`) because the fix
//! reads the enum member -> parent edges through the `QueryCache`'s attached
//! store, exactly as the production driver wires it; the plain bare-interner
//! harness has no store and cannot observe the first-wins behavior.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_with_libs_shared_def_store, diagnostic_code_messages, load_default_lib_files,
};

fn code_messages(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    diagnostic_code_messages(check_source_with_libs_shared_def_store(
        source,
        "main.ts",
        CheckerOptions::default(),
        &libs,
    ))
}

fn messages_with_code(source: &str, code: u32) -> Vec<String> {
    code_messages(source)
        .into_iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, m)| m)
        .collect()
}

fn assert_clean(source: &str, label: &str) {
    let diags = code_messages(source);
    assert!(
        diags.is_empty(),
        "{label}: expected no diagnostics, got {diags:?}"
    );
}

/// The conformance witness shape: an object-literal argument contributes a
/// function-return candidate, the second argument a naked candidate. tsc
/// widens the leftmost member to its parent enum and rejects the second
/// argument.
#[test]
fn object_return_candidate_plus_naked_conflicting_enum_reports_ts2345() {
    // oracle: TS2345 Argument of type 'Second' is not assignable to
    // parameter of type 'First'.
    let source = r#"
enum First { M }
enum Second { M }
declare function pick<T, U>(a: { w: (x: T) => U; r: () => T; }, b: T): U;
var out = pick({ w: x => x, r: () => First.M }, Second.M);
"#;
    assert_eq!(
        messages_with_code(source, 2345),
        vec![
            "Argument of type 'Second' is not assignable to parameter of type 'First'.".to_string()
        ],
        "leftmost enum candidate must win and the later argument must report TS2345"
    );
    assert_eq!(
        messages_with_code(source, 2322),
        Vec::<String>::new(),
        "the mismatch must not be inverted onto the object literal's return"
    );
}

/// Multi-member enums pin the widening half: the parameter renders as the
/// parent enum (`Alpha`), not the member (`Alpha.A`).
#[test]
fn object_return_candidate_widens_leftmost_member_to_parent_enum() {
    // oracle: TS2345 ... parameter of type 'Alpha'.
    let source = r#"
enum Alpha { A, B }
enum Beta { A }
declare function pick<T, U>(a: { w: (x: T) => U; r: () => T; }, b: T): U;
var out = pick({ w: x => x, r: () => Alpha.A }, Beta.A);
"#;
    assert_eq!(
        messages_with_code(source, 2345),
        vec!["Argument of type 'Beta' is not assignable to parameter of type 'Alpha'.".to_string()],
    );
}

/// A bare callback argument (no object wrapper) plus a naked argument: same
/// rule, and the historical failure mode here was the inverted TS2322 on the
/// callback return.
#[test]
fn bare_callback_return_candidate_plus_naked_conflicting_enum_reports_ts2345() {
    // oracle: TS2345 Argument of type 'Right' is not assignable to
    // parameter of type 'Left'.
    let source = r#"
enum Left { V, W }
enum Right { V }
declare function keep<T>(a: () => T, b: T): T;
var out = keep(() => Left.V, Right.V);
"#;
    assert_eq!(
        messages_with_code(source, 2345),
        vec!["Argument of type 'Right' is not assignable to parameter of type 'Left'.".to_string()],
    );
    assert_eq!(messages_with_code(source, 2322), Vec::<String>::new());
}

/// Two naked enum-member arguments: leftmost wins in source order.
#[test]
fn two_naked_conflicting_enum_members_fix_t_to_the_leftmost() {
    // oracle: TS2345 Argument of type 'Deux' is not assignable to
    // parameter of type 'Un'.
    let source = r#"
enum Un { Z }
enum Deux { Z }
declare function both<T>(a: T, b: T): T;
both(Un.Z, Deux.Z);
"#;
    assert_eq!(
        messages_with_code(source, 2345),
        vec!["Argument of type 'Deux' is not assignable to parameter of type 'Un'.".to_string()],
    );
}

/// Order sensitivity: swapping the arguments swaps which one is rejected.
/// With the type parameter at top level in the return type, tsc does NOT
/// widen the leftmost member, so a multi-member enum renders qualified.
#[test]
fn two_naked_conflicting_enum_members_reversed_order_rejects_the_second() {
    // oracle: TS2345 Argument of type 'Multi.P' is not assignable to
    // parameter of type 'Solo'.
    let source = r#"
enum Multi { P, Q }
enum Solo { P }
declare function both<T>(a: T, b: T): T;
both(Solo.P, Multi.P);
"#;
    assert_eq!(
        messages_with_code(source, 2345),
        vec![
            "Argument of type 'Multi.P' is not assignable to parameter of type 'Solo'.".to_string()
        ],
    );
}

/// Negative case: members of ONE enum share a base, so they union exactly as
/// before (tsc's `literalTypesWithSameBaseType` path) — no diagnostic, for
/// fresh member expressions and annotated member types alike.
#[test]
fn same_enum_members_still_union_without_diagnostics() {
    assert_clean(
        r#"
enum Only { A, B }
declare function both<T>(a: T, b: T): T;
both(Only.A, Only.B);
"#,
        "fresh members of one enum",
    );
    assert_clean(
        r#"
enum Only { A, B }
declare function both<T>(a: T, b: T): T;
declare const first: Only.A;
declare const second: Only.B;
both(first, second);
"#,
        "annotated members of one enum",
    );
}

/// Positive/fallback case: the object-wrapped shape with a SINGLE enum stays
/// clean (`T` resolves to that enum).
#[test]
fn object_return_candidate_same_enum_naked_argument_is_clean() {
    assert_clean(
        r#"
enum Lone { K }
declare function pick<T, U>(a: { w: (x: T) => U; r: () => T; }, b: T): U;
var out = pick({ w: x => x, r: () => Lone.K }, Lone.K);
"#,
        "same enum through both positions",
    );
}

/// A string enum against a numeric enum is still a cross-enum conflict.
#[test]
fn string_enum_vs_numeric_enum_first_wins() {
    // oracle: TS2345 Argument of type 'Str' is not assignable to
    // parameter of type 'Num.N'.
    let source = r#"
enum Num { N, O }
enum Str { S = "s" }
declare function both<T>(a: T, b: T): T;
both(Num.N, Str.S);
"#;
    assert_eq!(
        messages_with_code(source, 2345),
        vec!["Argument of type 'Str' is not assignable to parameter of type 'Num.N'.".to_string()],
    );
}

/// Rest-parameter arguments keep the unwidened leftmost member (tsc reports
/// the parameter as the member when the type parameter is top-level in the
/// return type).
#[test]
fn rest_parameter_conflicting_enum_members_first_wins() {
    // oracle: TS2345 Argument of type 'Tail' is not assignable to
    // parameter of type 'Head.H'.
    let source = r#"
enum Head { H, I }
enum Tail { H }
declare function gather<T>(...items: T[]): T;
gather(Head.H, Tail.H);
"#;
    assert_eq!(
        messages_with_code(source, 2345),
        vec![
            "Argument of type 'Tail' is not assignable to parameter of type 'Head.H'.".to_string()
        ],
    );
}

/// Object-property provenance is excluded from first-wins: tsc infers these
/// order-independently, and the anchored TS2322 on the conflicting property
/// must survive in both source orders.
#[test]
fn object_property_enum_candidates_keep_property_anchored_ts2322() {
    for source in [
        r#"
enum Gauche { G }
enum Droite { G }
declare function shape<T>(o: { bar: T, baz: T }): T;
shape({ bar: Gauche.G, baz: Droite.G });
"#,
        r#"
enum Gauche { G }
enum Droite { G }
declare function shape<T>(o: { bar: T, baz: T }): T;
shape({ baz: Droite.G, bar: Gauche.G });
"#,
    ] {
        // oracle (both orders): TS2322 Type 'Droite' is not assignable to
        // type 'Gauche'.
        assert_eq!(
            messages_with_code(source, 2322),
            vec!["Type 'Droite' is not assignable to type 'Gauche'.".to_string()],
            "object-property candidates must stay on the order-independent path"
        );
        assert_eq!(messages_with_code(source, 2345), Vec::<String>::new());
    }
}

/// The primitive analog of the mixed-priority shape: the function-return
/// candidate `0` wins over the later naked `string` argument, and the
/// mismatch is NOT inverted onto the object literal.
#[test]
fn object_return_primitive_candidate_plus_naked_string_reports_ts2345() {
    // oracle: TS2345 Argument of type 'string' is not assignable to
    // parameter of type 'number'.
    let source = r#"
declare function pick<T, U>(a: { w: (x: T) => U; r: () => T; }, b: T): U;
declare const s: string;
var out = pick({ w: x => x, r: () => 0 }, s);
"#;
    assert_eq!(
        messages_with_code(source, 2345),
        vec![
            "Argument of type 'string' is not assignable to parameter of type 'number'."
                .to_string()
        ],
    );
    assert_eq!(messages_with_code(source, 2322), Vec::<String>::new());
}
