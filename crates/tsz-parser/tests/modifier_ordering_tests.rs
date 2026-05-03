//! Tests for class member modifier ordering (TS1029) and ambient context (TS1040).

use crate::parser::ParserState;

fn parse_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect()
}

fn has_error(source: &str, code: u32) -> bool {
    parse_diagnostics(source).iter().any(|(c, _)| *c == code)
}

fn count_error(source: &str, code: u32) -> usize {
    parse_diagnostics(source)
        .iter()
        .filter(|(c, _)| *c == code)
        .count()
}

// =========================================================================
// TS1029: Modifier ordering — override vs readonly
// =========================================================================

#[test]
fn override_readonly_correct_order_no_ts1029() {
    // `override readonly` is the canonical order in tsc — no TS1029
    let source = r"
class B { p: number = 1; }
class D extends B {
    override readonly p: number;
}
";
    assert!(
        !has_error(source, 1029),
        "`override readonly` should not produce TS1029 — this is the correct order"
    );
}

#[test]
fn readonly_override_wrong_order_ts1029() {
    // `readonly override` is wrong order — should produce TS1029
    let source = r"
class B { p: number = 1; }
class D extends B {
    readonly override p: number;
}
";
    assert!(
        has_error(source, 1029),
        "`readonly override` should produce TS1029 — override must precede readonly"
    );
}

#[test]
fn override_async_correct_order_no_ts1029() {
    let source = r"
class B { m(): void {} }
class D extends B {
    override async m() {}
}
";
    assert!(
        !has_error(source, 1029),
        "`override async` should not produce TS1029"
    );
}

#[test]
fn async_override_wrong_order_ts1029() {
    let source = r"
class B { m(): void {} }
class D extends B {
    async override m() {}
}
";
    assert!(
        has_error(source, 1029),
        "`async override` should produce TS1029"
    );
}

// =========================================================================
// TS1040: override in ambient context (declare)
// =========================================================================

#[test]
fn override_declare_produces_ts1040() {
    // `override declare` on a member property → TS1040
    let source = r"
class B { p: number = 1; }
class D extends B {
    override declare p: number;
}
";
    assert!(
        has_error(source, 1040),
        "`override declare` should produce TS1040 — override cannot be in ambient context"
    );
}

#[test]
fn declare_override_produces_ts1040() {
    // Reverse order `declare override` also → TS1040
    let source = r"
class B { p: number = 1; }
class D extends B {
    declare override p: number;
}
";
    assert!(
        has_error(source, 1040),
        "`declare override` should produce TS1040 — override cannot be in ambient context"
    );
}

#[test]
fn override_without_declare_no_ts1040() {
    // Plain override should not produce TS1040
    let source = r"
class B { p: number = 1; }
class D extends B {
    override p: number;
}
";
    assert!(
        !has_error(source, 1040),
        "plain `override` should not produce TS1040"
    );
}

#[test]
fn declare_without_override_no_ts1040() {
    // Plain declare should not produce TS1040
    let source = r"
class B { p: number = 1; }
class D extends B {
    declare p: number;
}
";
    assert!(
        !has_error(source, 1040),
        "plain `declare` should not produce TS1040"
    );
}

#[test]
fn override_declare_exactly_one_ts1040() {
    // Should emit exactly one TS1040, not two
    let source = r"
class B { p: number = 1; }
class D extends B {
    override declare p: number;
}
";
    assert_eq!(
        count_error(source, 1040),
        1,
        "`override declare` should produce exactly one TS1040"
    );
}

// =========================================================================
// TS1029 / TS1030: Variance modifier ordering on type parameters.
//
// `in` must precede `out`; both can appear at most once.
// These rules must be enforced regardless of which user-chosen identifier
// follows them, so each test exercises a different name to ensure the parser
// is matching token kinds, not name strings.
// =========================================================================

#[test]
fn type_param_in_out_correct_order_no_diag() {
    let source = "type A<in out U> = (x: U) => U;";
    assert!(!has_error(source, 1029));
    assert!(!has_error(source, 1030));
}

