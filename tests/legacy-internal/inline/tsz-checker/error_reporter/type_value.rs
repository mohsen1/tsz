//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/error_reporter/type_value.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e09fe3bf9942b4e9f65335e6162dbc4329d2ad1ed8c4c91538fb77a90ace6608 1327 emits_ts2693_for_computed_type_keyword_in_type_member
    #[test]
    fn emits_ts2693_for_computed_type_keyword_in_type_member() {
        let diagnostics = check_source_diagnostics(
            r#"
namespace m1 {
  export class C2 {
    public get p(arg) {
      return 0;
    }
  }

  export function f4(arg1: {
    [number]: C1;
  }) {}
}

class C1 {}
"#,
        );

        // Primitive type keywords recovered in computed type-member keys flow
        // through the same wrong-meaning path as value-position keyword usage.
        assert!(
            diagnostics.iter().any(|diag| diag.code == 2693),
            "Expected TS2693 for computed type keyword in type member, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END e09fe3bf9942b4e9f65335e6162dbc4329d2ad1ed8c4c91538fb77a90ace6608

// TSZ_INLINE_TEST_BEGIN 71707ed54374676a0076eff7c91d3880c19ad83b8eab13ec685ebe5bef52c92f 1355 emits_ts2690_for_type_alias_computed_type_member_key
    #[test]
    fn emits_ts2690_for_type_alias_computed_type_member_key() {
        let diagnostics = check_source_diagnostics(
            r#"
type KeyName = "name";
type Shape = {
  [KeyName]: string;
};
"#,
        );

        let ts2690 = diagnostics
            .iter()
            .find(|diag| diag.code == 2690)
            .expect("Expected TS2690 for type alias used as computed type member key");
        assert!(
            ts2690
                .message_text
                .contains("Did you mean to use 'K in KeyName'"),
            "TS2690 should use the mapped-type suggestion from identifier facts, got: {ts2690:?}",
        );
    }
// TSZ_INLINE_TEST_END 71707ed54374676a0076eff7c91d3880c19ad83b8eab13ec685ebe5bef52c92f

// TSZ_INLINE_TEST_BEGIN 646b080b38121c14a42650666bd64c40267bfa8325714b89f2a1f61ef871bc5a 1378 emits_ts2690_for_commented_computed_type_member_key
    #[test]
    fn emits_ts2690_for_commented_computed_type_member_key() {
        let diagnostics = check_source_diagnostics(
            r#"
type OtherKey = "other";
interface Shape {
  [/* before */ OtherKey /* after */]: number;
}
"#,
        );

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == 2690 && diag.message_text.contains("O in OtherKey")),
            "Expected TS2690 from computed-property AST ancestry with comments around the key, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 646b080b38121c14a42650666bd64c40267bfa8325714b89f2a1f61ef871bc5a

// TSZ_INLINE_TEST_BEGIN 6354a22b0156885c3fda7a149c036aa1a5966cfb91cd749880aa3e74cedcffe6 1397 emits_ts2693_for_type_alias_value_use_outside_computed_member_key
    #[test]
    fn emits_ts2693_for_type_alias_value_use_outside_computed_member_key() {
        let diagnostics = check_source_diagnostics(
            r#"
type Plain = string;
Plain;
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2693),
            "Expected ordinary type/value TS2693 outside computed type members, got: {diagnostics:?}",
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2690),
            "Should not emit mapped-type suggestion outside computed type members, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 6354a22b0156885c3fda7a149c036aa1a5966cfb91cd749880aa3e74cedcffe6

// TSZ_INLINE_TEST_BEGIN 028eff966f233aeb6d5042a6713fdbcc39456207ed85cd1556f1ba9c6075e1c5 1416 suppresses_ts2693_for_new_primitive_array_recovery
    #[test]
    fn suppresses_ts2693_for_new_primitive_array_recovery() {
        let diagnostics = check_source_diagnostics(
            r#"
const x = new number[];
"#,
        );

        let ts2693_count = diagnostics.iter().filter(|diag| diag.code == 2693).count();
        assert_eq!(
            ts2693_count, 0,
            "Expected no TS2693 for `new number[]` parse recovery, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 028eff966f233aeb6d5042a6713fdbcc39456207ed85cd1556f1ba9c6075e1c5

// TSZ_INLINE_TEST_BEGIN 90ed2a91c1ef1c8a0a5e3b7d48a4854829c04115fa140d558bbfe19373adf412 1431 emits_ts2702_for_empty_interface_used_as_namespace
    #[test]
    fn emits_ts2702_for_empty_interface_used_as_namespace() {
        // Empty interface has no property "hello", so TS2702 should fire
        let diagnostics = check_source_diagnostics(
            r#"
interface OhNo {}
declare let y: OhNo.hello;
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2702),
            "Expected TS2702 for empty interface used as namespace, got: {diagnostics:?}",
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2713),
            "Should NOT emit TS2713 when property doesn't exist, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 90ed2a91c1ef1c8a0a5e3b7d48a4854829c04115fa140d558bbfe19373adf412

// TSZ_INLINE_TEST_BEGIN 17384948411ff94494d8890380618ee14d9bdc4681f0fe6777cb23aa14a38211 1451 emits_ts2713_for_interface_property_as_type
    #[test]
    fn emits_ts2713_for_interface_property_as_type() {
        // Interface has property "bar", so TS2713 (with suggestion) should fire
        let diagnostics = check_source_diagnostics(
            r#"
interface Foo { bar: string; }
var x: Foo.bar = "";
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2713),
            "Expected TS2713 for interface property used as type, got: {diagnostics:?}",
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2702),
            "Should NOT emit TS2702 when property exists, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 17384948411ff94494d8890380618ee14d9bdc4681f0fe6777cb23aa14a38211

// TSZ_INLINE_TEST_BEGIN e1045ad7427130640a9f0043350b97b3f9afb594199f77d600d26e178a55fe6c 1471 emits_ts2702_for_union_with_non_shared_property
    #[test]
    fn emits_ts2702_for_union_with_non_shared_property() {
        // Union where NOT all members have "bar" (Test5 pattern) → TS2702
        let diagnostics = check_source_diagnostics(
            r#"
type Foo = { bar: number } | { wat: string };
var x: Foo.bar = "";
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2702),
            "Expected TS2702 for union with non-shared property, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END e1045ad7427130640a9f0043350b97b3f9afb594199f77d600d26e178a55fe6c

// TSZ_INLINE_TEST_BEGIN 8792af677abc2267912e13e763db3c02ac8fd196de3d6aade2f8db114d7fb948 1487 emits_ts2713_for_union_with_shared_property
    #[test]
    fn emits_ts2713_for_union_with_shared_property() {
        // Union where ALL members have "bar" (Test4 pattern) → TS2713
        let diagnostics = check_source_diagnostics(
            r#"
type Foo = { bar: number } | { bar: string };
var x: Foo.bar = "";
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2713),
            "Expected TS2713 for union with shared property, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 8792af677abc2267912e13e763db3c02ac8fd196de3d6aade2f8db114d7fb948

// TSZ_INLINE_TEST_BEGIN 635bca3288e1e91a1b683b7a1d17606ffebcf1806b4422d21d8634dadc3dd812 1503 emits_ts2713_for_type_alias_with_property
    #[test]
    fn emits_ts2713_for_type_alias_with_property() {
        // Type alias with property "bar" → TS2713
        let diagnostics = check_source_diagnostics(
            r#"
type Foo = { bar: string; };
var x: Foo.bar = "";
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2713),
            "Expected TS2713 for type alias property used as type, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 635bca3288e1e91a1b683b7a1d17606ffebcf1806b4422d21d8634dadc3dd812

// TSZ_INLINE_TEST_BEGIN 7f4b64ecd560f28d955a9d730bfc6925eb42f23ef5fcf4fb1bb8f10c621eb03e 1519 suppresses_ts1361_for_computed_property_in_interface
    #[test]
    fn suppresses_ts1361_for_computed_property_in_interface() {
        // Type-only import used in interface computed property name should NOT
        // emit TS1361 — the expression is never evaluated at runtime.
        let diagnostics = check_source_diagnostics(
            r#"
import type { onInit } from './hooks';
interface Component {
  [onInit]?(): void;
}
"#,
        );

        let ts1361_count = diagnostics.iter().filter(|d| d.code == 1361).count();
        assert_eq!(
            ts1361_count, 0,
            "Should not emit TS1361 for computed property in interface, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 7f4b64ecd560f28d955a9d730bfc6925eb42f23ef5fcf4fb1bb8f10c621eb03e

// TSZ_INLINE_TEST_BEGIN 1ff589a684b2b3fae0466138ed039bc6993a815642296994531a268eb82b3882 1539 suppresses_ts1361_for_computed_property_in_type_literal
    #[test]
    fn suppresses_ts1361_for_computed_property_in_type_literal() {
        let diagnostics = check_source_diagnostics(
            r#"
import type { key } from './keys';
type T = { [key]: any; };
"#,
        );

        let ts1361_count = diagnostics.iter().filter(|d| d.code == 1361).count();
        assert_eq!(
            ts1361_count, 0,
            "Should not emit TS1361 for computed property in type literal, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 1ff589a684b2b3fae0466138ed039bc6993a815642296994531a268eb82b3882

// TSZ_INLINE_TEST_BEGIN 120c4a3ab066253b6aeb6e05279a015cd9adb7a0bf3f4e05544dfa1ec005d727 1555 suppresses_ts1361_for_computed_accessor_in_type_literal
    #[test]
    fn suppresses_ts1361_for_computed_accessor_in_type_literal() {
        // Adjacent case to `suppresses_ts1361_for_computed_property_in_type_literal`:
        // the get/set-accessor member arm in `get_type_from_type_literal` is a
        // separate call site of the same wide-symbol classifier and needs the
        // same `checking_computed_property_name` context published around it.
        // Renamed binder from the property-signature test (#16466 adjacent-case
        // discipline: don't let the fix look coupled to one identifier).
        let diagnostics = check_source_diagnostics(
            r#"
import type { propId } from './ids';
type WithAccessor = { get [propId](): any; };
"#,
        );

        let ts1361_count = diagnostics.iter().filter(|d| d.code == 1361).count();
        assert_eq!(
            ts1361_count, 0,
            "Should not emit TS1361 for computed accessor in type literal, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 120c4a3ab066253b6aeb6e05279a015cd9adb7a0bf3f4e05544dfa1ec005d727

// TSZ_INLINE_TEST_BEGIN 38a5ab6270e4f6183552b802b396906194c1d0eb40c6b242f236f436defc9a72 1577 alias_merges_with_local_value_suppresses_ts1361
    #[test]
    fn alias_merges_with_local_value_suppresses_ts1361() {
        // When import type is followed by a local const with the same name,
        // the const should shadow the import type in value position.
        let diagnostics = check_source_diagnostics(
            r#"
import type { A } from './a';
const A: A = "a";
A.toUpperCase();
"#,
        );

        let ts1361_count = diagnostics.iter().filter(|d| d.code == 1361).count();
        assert_eq!(
            ts1361_count, 0,
            "Should not emit TS1361 when local value shadows type-only import, got: {diagnostics:?}",
        );
    }
// TSZ_INLINE_TEST_END 38a5ab6270e4f6183552b802b396906194c1d0eb40c6b242f236f436defc9a72
