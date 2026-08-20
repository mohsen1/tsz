//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/computation/helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7908e77b8658d509ce157c4e311b5d52b43264d6412cb294e93500817ee3383c 1744 template_expr_contextual_type_no_false_positive
    #[test]
    fn template_expr_contextual_type_no_false_positive() {
        // Template expression `\`${scope}:${event}\`` passed to a parameter expecting
        // a template literal type should NOT produce TS2345
        let source = r#"
type Registry = { a: { a1: {} }; b: { b1: {} } };
type Keyof<T> = keyof T & string;
declare function f1<
  Scope extends Keyof<Registry>,
  Event extends Keyof<Registry[Scope]>,
>(eventPath: `${Scope}:${Event}`): void;
function f2<
  Scope extends Keyof<Registry>,
  Event extends Keyof<Registry[Scope]>,
>(scope: Scope, event: Event) {
  f1(`${scope}:${event}`);
}
"#;
        let errors = check_source_codes(source);
        assert!(
            !errors.contains(&2345),
            "Should not emit TS2345 for template literal matching contextual type, got: {errors:?}"
        );
    }
// TSZ_INLINE_TEST_END 7908e77b8658d509ce157c4e311b5d52b43264d6412cb294e93500817ee3383c

// TSZ_INLINE_TEST_BEGIN 60f3cec04008817b6f84d13a4b768c93a7cd352e75c2409d11f34bd443537bc9 1769 generic_array_like_context_provides_element_type
    #[test]
    fn generic_array_like_context_provides_element_type() {
        // When contextual type is a generic Application like ReadonlyArray<[K, V]>,
        // ensure the solver extracts the element type from the type arguments.
        // This exercises the Application → evaluation path in get_array_element_type.
        // The full Iterable<readonly [K, V]> path (used by Map constructor) is
        // validated by conformance tests (for-of37, for-of40, for-of50) since it
        // requires Symbol.iterator from lib definitions.
        let source = r#"
interface ReadonlyArray<T> {
    readonly length: number;
    readonly [n: number]: T;
}
declare function f<K, V>(entries: ReadonlyArray<readonly [K, V]>): [K, V];
const r = f([["", true]]);
"#;
        let errors = check_source_codes(source);
        let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
        assert!(
            !semantic_errors.contains(&2345) && !semantic_errors.contains(&2769),
            "ReadonlyArray<readonly [K, V]> should contextually type array elements as tuples, got: {semantic_errors:?}"
        );
    }
// TSZ_INLINE_TEST_END 60f3cec04008817b6f84d13a4b768c93a7cd352e75c2409d11f34bd443537bc9

// TSZ_INLINE_TEST_BEGIN 1446488056a73c4733149d18158cde69934f4fda38da51cf45c34cfed631e62c 1793 array_param_context_still_works
    #[test]
    fn array_param_context_still_works() {
        // Ensure the fix doesn't break the already-working array parameter path.
        // When the parameter is a plain array type (readonly (readonly [K, V])[]),
        // contextual typing should still work without needing the fallback.
        let source = r#"
declare function f<K, V>(entries: readonly (readonly [K, V])[]): [K, V];
const result = f([["", true]]);
"#;
        let errors = check_source_codes(source);
        let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
        assert!(
            !semantic_errors.contains(&2345) && !semantic_errors.contains(&2769),
            "Array parameter should contextually type elements as tuples, got: {semantic_errors:?}"
        );
    }
// TSZ_INLINE_TEST_END 1446488056a73c4733149d18158cde69934f4fda38da51cf45c34cfed631e62c

