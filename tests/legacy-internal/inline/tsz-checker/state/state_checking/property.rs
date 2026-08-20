//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/state_checking/property.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bc5a2d50cf37ec6c7344675f9f56c5aa7e2a37826ce7aeed1cca68cb46ca9ce4 1064 ts2353_spread_object_literal_reports_explicit_excess_property_only
    #[test]
    fn ts2353_spread_object_literal_reports_explicit_excess_property_only() {
        let diags = check_source_diagnostics(
            "let x = { b: 1, extra: 2 };\nlet xx: { a, b } = { a: 1, ...x, z: 3 };",
        );

        let ts2353 = diags.iter().filter(|d| d.code == 2353).collect::<Vec<_>>();
        assert_eq!(
            ts2353.len(),
            1,
            "expected one TS2353 for z, got {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        assert!(
            ts2353[0].message_text.contains("'z'"),
            "TS2353 should mention z, got: {}",
            ts2353[0].message_text
        );
    }
// TSZ_INLINE_TEST_END bc5a2d50cf37ec6c7344675f9f56c5aa7e2a37826ce7aeed1cca68cb46ca9ce4

// TSZ_INLINE_TEST_BEGIN d01b2e2cad8e026f81bf267a6cdb44bf52b98b6066e9672ed9e3303ae8784c26 1084 ts2353_inferred_pattern_target_type_reports_computed_property_name
    #[test]
    fn ts2353_inferred_pattern_target_type_reports_computed_property_name() {
        let diags = check_source_diagnostics(
            "const k = 'extra';\nconst source = { x: 1, y: 2 };\nlet { x } = { x: 1, ...source, [k]: 3 };",
        );

        let ts2353 = diags.iter().filter(|d| d.code == 2353).collect::<Vec<_>>();
        assert_eq!(
            ts2353.len(),
            1,
            "expected one TS2353 for [k], got {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        assert!(
            ts2353[0].message_text.contains("'[k]'") || ts2353[0].message_text.contains("\"[k]\""),
            "TS2353 should mention [k], got: {}",
            ts2353[0].message_text
        );
    }
// TSZ_INLINE_TEST_END d01b2e2cad8e026f81bf267a6cdb44bf52b98b6066e9672ed9e3303ae8784c26

