//! Tests for class member modifier ordering (TS1029) and ambient context (TS1040).

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;
use tsz_scanner::SyntaxKind;

fn parse_diagnostics(source: &str) -> Vec<(u32, String)> {
    let (parser, _root) = parse_source(source);
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

#[test]
fn abstract_static_illegal_pair_does_not_emit_ts1029() {
    let source = "abstract class A { abstract static x: number }\n";
    let diagnostics = parse_diagnostics(source);
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 1029 && message.contains("'static' modifier must precede 'abstract'")
        }),
        "`abstract static` should be left to TS1243 without an extra TS1029: {diagnostics:?}"
    );
}

#[test]
fn readonly_accessor_emits_ts1243_without_ts1029() {
    let source = "class C { readonly accessor id: number = 1; }";
    let diagnostics = parse_diagnostics(source);

    assert_eq!(
        count_error(source, 1243),
        1,
        "`readonly accessor` should produce exactly one TS1243: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 1243
                && message == "'accessor' modifier cannot be used with 'readonly' modifier."
        }),
        "`readonly accessor` should anchor TS1243 on the accessor modifier: {diagnostics:?}"
    );
    assert_eq!(
        count_error(source, 1029),
        0,
        "`readonly accessor` should not be treated as a modifier-ordering issue: {diagnostics:?}"
    );
}

#[test]
fn accessor_readonly_emits_ts1243_without_ts1029() {
    let source = "class C { accessor readonly id: number = 1; }";
    let diagnostics = parse_diagnostics(source);

    assert_eq!(
        count_error(source, 1243),
        1,
        "`accessor readonly` should produce exactly one TS1243: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 1243
                && message == "'readonly' modifier cannot be used with 'accessor' modifier."
        }),
        "`accessor readonly` should anchor TS1243 on the readonly modifier: {diagnostics:?}"
    );
    assert_eq!(
        count_error(source, 1029),
        0,
        "`accessor readonly` should not be treated as a modifier-ordering issue: {diagnostics:?}"
    );
}

// =========================================================================
// TS1029: `async` never legally coexists with `readonly`/`accessor`
// (function-only vs data-member-only modifiers), and `readonly` legally
// coexists with `abstract` in EITHER order — tsc never reaches the ordering
// diagnostic in any of these shapes, matching only the member's own
// TS1024/TS1042/TS1243. Oracle-verified against typescript@7.0.2 (#16553
// coordination-board thread).
// =========================================================================

#[test]
fn readonly_async_property_no_ts1029() {
    // `readonly` before `async` on a property: tsc reports only TS1042
    // ('async' cannot be used here); it never reaches the ordering check
    // because `async` is illegal on a property regardless of order.
    let source = "class C { readonly async p: number = 1; }";
    let diagnostics = parse_diagnostics(source);
    assert!(
        !has_error(source, 1029),
        "`readonly async` on a property should not produce TS1029: {diagnostics:?}"
    );
}

#[test]
fn async_readonly_method_no_ts1029() {
    // `async` before `readonly` on a method: tsc reports only TS1024
    // ('readonly' cannot appear on a method); still no ordering diagnostic.
    let source = "class C { async readonly m(): Promise<number> { return 1; } }";
    let diagnostics = parse_diagnostics(source);
    assert!(
        !has_error(source, 1029),
        "`async readonly` on a method should not produce TS1029: {diagnostics:?}"
    );
}

#[test]
fn async_accessor_property_no_ts1029() {
    // `async` before `accessor`: tsc reports only TS1042; `async` never
    // legally applies to an auto-accessor property.
    let source = "class C { async accessor p: number = 1; }";
    let diagnostics = parse_diagnostics(source);
    assert!(
        !has_error(source, 1029),
        "`async accessor` should not produce TS1029: {diagnostics:?}"
    );
}

#[test]
fn readonly_abstract_property_no_ts1029() {
    // `readonly` before `abstract`: legal in tsc in EITHER order on an
    // abstract property (unlike `override`/`accessor`, which are genuinely
    // order-constrained relative to `abstract`), so no TS1029 either way.
    let source = "abstract class C { readonly abstract p: number; }";
    let diagnostics = parse_diagnostics(source);
    assert!(
        !has_error(source, 1029),
        "`readonly abstract` should not produce TS1029 — both orders are legal: {diagnostics:?}"
    );
}

#[test]
fn abstract_readonly_property_no_ts1029() {
    let source = "abstract class C { abstract readonly p: number; }";
    let diagnostics = parse_diagnostics(source);
    assert!(
        !has_error(source, 1029),
        "`abstract readonly` should not produce TS1029: {diagnostics:?}"
    );
}

