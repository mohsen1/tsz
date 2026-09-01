//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/queries/binding.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN adf6d5aa751aac6961cc27c2ac0b00506f75b16300728a3fd0140b4c3826034c 250 arrow_in_binding_pattern_gets_contextual_type
    /// The contextual type fix ensures arrow function initializers in binding
    /// patterns get their parameter types inferred from the element type.
    /// Without this fix, `v => v.toString()` would be typed as `(v: any) => any`
    /// instead of `(v: number) => string`.
    #[test]
    fn arrow_in_binding_pattern_gets_contextual_type() {
        // This should not produce TS7006 (implicit any) because the arrow
        // parameter `v` should be contextually typed as `number`.
        let codes = check_source_codes(
            "interface Show { show: (x: number) => string; }
             function f({ show = v => v.toString() }: Show) {}",
        );
        assert!(
            !codes.contains(&7006),
            "Arrow param should not be implicit any: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END adf6d5aa751aac6961cc27c2ac0b00506f75b16300728a3fd0140b4c3826034c

// TSZ_INLINE_TEST_BEGIN 380e6257a8767f6a18b6ca587ad9ede3782c4bc7e1efb5a75ea5dbfd6582a351 265 var_decl_arrow_binding_gets_contextual_type
    /// Variable declaration with arrow function default in binding pattern.
    #[test]
    fn var_decl_arrow_binding_gets_contextual_type() {
        let codes = check_source_codes(
            "interface SI { stringIdentity(s: string): string; }
             let { stringIdentity: id = arg => arg }: SI = { stringIdentity: x => x };",
        );
        assert!(
            !codes.contains(&7006),
            "Arrow param in var decl binding should not be implicit any: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 380e6257a8767f6a18b6ca587ad9ede3782c4bc7e1efb5a75ea5dbfd6582a351

// TSZ_INLINE_TEST_BEGIN 86036b248ea25df1509e671b36d6d3db373b274a2767ff7e52b88a29661c47bb 278 function_expr_binding_gets_contextual_type
    /// Function expression default in binding pattern gets contextual type.
    #[test]
    fn function_expr_binding_gets_contextual_type() {
        let codes = check_source_codes(
            "interface Fn { handler: (x: number) => number; }
             function f({ handler = function(x) { return x; } }: Fn) {}",
        );
        assert!(
            !codes.contains(&7006),
            "Function expr param in binding should not be implicit any: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 86036b248ea25df1509e671b36d6d3db373b274a2767ff7e52b88a29661c47bb

// TSZ_INLINE_TEST_BEGIN 2299d1444ff1ca37ebe75eab40aa209d45590f05f7f7510725f2f59622b10c7d 292 inferred_rest_type_no_false_ts2339
    /// Destructuring from `unknown` parent type (e.g. rest type param
    /// constraint) must not emit false TS2339.
    #[test]
    fn inferred_rest_type_no_false_ts2339() {
        let codes = check_source_codes(
            "function wrap<Args extends unknown[]>(_: (...args: Args) => void) {}
             wrap(({ cancelable } = {}) => {});",
        );
        assert!(
            !codes.contains(&2339),
            "Should not emit TS2339 for destructured param with default from unknown rest type: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 2299d1444ff1ca37ebe75eab40aa209d45590f05f7f7510725f2f59622b10c7d

// TSZ_INLINE_TEST_BEGIN 1b636e6bc2b765aa1efd71939a44c5977c46b315bf8b68ea5e6b24c8dc260128 309 nested_destructured_optional_property_propagates_undefined
    /// Nested destructuring of an optional property in an annotated parameter
    /// must include `| undefined` in the inner binding's type. The single-level
    /// resolver only handles `{ x }: T` patterns; nested patterns like
    /// `{ a: { b } }: T` previously fell through to `any` and silently dropped
    /// `| undefined`, masking real assignability errors in the function body.
    #[test]
    fn nested_destructured_optional_property_propagates_undefined() {
        let codes = check_source_codes(
            "// @strict: true
             function f({ a: { b } }: { a: { b?: number } } = { a: {} }) {
                 const x: number = b;
             }",
        );
        assert!(
            codes.contains(&2322),
            "Nested destructured optional property `b` should be `number | undefined` and emit TS2322 when assigned to `number`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 1b636e6bc2b765aa1efd71939a44c5977c46b315bf8b68ea5e6b24c8dc260128

// TSZ_INLINE_TEST_BEGIN 1dbfe77390a6edd40fe96c5c91be5d3c594ccf08135717293248b33ec6a07221 327 nested_destructured_optional_property_propagates_undefined_alt_names
    /// Same rule with different identifier names (`outer/inner` instead of
    /// `a/b`) — confirms the fix is keyed on the *structure* (nested binding
    /// pattern with optional property), not on any specific identifier name
    /// (per CLAUDE.md §25 anti-hardcoding review checklist).
    #[test]
    fn nested_destructured_optional_property_propagates_undefined_alt_names() {
        let codes = check_source_codes(
            "// @strict: true
             function g({ outer: { inner } }: { outer: { inner?: string } } = { outer: {} }) {
                 const x: string = inner;
             }",
        );
        assert!(
            codes.contains(&2322),
            "Nested destructured optional `inner` should propagate `| undefined`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 1dbfe77390a6edd40fe96c5c91be5d3c594ccf08135717293248b33ec6a07221

// TSZ_INLINE_TEST_BEGIN 2b4209432fb039eb1bb9ec7f79e5bb213dc24fb0fc236009e491fa7776bf3f0d 353 const_array_default_preserves_literal_element
    /// Reported repro: `const [first = 0] = [10, 20]` infers `0 | 10`, so
    /// assigning to `0 | 10` is fine but assigning to `null` is not.
    #[test]
    fn const_array_default_preserves_literal_element() {
        let codes = check_source_codes("const [first = 0] = [10, 20]; const ok: 0 | 10 = first;");
        assert!(
            !codes.contains(&2322),
            "first should be `0 | 10`, assignable to `0 | 10`: {codes:?}"
        );
        let codes = check_source_codes("const [first = 0] = [10, 20]; const bad: 0 = first;");
        assert!(
            codes.contains(&2322),
            "first is `0 | 10`, not assignable to `0`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 2b4209432fb039eb1bb9ec7f79e5bb213dc24fb0fc236009e491fa7776bf3f0d

// TSZ_INLINE_TEST_BEGIN 612ffa89d962b8f79cd51b670d660d3bfa29eb61824be7a69911188d721ccc4a 369 const_array_default_preserves_literal_element_renamed_binder
    /// Same rule with a renamed binder — the fix is keyed on structure, not on
    /// the identifier spelling (CLAUDE.md §25 anti-hardcoding checklist).
    #[test]
    fn const_array_default_preserves_literal_element_renamed_binder() {
        let codes =
            check_source_codes("const [renamed = 0] = [10, 20]; const ok: 0 | 10 = renamed;");
        assert!(
            !codes.contains(&2322),
            "renamed should be `0 | 10`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 612ffa89d962b8f79cd51b670d660d3bfa29eb61824be7a69911188d721ccc4a

// TSZ_INLINE_TEST_BEGIN 62a5a048f6c4539bacd7515934bed2a979c81d9c8747d819d033c5a4ab54f400 381 let_array_default_widens_to_primitive
    /// `let` (and `var`) widen the inferred type to the primitive, so the
    /// literal target no longer accepts it.
    #[test]
    fn let_array_default_widens_to_primitive() {
        let codes = check_source_codes("let [first = 0] = [10, 20]; const x: 0 | 10 = first;");
        assert!(
            codes.contains(&2322),
            "let binding widens to `number`, not assignable to `0 | 10`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 62a5a048f6c4539bacd7515934bed2a979c81d9c8747d819d033c5a4ab54f400

// TSZ_INLINE_TEST_BEGIN 77b18f4b1a727cf9e1758b2169a3b27559135378e481e9102bdfcf84bc21e0e8 392 var_array_default_widens_to_primitive
    /// Same rule for `var`, oracle-verified alongside `let` (typescript@7.0.2):
    /// `var [second = 0] = [10, 20]; const y: 0 | 10 = second;` reports TS2322.
    #[test]
    fn var_array_default_widens_to_primitive() {
        let codes = check_source_codes("var [second = 0] = [10, 20]; const y: 0 | 10 = second;");
        assert!(
            codes.contains(&2322),
            "var binding widens to `number`, not assignable to `0 | 10`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 77b18f4b1a727cf9e1758b2169a3b27559135378e481e9102bdfcf84bc21e0e8

// TSZ_INLINE_TEST_BEGIN 2b08f40b226a47154dd5340e8a3933935e20c65f695de752c0330866392a59d6 404 let_object_default_widens_to_primitive
    /// Object-pattern sibling of `let_array_default_widens_to_primitive`
    /// (oracle-verified, typescript@7.0.2): a `let` object-destructuring
    /// default widens the same way an array-destructuring one does.
    #[test]
    fn let_object_default_widens_to_primitive() {
        let codes = check_source_codes("let { p = 0 } = { p: 10 }; const z: 10 = p;");
        assert!(
            codes.contains(&2322),
            "let object binding widens to `number`, not assignable to `10`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 2b08f40b226a47154dd5340e8a3933935e20c65f695de752c0330866392a59d6

// TSZ_INLINE_TEST_BEGIN 299f219e17918dd9f5a5bdfc1205a84b9fc0e8805cad8e3bfbac4e091d96f945 414 const_array_string_default_preserves_literal_element
    /// String-literal defaults preserve string-literal source elements.
    #[test]
    fn const_array_string_default_preserves_literal_element() {
        let codes =
            check_source_codes("const [a = \"d\"] = [\"s\", \"t\"]; const ok: \"s\" | \"d\" = a;");
        assert!(
            !codes.contains(&2322),
            "a should be `\"s\" | \"d\"`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 299f219e17918dd9f5a5bdfc1205a84b9fc0e8805cad8e3bfbac4e091d96f945

// TSZ_INLINE_TEST_BEGIN 39973df8baf16b1be80d71e20c71b2b63bc6652892d8ad5a91860bef04cf68f3 426 const_array_default_widens_mismatched_source_but_keeps_default_literal
    /// Mismatched primitive kinds: the source string literal widens to
    /// `string`, but the `const` default `0` is preserved, giving `string | 0`.
    #[test]
    fn const_array_default_widens_mismatched_source_but_keeps_default_literal() {
        // `string | 0` is the inferred type: assignable to `string | 0`.
        let codes = check_source_codes("const [a = 0] = [\"s\"]; const ok: string | 0 = a;");
        assert!(
            !codes.contains(&2322),
            "a should be `string | 0`: {codes:?}"
        );
        // The `0` member proves the default literal was preserved (not widened
        // to `number`): assigning to bare `string` must still fail.
        let codes = check_source_codes("const [a = 0] = [\"s\"]; const bad: string = a;");
        assert!(
            codes.contains(&2322),
            "a is `string | 0`, the literal `0` is not assignable to `string`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 39973df8baf16b1be80d71e20c71b2b63bc6652892d8ad5a91860bef04cf68f3

// TSZ_INLINE_TEST_BEGIN 4624a02373cecadcf71bdb2c913aac1afee2359d7164166f1d97f1f8d3964fb4 445 array_default_from_widened_source_stays_primitive
    /// Boundary: when the source is an already-widened `number[]` variable (not
    /// a fresh array literal), the element widens to `number` in both tsc/tsz.
    #[test]
    fn array_default_from_widened_source_stays_primitive() {
        let codes =
            check_source_codes("const arr = [1, 2, 3]; const [a = 0] = arr; const x: 0 | 10 = a;");
        assert!(
            codes.contains(&2322),
            "destructuring a `number[]` variable yields `number`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 4624a02373cecadcf71bdb2c913aac1afee2359d7164166f1d97f1f8d3964fb4

// TSZ_INLINE_TEST_BEGIN daabf66e3d88205c858529a807308c18a3fc256188cdd744cfc340269423500b 457 const_array_no_default_widens_element
    /// Control: without a default, the fresh array literal element widens to
    /// the primitive (`const [first] = [10, 20]` → `number`).
    #[test]
    fn const_array_no_default_widens_element() {
        let codes = check_source_codes("const [first] = [10, 20]; const x: 10 = first;");
        assert!(
            codes.contains(&2322),
            "without a default the element widens to `number`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END daabf66e3d88205c858529a807308c18a3fc256188cdd744cfc340269423500b

// TSZ_INLINE_TEST_BEGIN 753ad70f840a22ab43b0c60c781921d354c684619793830efa8e1fc207ea28ff 468 const_nested_array_default_preserves_literal_element
    /// Nested array destructuring inherits const-ness, so the inner element is
    /// also preserved (`const [[a = 0]] = [[10]]` → `0 | 10`).
    #[test]
    fn const_nested_array_default_preserves_literal_element() {
        let codes = check_source_codes("const [[a = 0]] = [[10]]; const ok: 0 | 10 = a;");
        assert!(
            !codes.contains(&2322),
            "nested const destructuring preserves `0 | 10`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 753ad70f840a22ab43b0c60c781921d354c684619793830efa8e1fc207ea28ff

// TSZ_INLINE_TEST_BEGIN 541ddab8d07d37d6a8e4a5beab20a4e7d5db0ae1f5d309136f2d47194c3f49ee 480 const_array_default_is_per_element
    /// Per-element: only the defaulted position preserves its literal; the
    /// non-defaulted sibling widens (`const [a, b = 0] = [10, 20]` → a:
    /// `number`, b: `0 | 20`).
    #[test]
    fn const_array_default_is_per_element() {
        // b has a default and preserves `0 | 20`.
        let codes = check_source_codes("const [a, b = 0] = [10, 20]; const ok: 0 | 20 = b;");
        assert!(!codes.contains(&2322), "b should be `0 | 20`: {codes:?}");
        // a has no default and widens to `number`.
        let codes = check_source_codes("const [a, b = 0] = [10, 20]; const bad: 10 = a;");
        assert!(
            codes.contains(&2322),
            "a has no default and widens to `number`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 541ddab8d07d37d6a8e4a5beab20a4e7d5db0ae1f5d309136f2d47194c3f49ee