// TSZ_INLINE_TEST_BEGIN 33bb23a0b2d27949b5c6714a2a38df3ec909ba7af01396001fc280601b34814c 1104 excess_property_method_contextual_retry_keeps_parameter_types
    #[test]
    fn excess_property_method_contextual_retry_keeps_parameter_types() {
        let diags = check_source_diagnostics(
            r#"
type Nested = { run: (value: string) => string };
declare function accept(value: { nested: Nested }): void;

accept({
    nested: {
        run(value) { return value; },
        extra: 1,
    },
});
"#,
        );

        let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
        assert_eq!(
            ts7006.len(),
            0,
            "Expected method contextual retry during excess-property checking to keep parameter context, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END 33bb23a0b2d27949b5c6714a2a38df3ec909ba7af01396001fc280601b34814c

// TSZ_INLINE_TEST_BEGIN db268ec4475ac90c31efaae9d5f0c49c3971d375b61baa1171cbb0f0d51d927e 1128 excess_property_accessor_contextual_retry_keeps_setter_parameter_types
    #[test]
    fn excess_property_accessor_contextual_retry_keeps_setter_parameter_types() {
        let diags = check_source_diagnostics(
            r#"
type Access = { get size(): number; set size(value: number); };
declare function accept(value: Access): void;

accept({
    get size() { return 1; },
    set size(value) { void value; },
    extra: 1,
});
"#,
        );

        let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
        assert_eq!(
            ts7006.len(),
            0,
            "Expected accessor contextual retry during excess-property checking to keep setter parameter context, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END db268ec4475ac90c31efaae9d5f0c49c3971d375b61baa1171cbb0f0d51d927e

// TSZ_INLINE_TEST_BEGIN 0b32345166bd3565b8775ab6391b5f3973c50c1cf96b1d6003a3cac059f2241a 1159 ts2353_discriminated_union_with_intersected_member_property_types
    /// Regression test: when a discriminated-union target has members whose
    /// discriminant property is an unsimplified intersection (e.g. the merged
    /// shape of `BaseAttribute<string> & { type: 'string' }` exposes
    /// `type: (string | undefined) & 'string'`), tsz must evaluate that
    /// property type before applying the `is_unit_type` discriminant test.
    /// Without evaluation, `is_unit_type(intersection)` returns false and the
    /// excess-property check silently bails, missing the TS2353 that tsc
    /// emits.
    #[test]
    fn ts2353_discriminated_union_with_intersected_member_property_types() {
        let diags = check_source_diagnostics(
            r#"
type BaseAttribute<T> = {
    type?: string | undefined;
    required?: boolean | undefined;
    defaultsTo?: T | undefined;
};
type StringAttribute = BaseAttribute<string> & { type: 'string'; };
type NumberAttribute = BaseAttribute<number> & {
    type: 'number';
    autoIncrement?: boolean | undefined;
};
type Attribute = string | StringAttribute | NumberAttribute;

const a: Attribute = {
    type: 'string',
    autoIncrement: true,
    required: true,
};
"#,
        );

        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert_eq!(
            ts2353.len(),
            1,
            "expected one TS2353 for 'autoIncrement' against StringAttribute, got: {diags:?}"
        );
        assert!(
            ts2353[0].message_text.contains("'autoIncrement'"),
            "TS2353 should mention 'autoIncrement', got: {}",
            ts2353[0].message_text
        );
    }
// TSZ_INLINE_TEST_END 0b32345166bd3565b8775ab6391b5f3973c50c1cf96b1d6003a3cac059f2241a

// TSZ_INLINE_TEST_BEGIN 30c22e3ac49aa506af2e4eabf5ab24983a66833371b04f8b570375d29e097ef4 1203 ts2353_no_false_positive_for_recursive_intersection_nested_literal
    #[test]
    fn ts2353_no_false_positive_for_recursive_intersection_nested_literal() {
        // `parent` is `User | undefined` in User, but the target is `UserGroup`
        // (= User & { admin: boolean }).  A nested literal that includes `admin`
        // must NOT trigger TS2353.
        let diags = check_source_diagnostics(
            r#"
interface User { name: string; parent?: User; }
type UserGroup = User & { admin: boolean; }
const u: UserGroup = { name: "Alice", admin: true, parent: { name: "Bob", admin: false } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert!(
            ts2353.is_empty(),
            "expected no TS2353 for valid nested intersection literal, got: {ts2353:?}"
        );
    }
// TSZ_INLINE_TEST_END 30c22e3ac49aa506af2e4eabf5ab24983a66833371b04f8b570375d29e097ef4

// TSZ_INLINE_TEST_BEGIN dabbaabf82bf11ad3cdffe5408e7ebf0468f8239a0e63c1ee9b82d578be491d7 1222 ts2353_no_false_positive_recursive_intersection_renamed_type_param
    #[test]
    fn ts2353_no_false_positive_recursive_intersection_renamed_type_param() {
        // Variant with differently-named interface to prove the fix is not
        // keyed on the name "User".
        let diags = check_source_diagnostics(
            r#"
interface Node { value: number; child?: Node; }
type AnnotatedNode = Node & { label: string; }
const n: AnnotatedNode = { value: 1, label: "root", child: { value: 2, label: "leaf" } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert!(
            ts2353.is_empty(),
            "expected no TS2353 for valid recursive annotated node, got: {ts2353:?}"
        );
    }
// TSZ_INLINE_TEST_END dabbaabf82bf11ad3cdffe5408e7ebf0468f8239a0e63c1ee9b82d578be491d7

// TSZ_INLINE_TEST_BEGIN 7cdc66d6736d8a9e3ff0fb87cf4360352aefde5b480ed0ff985a15f78603cb3f 1240 ts2353_still_reports_genuinely_excess_property_on_recursive_intersection
    #[test]
    fn ts2353_still_reports_genuinely_excess_property_on_recursive_intersection() {
        // Even with a recursive intersection target, a truly extra property
        // (one that's in neither member) must still cause TS2353.
        let diags = check_source_diagnostics(
            r#"
interface User { name: string; parent?: User; }
type UserGroup = User & { admin: boolean; }
const u: UserGroup = { name: "Alice", admin: true, parent: { name: "Bob", admin: false, extra: 99 } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert_eq!(
            ts2353.len(),
            1,
            "expected exactly one TS2353 for 'extra', got: {ts2353:?}"
        );
        assert!(
            ts2353[0].message_text.contains("'extra'"),
            "TS2353 should mention 'extra', got: {}",
            ts2353[0].message_text
        );
    }
// TSZ_INLINE_TEST_END 7cdc66d6736d8a9e3ff0fb87cf4360352aefde5b480ed0ff985a15f78603cb3f

// TSZ_INLINE_TEST_BEGIN b72b320c35f9394161450a46c96f4b2bb05ef3d730f85944efdb02c796d74e86 1264 ts2353_no_false_positive_recursive_intersection_via_type_alias
    #[test]
    fn ts2353_no_false_positive_recursive_intersection_via_type_alias() {
        // Same structural pattern through an explicit type alias rather than a
        // direct interface reference, to confirm alias indirection is handled.
        let diags = check_source_diagnostics(
            r#"
interface Category { name: string; parent?: Category; }
type TaggedCategory = Category & { tag: string; }
const c: TaggedCategory = { name: "root", tag: "top", parent: { name: "child", tag: "mid" } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert!(
            ts2353.is_empty(),
            "expected no TS2353 for valid tagged recursive category, got: {ts2353:?}"
        );
    }
// TSZ_INLINE_TEST_END b72b320c35f9394161450a46c96f4b2bb05ef3d730f85944efdb02c796d74e86

// TSZ_INLINE_TEST_BEGIN eb1fd73c4343122bc255a4479bc779669705b6cf74d0991222ed2bbcb704880e 1292 ts2353_symbol_index_nested_object_literal_excess_property
    #[test]
    fn ts2353_symbol_index_nested_object_literal_excess_property() {
        let diags = check_source_diagnostics(
            r#"
declare const sym: unique symbol;
interface Val { a: number; }
interface I { [k: string]: number; [k: symbol]: Val; }
const i2: I = { [sym]: { a: 1, b: 2 } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert_eq!(
            ts2353.len(),
            1,
            "expected one TS2353 on the nested excess property 'b', got: {diags:?}"
        );
        assert!(
            ts2353[0].message_text.contains("'b'"),
            "TS2353 should mention 'b', got: {}",
            ts2353[0].message_text
        );
        let ts2418: Vec<_> = diags.iter().filter(|d| d.code == 2418).collect();
        assert!(
            ts2418.is_empty(),
            "expected no outer TS2418 once the nested excess property is reported, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END eb1fd73c4343122bc255a4479bc779669705b6cf74d0991222ed2bbcb704880e

// TSZ_INLINE_TEST_BEGIN 1ec29586fad42bd0e84a0014f70414b1e1bbbc327e6be5f7267645aeec00edb0 1320 ts2353_symbol_index_nested_object_literal_no_excess_property_is_clean
    #[test]
    fn ts2353_symbol_index_nested_object_literal_no_excess_property_is_clean() {
        let diags = check_source_diagnostics(
            r#"
declare const sym: unique symbol;
interface Val { a: number; }
interface I { [k: string]: number; [k: symbol]: Val; }
const i: I = { [sym]: { a: 1 } };
"#,
        );
        assert!(
            diags.is_empty(),
            "expected no diagnostics for a matching symbol-indexed nested literal, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END 1ec29586fad42bd0e84a0014f70414b1e1bbbc327e6be5f7267645aeec00edb0

// TSZ_INLINE_TEST_BEGIN 0ba40d3578b401d81f7a848891eb17d994f2d965176e96bc7e27425218590b0d 1336 ts2322_symbol_index_nested_object_literal_type_mismatch_drills_in
    #[test]
    fn ts2322_symbol_index_nested_object_literal_type_mismatch_drills_in() {
        // This row previously pinned the *wrong* behavior on purpose — the flat
        // TS2418 computed-property message won over
        // `try_elaborate_assignment_source_error` unconditionally, rather than
        // only when there was nothing to elaborate — with a note that a future
        // fix should update it deliberately. This is that update: the
        // elaboration now drills into the nested literal for a member
        // *mismatch* the same way #16649/#16651 made it drill in for an excess
        // property.
        //
        // Oracle (`typescript@7.0.2`, `--strict --lib es2024 --target es2022`):
        //
        //   error TS2322: Type 'string' is not assignable to type 'number'.
        let diags = check_source_diagnostics(
            r#"
declare const sym: unique symbol;
interface Val { a: number; }
interface I { [k: string]: number; [k: symbol]: Val; }
const i3: I = { [sym]: { a: "wrong" } };
"#,
        );
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&2322),
            "a nested member mismatch drills in to TS2322, got: {diags:?}"
        );
        assert!(
            !codes.contains(&2418),
            "the flat TS2418 computed-property message must not also fire once \
             the mismatch elaborates, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END 0ba40d3578b401d81f7a848891eb17d994f2d965176e96bc7e27425218590b0d

// TSZ_INLINE_TEST_BEGIN 6e956dabd49cccc36b12e65be6509b7664054c35362b42b00501fdd885dca911 1370 ts2353_string_index_nested_object_literal_excess_property_regression
    #[test]
    fn ts2353_string_index_nested_object_literal_excess_property_regression() {
        // Regression guard: the `[k: string]` case already drilled in
        // correctly before this fix and must keep doing so.
        let diags = check_source_diagnostics(
            r#"
interface Val { a: number; }
interface I { [k: string]: Val; }
const i4: I = { foo: { a: 1, b: 2 } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert_eq!(
            ts2353.len(),
            1,
            "expected one TS2353 on the nested excess property 'b', got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END 6e956dabd49cccc36b12e65be6509b7664054c35362b42b00501fdd885dca911

// TSZ_INLINE_TEST_BEGIN 82ce0daccab903eda48ed65626abb7563876b708d4abd681e18a707957959c91 1389 ts2418_symbol_index_primitive_value_still_reports_flat_mismatch
    #[test]
    fn ts2418_symbol_index_primitive_value_still_reports_flat_mismatch() {
        // This issue is object-valued-index only: a `[k: symbol]` index whose
        // value type is a primitive keeps the flat TS2418 (nothing to drill
        // into).
        let diags = check_source_diagnostics(
            r#"
declare const sym: unique symbol;
interface I { [k: string]: number; [k: symbol]: number; }
const i5: I = { [sym]: { a: 1, b: 2 } };
"#,
        );
        let ts2418: Vec<_> = diags.iter().filter(|d| d.code == 2418).collect();
        assert_eq!(
            ts2418.len(),
            1,
            "expected the flat TS2418 for a primitive-valued symbol index, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END 82ce0daccab903eda48ed65626abb7563876b708d4abd681e18a707957959c91

// TSZ_INLINE_TEST_BEGIN 2d9dc759f45a073690546ba3596f8582cd036461b6efed448242660f28a38b9e 1409 ts2353_debug_structural_type_alias_recursive_intersection
    #[test]
    fn ts2353_debug_structural_type_alias_recursive_intersection() {
        let diags = check_source_diagnostics(
            r#"
type Chain = { data: string; rest?: Chain; };
type MarkedChain = Chain & { marker: number; }
const c: MarkedChain = { data: "a", marker: 1, rest: { data: "b", marker: 2 } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert!(
            ts2353.is_empty(),
            "expected no TS2353 for structural type alias recursive intersection, got: {ts2353:?}"
        );
    }
// TSZ_INLINE_TEST_END 2d9dc759f45a073690546ba3596f8582cd036461b6efed448242660f28a38b9e