#[test]
fn type_param_out_in_emits_ts1029() {
    // `out in` reverses the canonical order: `in` must precede `out`.
    let source = "type A<out in V> = V;";
    assert!(
        has_error(source, 1029),
        "`out in` should emit TS1029 at the second modifier"
    );
}

#[test]
fn type_param_duplicate_in_emits_ts1030() {
    // The second `in` is a duplicate, reported as "modifier already seen".
    let source = "type A<in out in W> = W;";
    assert_eq!(count_error(source, 1030), 1);
}

#[test]
fn type_param_duplicate_out_emits_ts1030() {
    let source = "type A<in out out X> = X;";
    assert_eq!(count_error(source, 1030), 1);
}

#[test]
fn type_param_modifier_diag_independent_of_param_name() {
    // The same diagnostic must fire whichever identifier the user picks for
    // the type parameter — confirms the parser keys off token kind, not name.
    for name in ["T", "K", "P", "Foo", "_"] {
        let source = format!("type A<out in {name}> = {name};");
        assert!(
            has_error(&source, 1029),
            "`out in {name}` should emit TS1029"
        );
    }
}

// =========================================================================
// TS1275: 'accessor' modifier can only appear on a property declaration
// TS1276: An 'accessor' property cannot be declared optional
// =========================================================================

#[test]
fn accessor_on_method_emits_ts1275() {
    let source = r"
class C {
    accessor m() {}
}
";
    assert_eq!(count_error(source, 1275), 1);
}

#[test]
fn accessor_on_get_accessor_emits_ts1275() {
    let source = r"
class C {
    accessor get x() { return 1; }
}
";
    assert_eq!(count_error(source, 1275), 1);
}

#[test]
fn accessor_on_set_accessor_emits_ts1275() {
    let source = r"
class C {
    accessor set x(v: any) {}
}
";
    assert_eq!(count_error(source, 1275), 1);
}

#[test]
fn accessor_on_constructor_emits_ts1275() {
    let source = r"
class C {
    accessor constructor() {}
}
";
    assert_eq!(count_error(source, 1275), 1);
}

#[test]
fn accessor_on_top_level_class_emits_ts1275() {
    let source = "accessor class C3 {}";
    assert_eq!(count_error(source, 1275), 1);
}

#[test]
fn accessor_on_top_level_var_emits_ts1275() {
    let source = "accessor var V1: any;";
    assert_eq!(count_error(source, 1275), 1);
}

#[test]
fn accessor_on_top_level_function_emits_ts1275() {
    let source = "accessor function F1() {}";
    assert_eq!(count_error(source, 1275), 1);
}

#[test]
fn accessor_on_top_level_import_emits_ts1275() {
    let source = "accessor import \"x\";";
    assert_eq!(count_error(source, 1275), 1);
}

#[test]
fn accessor_optional_property_emits_ts1276() {
    let source = r"
class C {
    accessor p?: any;
}
";
    assert_eq!(count_error(source, 1276), 1);
}

#[test]
fn accessor_required_property_no_ts1276() {
    let source = r"
class C {
    accessor p: any;
}
";
    assert!(!has_error(source, 1276));
    assert!(!has_error(source, 1275));
}

#[test]
fn accessor_property_keys_off_token_kind_not_member_name() {
    // The same diagnostic must fire whichever identifier the user picks for
    // the auto-accessor property — confirms the parser keys off token kind.
    for name in ["a", "myProp", "_x", "$_"] {
        let optional_src = format!("class C {{ accessor {name}?: any; }}");
        assert_eq!(
            count_error(&optional_src, 1276),
            1,
            "optional accessor property `{name}?: any` should emit TS1276"
        );

        let method_src = format!("class C {{ accessor {name}() {{}} }}");
        assert_eq!(
            count_error(&method_src, 1275),
            1,
            "accessor on method `{name}()` should emit TS1275"
        );
    }
}