// TSZ_INLINE_TEST_BEGIN 839979ef452cc41ddb6d263c088630b96bb04b034bd7b50e129bdaf0af68457a 1810 generic_iterable_context_preserves_heterogeneous_entries_for_type_mismatch
    #[test]
    fn generic_iterable_context_preserves_heterogeneous_entries_for_type_mismatch() {
        let source = r#"
declare function f<K, V>(entries: readonly (readonly [K, V])[]): [K, V];
const result = f([["", true], ["", 0]]);
"#;
        let errors = check_source_codes(source);
        let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
        // tsc emits TS2322 ("Type 'number' is not assignable to type 'boolean'.")
        // on the inner element when V is inferred from the first entry
        // and the second entry's V mismatches. Earlier we incorrectly
        // surfaced TS2345 on the whole array argument because element-wise
        // elaboration was suppressed for any call argument targeting a
        // generic parameter; we now elaborate when the resolved target
        // element type is concrete.
        assert!(
            semantic_errors.contains(&2322),
            "Heterogeneous generic entries should produce TS2322 element elaboration, got: {semantic_errors:?}"
        );
    }
// TSZ_INLINE_TEST_END 839979ef452cc41ddb6d263c088630b96bb04b034bd7b50e129bdaf0af68457a

// TSZ_INLINE_TEST_BEGIN e9b088e4a3e1d6182df84cc166e6df514f9d915309440f7f3ff697096bb9d377 1831 template_expr_without_context_stays_string
    #[test]
    fn template_expr_without_context_stays_string() {
        // Template expression assigned to `string` should still work (not break)
        let source = r#"
function f(x: string, y: number): string {
    return `${x} is ${y}`;
}
"#;
        let errors = check_source_codes(source);
        // Filter out TS2318 (lib not found) since test env has no lib definitions
        let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
        assert!(
            semantic_errors.is_empty(),
            "Template expression returning string should produce no semantic errors, got: {semantic_errors:?}"
        );
    }
// TSZ_INLINE_TEST_END e9b088e4a3e1d6182df84cc166e6df514f9d915309440f7f3ff697096bb9d377

// TSZ_INLINE_TEST_BEGIN 9fb9f9693fbd8cb8666b5550f735d13f730ca341569e5f75d7e20eb3d8cf0628 1853 shadowed_symbol_call_keeps_local_return_type
    /// Issue #2871: a local function named `Symbol` must not be treated as
    /// the lib global `Symbol`. The const initializer should keep the local
    /// function's return type (`string`) instead of being inferred as
    /// `unique symbol`. Without the fix, the TS2322 lands on `asString`
    /// instead of `asSymbol`.
    #[test]
    fn shadowed_symbol_call_keeps_local_return_type() {
        let source = r#"
function test() {
    const Symbol = () => "local";
    const value = Symbol();
    const asSymbol: symbol = value;
    const asString: string = value;
    asSymbol;
    asString;
}
"#;
        let codes = check_source_codes(source);
        let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
        assert_eq!(
            ts2322_count, 1,
            "Expected exactly one TS2322 (string→symbol on asSymbol), got: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 9fb9f9693fbd8cb8666b5550f735d13f730ca341569e5f75d7e20eb3d8cf0628

// TSZ_INLINE_TEST_BEGIN 37c48ad14f7928f98d60e90b7f33925a01e3a0fe521dd6e999e77c445c6f8831 1877 shadowed_symbol_call_function_decl_not_unique_symbol
    /// Issue #2871: same rule, different declaration kind. A local
    /// `function Symbol(): \"outer\"` shadows the global, so the const
    /// initializer's type must come from the local return type, not the
    /// global `Symbol()` special case.
    #[test]
    fn shadowed_symbol_call_function_decl_not_unique_symbol() {
        let source = r#"
function outer() {
    function Symbol(): "outer" { return "outer"; }
    const value = Symbol();
    const taken: symbol = value;
    taken;
}
"#;
        let codes = check_source_codes(source);
        assert!(
            codes.contains(&2322),
            "Expected TS2322 for string→symbol via shadowed Symbol(), got: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 37c48ad14f7928f98d60e90b7f33925a01e3a0fe521dd6e999e77c445c6f8831