#[test]
fn abstract_async_method_no_ts1029() {
    // `async` before `abstract`: tsc reports only TS1243 ('async' cannot be
    // used with 'abstract') — never the ordering diagnostic, since an
    // abstract member can never have a body an `async` modifier would apply
    // to. TS1243 itself is a checker-level (not parser-level) diagnostic, so
    // it is not asserted here — see `crates/tsz-checker`'s modifier-grammar
    // suite for that half.
    let source = "abstract class C { async abstract m(): Promise<number>; }";
    let diagnostics = parse_diagnostics(source);
    assert!(
        !has_error(source, 1029),
        "`async abstract` should not produce TS1029: {diagnostics:?}"
    );
}

// Regression guard: unlike `abstract`/`readonly` above, `abstract`/`override`
// and `abstract`/`accessor` ARE genuinely order-constrained (`abstract` must
// come first) and must keep reporting TS1029 in the wrong order — only the
// `readonly`/`async` triggers were removed from the `abstract` branch's
// ordering check, not `override`/`accessor`.

#[test]
fn abstract_override_correct_order_no_ts1029() {
    let source = r"
class B { m(): void {} }
abstract class D extends B {
    abstract override m(): void;
}
";
    assert!(
        !has_error(source, 1029),
        "`abstract override` is the correct order and must not produce TS1029: {:?}",
        parse_diagnostics(source)
    );
}

#[test]
fn override_abstract_wrong_order_still_ts1029() {
    let source = r"
class B { m(): void {} }
abstract class D extends B {
    override abstract m(): void;
}
";
    assert!(
        has_error(source, 1029),
        "`override abstract` (wrong order) must still produce TS1029: {:?}",
        parse_diagnostics(source)
    );
}

#[test]
fn abstract_accessor_correct_order_no_ts1029() {
    let source = "abstract class C { abstract accessor p: number; }\n";
    assert!(
        !has_error(source, 1029),
        "`abstract accessor` is the correct order and must not produce TS1029: {:?}",
        parse_diagnostics(source)
    );
}

#[test]
fn accessor_abstract_wrong_order_still_ts1029() {
    let source = "abstract class C { accessor abstract p: number; }\n";
    assert!(
        has_error(source, 1029),
        "`accessor abstract` (wrong order) must still produce TS1029: {:?}",
        parse_diagnostics(source)
    );
}

// =========================================================================
// TS1029: Modifier ordering — accessibility (public/protected) vs abstract
//
// Unlike `private`, which is a hard TS1243 conflict with `abstract` in
// EITHER order (see `abstract_static_illegal_pair_does_not_emit_ts1029`'s
// sibling checker-side check), `public`/`protected` have a valid order with
// `abstract` — before it — so writing either one after `abstract` is the
// same ordering mistake as `readonly override`/`async override` above, not
// a hard conflict. Oracle (typescript@7.0.2),
// `classAbstractMixedWithModifiers.ts`.
// =========================================================================

#[test]
fn public_abstract_correct_order_no_ts1029() {
    let source = "abstract class C { public abstract m(): void; }\n";
    assert!(
        !has_error(source, 1029),
        "`public abstract` is the correct order and must not produce TS1029: {:?}",
        parse_diagnostics(source)
    );
}

#[test]
fn abstract_public_wrong_order_ts1029() {
    let source = "abstract class C { abstract public m(): void; }\n";
    let diagnostics = parse_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, message)| *code == 1029
            && message.contains("'public' modifier must precede 'abstract' modifier")),
        "`abstract public` (wrong order) must produce TS1029: {diagnostics:?}"
    );
}

#[test]
fn protected_abstract_correct_order_no_ts1029() {
    let source = "abstract class C { protected abstract m(): void; }\n";
    assert!(
        !has_error(source, 1029),
        "`protected abstract` is the correct order and must not produce TS1029: {:?}",
        parse_diagnostics(source)
    );
}

#[test]
fn abstract_protected_wrong_order_ts1029() {
    let source = "abstract class C { abstract protected m(): void; }\n";
    let diagnostics = parse_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, message)| *code == 1029
            && message.contains("'protected' modifier must precede 'abstract' modifier")),
        "`abstract protected` (wrong order) must produce TS1029: {diagnostics:?}"
    );
}

#[test]
fn abstract_private_wrong_order_no_ts1029_ts1243_owns_it() {
    // `private` stays exclusively on the checker's TS1243 "cannot be used
    // with" path in either order — it never gets the TS1029 ordering
    // diagnostic `public`/`protected` get, because no order of `private` +
    // `abstract` is ever legal.
    let source = "abstract class C { abstract private m(): void; }\n";
    assert!(
        !has_error(source, 1029),
        "`abstract private` must not produce TS1029 — TS1243 (checker) owns this pair: {:?}",
        parse_diagnostics(source)
    );
}

