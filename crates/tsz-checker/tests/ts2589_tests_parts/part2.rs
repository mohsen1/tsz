mod issue_9784 {
    //! Tests for <https://github.com/mohsen1/tsz/issues/9784>.
    //!
    //! Structural rule: a generic type alias whose body is `T extends infer X
    //! ? <re-application of the alias> : ...` always takes the true branch (a
    //! bare `infer X` matches unconditionally), so re-applying the alias is
    //! infinite instantiation. tsc reports TS2589 and collapses the alias to
    //! the error type; this makes tsz do the same so use sites do not cascade
    //! into a spurious TS2322 against the unexpanded alias. The fix is keyed on
    //! the structural shape, not on identifier names or the grown wrapper type.
    use std::sync::{Arc, OnceLock};

    use tsz_binder::lib_loader::LibFile;

    use crate::context::CheckerOptions;
    use crate::test_utils::{
        check_source_with_libs, diagnostics_with_code, load_default_lib_files,
    };

    fn check_with_libs(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
        static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
        let libs = LIBS.get_or_init(load_default_lib_files);
        check_source_with_libs(source, "test.ts", CheckerOptions::default(), libs)
    }

    fn codes(source: &str) -> Vec<u32> {
        check_with_libs(source)
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn intersection_growth_emits_ts2589_no_cascade() {
        let diags = check_with_libs(
            "type Acc<T> = T extends infer X ? Acc<X & { k: 1 }> : never;\ntype R = Acc<{}>;\nconst r: R = { k: 1 };\n",
        );
        assert!(
            !diagnostics_with_code(&diags, 2589).is_empty(),
            "expected TS2589 for unbounded intersection growth; got: {diags:?}"
        );
        assert!(
            diagnostics_with_code(&diags, 2322).is_empty(),
            "poisoned alias must collapse to error type, no TS2322 cascade; got: {diags:?}"
        );
    }

    #[test]
    fn renamed_alias_and_var_still_emits_ts2589() {
        let diags = check_with_libs(
            "type Grow<P> = P extends infer Y ? Grow<Y & { m: 2 }> : never;\ntype Q = Grow<{}>;\nconst q: Q = { m: 2 };\n",
        );
        assert!(
            !diagnostics_with_code(&diags, 2589).is_empty(),
            "renamed alias/var must still emit TS2589; got: {diags:?}"
        );
        assert!(
            diagnostics_with_code(&diags, 2322).is_empty(),
            "renamed alias must not cascade TS2322; got: {diags:?}"
        );
    }

    #[test]
    fn direct_recursion_still_emits_ts2589() {
        let diags = check_with_libs(
            "type Foo<T> = T extends unknown ? Foo<T> : unknown;\ntype R = Foo<number>;\nconst r: R = { k: 1 };\n",
        );
        assert!(
            !diagnostics_with_code(&diags, 2589).is_empty(),
            "direct recursion must still emit TS2589; got: {diags:?}"
        );
    }

    #[test]
    fn terminating_infer_alias_no_ts2589() {
        let diags = check_with_libs(
            "type Once<T> = T extends infer X ? { v: X } : never;\ntype R = Once<number>;\nconst ok: R = { v: 1 };\nconst bad: R = { v: 5 as unknown as string };\n",
        );
        assert!(
            diagnostics_with_code(&diags, 2589).is_empty(),
            "terminating infer alias must NOT emit TS2589; got: {diags:?}"
        );
        assert!(
            !diagnostics_with_code(&diags, 2322).is_empty(),
            "the real string-vs-number mismatch must still be reported; got: {diags:?}"
        );
    }

    #[test]
    fn guarded_infer_recursion_no_ts2589() {
        let cs = codes(
            "type Flatten<T> = T extends infer X ? (X extends readonly any[] ? Flatten<X[number]> : X) : never;\ntype R = Flatten<number[][]>;\nconst r: R = 5;\n",
        );
        assert!(
            !cs.contains(&2589),
            "guarded recursion that terminates must NOT emit TS2589; got: {cs:?}"
        );
        assert!(
            !cs.contains(&2322),
            "Flatten<number[][]> resolves to number, 5 is assignable; got: {cs:?}"
        );
    }
}

mod issue_9777 {
    //! Tests for <https://github.com/mohsen1/tsz/issues/9777>.
    //!
    //! Structural rule: when a recursive type alias re-applies itself through an
    //! `infer`/conditional wrapper and the type argument *grows* every step (an
    //! unbounded template-literal string, tuple, or intersection), the recursion
    //! is divergent. tsc bounds it and reports TS2589 ("Type instantiation is
    //! excessively deep and possibly infinite"); tsz must do the same instead of
    //! expanding the argument without bound until it exhausts memory/time.
    //!
    //! The growing recursion reaches the tail-call instantiation path
    //! (`try_instantiate_application_for_tail_call`), which intentionally skips
    //! the per-`DefId` depth guard so convergent tail recursion can iterate past
    //! it. A cumulative structural-weight budget on that path catches the
    //! divergent (growing) case while leaving convergent recursion untouched.
    //! The rule is keyed on structural growth, not on identifier names, the
    //! grown wrapper type, or the spelling of any single witness.
    use std::sync::{Arc, OnceLock};

    use tsz_binder::lib_loader::LibFile;

    use crate::context::CheckerOptions;
    use crate::test_utils::{check_source_with_libs, load_default_lib_files};

    fn codes(source: &str) -> Vec<u32> {
        static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
        let libs = LIBS.get_or_init(load_default_lib_files);
        check_source_with_libs(source, "test.ts", CheckerOptions::default(), libs)
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    /// Reported repro: unbounded template-literal growth behind an `infer`/
    /// conditional wrapper hangs without the fix; it must emit TS2589 instead.
    #[test]
    fn template_growth_infer_indirection_emits_ts2589() {
        let cs = codes(
            "type Grow<A> = A extends infer X ? (X extends string ? Grow<`${A}${A}`> : never) : never;\ntype R = Grow<\"ab\">;\nconst r: R = \"x\" as any;\n",
        );
        assert!(
            cs.contains(&2589),
            "unbounded template growth through infer wrapper must emit TS2589; got: {cs:?}"
        );
    }

    /// Same rule, different identifier spellings and a primitive-typed argument.
    /// If renaming the bound variable or alias breaks the fix, it is hardcoded.
    #[test]
    fn template_growth_renamed_params_emits_ts2589() {
        let cs = codes(
            "type Expand<S> = S extends infer Y ? (Y extends string ? Expand<`${S}-${S}`> : never) : never;\ntype Q = Expand<\"z\">;\nconst q: Q = \"x\" as any;\n",
        );
        assert!(
            cs.contains(&2589),
            "renamed template-growth alias must still emit TS2589; got: {cs:?}"
        );
    }

    /// Witness from the issue comment: unbounded tuple growth behind the same
    /// `infer`/conditional wrapper. Linear (not exponential) growth, same rule.
    #[test]
    fn tuple_growth_infer_indirection_emits_ts2589() {
        let cs = codes(
            "type Push<T extends any[]> = T extends infer X ? (X extends any[] ? Push<[...X, 1]> : never) : never;\ntype R = Push<[]>;\nconst r: R = [] as any;\n",
        );
        assert!(
            cs.contains(&2589),
            "unbounded tuple growth through infer wrapper must emit TS2589; got: {cs:?}"
        );
    }

    /// Renamed tuple-growth variant — proves the fix is not keyed to spellings.
    #[test]
    fn tuple_growth_renamed_params_emits_ts2589() {
        let cs = codes(
            "type Append<L extends unknown[]> = L extends infer M ? (M extends unknown[] ? Append<[...M, true]> : never) : never;\ntype W = Append<[]>;\nconst w: W = [] as any;\n",
        );
        assert!(
            cs.contains(&2589),
            "renamed tuple-growth alias must still emit TS2589; got: {cs:?}"
        );
    }

    /// Control: a bounded infer-wrapped template recursion that terminates well
    /// before the growth budget must be accepted, with no spurious TS2589.
    #[test]
    fn bounded_infer_template_no_ts2589() {
        let cs = codes(
            "type Build<S extends string> = S extends infer X ? (X extends \"xxx\" ? X : Build<`${S}x`>) : never;\ntype R = Build<\"\">;\nconst r: R = \"xxx\";\n",
        );
        assert!(
            !cs.contains(&2589),
            "bounded template recursion must NOT emit TS2589; got: {cs:?}"
        );
        assert!(
            !cs.contains(&2322),
            "Build<\"\"> resolves to \"xxx\", which the value matches; got: {cs:?}"
        );
    }

    /// Control: a bounded infer-wrapped tuple recursion that terminates must be
    /// accepted. Mirrors the issue's `Loop` control with a small bound so the
    /// test stays fast and does not depend on the exact ceiling.
    #[test]
    fn bounded_infer_tuple_loop_no_ts2589() {
        let cs = codes(
            "type Loop<A extends 1[], N extends number> = A['length'] extends infer L ? (L extends N ? A : Loop<[...A, 1], N>) : never;\ntype R = Loop<[], 3>;\nconst r: R = [1, 1, 1];\n",
        );
        assert!(
            !cs.contains(&2589),
            "bounded tuple recursion must NOT emit TS2589; got: {cs:?}"
        );
    }
}

mod issue_10859 {
    //! Tests for <https://github.com/mohsen1/tsz/issues/10859>.
    //!
    //! Structural rule: a `default_omitting_recursive_alias` whose use site
    //! supplies all type arguments explicitly is always finite — the body's
    //! recursive sub-call omits the counter, so it always hits the base case.
    //! The aggressive TS2589 evaluator must be skipped for such references;
    //! it should only fire when the use site itself omits the counter arg.
    use std::sync::{Arc, OnceLock};

    use tsz_binder::lib_loader::LibFile;

    use crate::context::CheckerOptions;
    use crate::test_utils::{check_source_with_libs, load_default_lib_files};

    /// Common preamble shared by all tests in this module.
    const DEEP_OBJECT_PREAMBLE: &str = r#"
type BuildTuple<N extends number, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;

type DeepObject<T, N extends number = 0> =
  BuildTuple<N> extends []
    ? T
    : { [K in keyof T]: DeepObject<T[K]> };
"#;

    fn codes(source: &str) -> Vec<u32> {
        static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
        let libs = LIBS.get_or_init(load_default_lib_files);
        check_source_with_libs(source, "test.ts", CheckerOptions::default(), libs)
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    fn codes_with_preamble(extra: &str) -> Vec<u32> {
        codes(&format!("{DEEP_OBJECT_PREAMBLE}{extra}"))
    }

    /// Primary repro: `DeepObject<Anchor>` with depth N=0 (the default) must
    /// evaluate to `Anchor` immediately — `BuildTuple<0> = []` so the true
    /// branch is taken — without emitting TS2589 or hanging.
    #[test]
    fn deep_object_anchor_depth_zero_no_ts2589() {
        let cs = codes_with_preamble(
            r#"type Anchor = { value: string; nested?: Anchor };
type Fixed = DeepObject<Anchor>;
"#,
        );
        assert!(
            !cs.contains(&2589),
            "DeepObject<Anchor> at depth 0 must NOT emit TS2589; BuildTuple<0>=[] so base case fires; got: {cs:?}"
        );
    }

    /// Renamed-alias variant: the fix must not be keyed to specific identifiers.
    #[test]
    fn deep_object_renamed_no_ts2589() {
        let cs = codes(
            r#"
type MkArr<N extends number, A extends any[] = []> =
  A['length'] extends N ? A : MkArr<N, [...A, 0]>;

type Recurse<T, D extends number = 0> =
  MkArr<D> extends []
    ? T
    : { [K in keyof T]: Recurse<T[K]> };

type Node = { id: number; children?: Node[] };
type Result = Recurse<Node>;
"#,
        );
        assert!(
            !cs.contains(&2589),
            "renamed aliases must not emit TS2589 at depth 0; got: {cs:?}"
        );
    }

    /// Self-referential anchor with multiple properties: fan-out through
    /// `keyof` must not re-evaluate `DeepObject<Anchor>` exponentially.
    /// Correct result: `DeepObject<Anchor>` = `Anchor` (base case at N=0).
    #[test]
    fn deep_object_self_referential_multi_prop_no_ts2589() {
        let cs = codes_with_preamble(
            r#"type Tree = { label: string; left?: Tree; right?: Tree; value: number };
type DeepTree = DeepObject<Tree>;
"#,
        );
        assert!(
            !cs.contains(&2589),
            "fan-out through multiple self-referential properties must not emit TS2589; got: {cs:?}"
        );
    }

    /// At depth N=1 the true branch is `{ [K in keyof T]: DeepObject<T[K]> }`.
    /// For a non-recursive object type (`{ x: number; y: string }`) this is
    /// finite and must NOT emit TS2589.
    #[test]
    fn deep_object_depth_one_flat_type_no_ts2589() {
        let cs = codes_with_preamble(
            r#"type Flat = { x: number; y: string };
type D1 = DeepObject<Flat, 1>;
"#,
        );
        assert!(
            !cs.contains(&2589),
            "DeepObject over a flat type at depth 1 must NOT emit TS2589; got: {cs:?}"
        );
    }

    /// Definitions alone (no instantiation) must not emit TS2589.
    #[test]
    fn deep_object_definitions_only_no_ts2589() {
        let cs = codes_with_preamble("type Flat = { x: number; y: string };\n");
        assert!(
            !cs.contains(&2589),
            "definition-only (no D1 use) must NOT emit TS2589; got: {cs:?}"
        );
    }

    /// Primitive T at depth 1: explicit N with non-object T must not emit TS2589.
    #[test]
    fn deep_object_depth_one_primitive_no_ts2589() {
        let cs = codes_with_preamble("type D1 = DeepObject<number, 1>;\n");
        assert!(
            !cs.contains(&2589),
            "DeepObject<number, 1> must NOT emit TS2589; got: {cs:?}"
        );
    }

    /// Depth 2 with a flat object: the body at depth 2 yields
    /// `{ x: DeepObject<number> }` (N omitted → defaults to 0 → base case).
    /// All type args are explicit at the use site so the probe must be skipped.
    #[test]
    fn deep_object_depth_two_flat_type_no_ts2589() {
        let cs = codes_with_preamble(
            r#"type Flat = { x: number; y: string };
type D2 = DeepObject<Flat, 2>;
"#,
        );
        assert!(
            !cs.contains(&2589),
            "DeepObject<Flat, 2> must NOT emit TS2589; got: {cs:?}"
        );
    }

    /// Explicit N=0 at the use site: `BuildTuple<0> = []` so the base case fires
    /// immediately. Must not emit TS2589 even though N is fully spelled out.
    #[test]
    fn deep_object_explicit_depth_zero_no_ts2589() {
        let cs = codes_with_preamble(
            r#"type Flat = { x: number; y: string };
type D0 = DeepObject<Flat, 0>;
"#,
        );
        assert!(
            !cs.contains(&2589),
            "DeepObject<Flat, 0> (explicit N=0) must NOT emit TS2589; got: {cs:?}"
        );
    }
}