#[test]
fn abstract_static_public_wrong_order_ts1029_names_static_not_abstract() {
    // Priority chain regression: when `static` AND `abstract` both precede
    // the accessibility modifier, `static` outranks `abstract` in tsc's
    // walk — the diagnostic must name `static`, not fall through to the new
    // lowest-priority `abstract` arm.
    let source = "abstract class C { abstract static public m(): void; }\n";
    let diagnostics = parse_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, message)| *code == 1029
            && message.contains("'public' modifier must precede 'static' modifier")),
        "`abstract static public` must report TS1029 against 'static', not 'abstract': {diagnostics:?}"
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
fn declare_override_produces_ts1243_not_ts1040() {
    // Reverse order `declare override` on a property → TS1243 ("'override'
    // modifier cannot be used with 'declare' modifier"), NOT TS1040. Oracle
    // (typescript@7.0.2): tsc's `checkGrammarModifiers` resolves the
    // `declare`-first conflict once the member kind is known — a property is
    // the one member-local-`declare` host it allows, and there it reports the
    // pairwise TS1243, distinct from the `override`-first order's TS1040.
    let source = r"
class B { p: number = 1; }
class D extends B {
    declare override p: number;
}
";
    assert!(
        has_error(source, 1243),
        "`declare override` on a property should produce TS1243, not TS1040: {:?}",
        parse_diagnostics(source)
    );
    assert!(
        !has_error(source, 1040),
        "`declare override` on a property must not also produce TS1040: {:?}",
        parse_diagnostics(source)
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

#[test]
fn accessor_optional_method_reports_ts1275_without_ts1276() {
    let source = "class C { accessor m?() {} }";

    assert_eq!(
        count_error(source, 1275),
        1,
        "accessor method should emit TS1275"
    );
    assert_eq!(
        count_error(source, 1276),
        0,
        "TS1276 only applies to accessor properties, not methods"
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
fn accessor_on_top_level_function_is_preserved_as_recovered_modifier() {
    let source = "accessor /* recovered */ function F1() {}";
    let (parser, root) = parse_source(source);
    assert_eq!(count_error(source, 1275), 1);

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let statement = source_file.statements.nodes[0];
    let node = arena.get(statement).unwrap();
    assert_eq!(node.kind, syntax_kind_ext::FUNCTION_DECLARATION);
    let function = arena.get_function_at(statement).unwrap();
    assert!(
        arena.has_modifier(&function.modifiers, SyntaxKind::AccessorKeyword),
        "recovered top-level accessor should be stored as a modifier"
    );
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

// =========================================================================
// TS1028: Duplicate accessibility modifier — properties AND methods
//
// tsc emits TS1028 for a second accessibility modifier on ANY class member
// (checkGrammarModifiers records each modifier and reports the duplicate
// without inspecting the member kind). tsz previously suppressed it on
// property declarations via a method-context lookahead heuristic.
// =========================================================================

#[test]
fn duplicate_accessibility_on_property_ts1028() {
    // Every property form — with/without initializer or type annotation, each
    // accessibility keyword, and a mixed pair — must emit exactly one TS1028.
    // This is the path the old method-context lookahead heuristic wrongly
    // suppressed.
    for member in [
        "public public x = 1",
        "public public x",
        "public public x: number",
        "private private y = 1",
        "protected protected z = 1",
        "public private mixed = 1",
    ] {
        let source = format!("class C {{ {member}; }}");
        assert_eq!(
            count_error(&source, 1028),
            1,
            "`{member}` should emit exactly one TS1028"
        );
    }
}

#[test]
fn duplicate_accessibility_on_method_still_ts1028() {
    // Control: the method/constructor path already fired TS1028 and must not
    // regress when the lookahead heuristic is removed.
    assert_eq!(count_error("class C { public public m() {} }", 1028), 1);
    assert_eq!(
        count_error("class C { protected protected m() {} }", 1028),
        1
    );
    assert_eq!(
        count_error("class C { public public constructor() {} }", 1028),
        1
    );
}

#[test]
fn triple_accessibility_emits_single_ts1028() {
    // tsc's checkGrammarModifiers `return`s after the first duplicate, so a
    // third accessibility keyword does not produce a second TS1028.
    let source = "class C { public public public x = 1; }";
    assert_eq!(count_error(source, 1028), 1);
}

#[test]
fn single_accessibility_no_ts1028() {
    // No false positive on a lone accessibility modifier.
    assert!(!has_error("class C { public x = 1; }", 1028));
    assert!(!has_error("class C { private m() {} }", 1028));
    assert!(!has_error("class C { protected p: number; }", 1028));
}

#[test]
fn duplicate_accessibility_keys_off_token_kind_not_member_name() {
    // The diagnostic must fire regardless of the property/method identifier,
    // confirming the parser keys off token kind rather than the binder name.
    for name in ["a", "myProp", "_x", "$field"] {
        let prop_src = format!("class C {{ public public {name} = 1; }}");
        assert_eq!(
            count_error(&prop_src, 1028),
            1,
            "duplicate accessibility on property `{name}` should emit one TS1028"
        );

        let method_src = format!("class C {{ private private {name}() {{}} }}");
        assert_eq!(
            count_error(&method_src, 1028),
            1,
            "duplicate accessibility on method `{name}()` should emit one TS1028"
        );
    }
}
