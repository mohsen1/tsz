//! Regression tests for overloaded-constructor excess-property false positives.
//!
//! Structural rule: when a `new` expression's target has more than one construct
//! signature (an overload set), a fresh object-literal argument that fits a *later*
//! overload must not be reported as an excess-property error (TS2353) against an
//! *earlier* overload's parameter type. Excess-property checking is deferred to
//! overload resolution, exactly as the call-expression path already does for
//! overloaded function calls. Only when *no* overload accepts the argument should a
//! diagnostic surface (TS2769 "No overload matches this call").
//!
//! See issue #10678 (kysely: false TS2353 excess property `executor` on
//! `KyselyConfig` — constructor-arg type resolved to the wrong overload).

use crate::test_utils::check_source_codes as get_error_codes;

/// Literal-specialized construct signatures are tried before broad signatures,
/// using the same stable partition as call overloads. This remains true in both
/// source declaration orders.
#[test]
fn interface_construct_overload_prefers_literal_specialization() {
    let source = r#"
interface BroadFirst {
    new (value: string): { selected: "broad" };
    new (value: "exact"): { selected: "exact" };
}
declare const BroadCtor: BroadFirst;
const broadFirstResult: { selected: "exact" } = new BroadCtor("exact");
declare const other: string;
const broadFallbackResult: { selected: "broad" } = new BroadCtor(other);

interface ExactFirst {
    new (renamed: "precise"): { chosen: "precise" };
    new (renamed: string): { chosen: "wide" };
}
declare const ExactCtor: ExactFirst;
const exactFirstResult: { chosen: "precise" } = new ExactCtor("precise");

interface StableSpecializedGroup {
    new (input: string): { result: "wide" };
    new (input: "same"): { result: "first" };
    new (input: "same"): { result: "second" };
}
declare const StableCtor: StableSpecializedGroup;
const stableResult: { result: "first" } = new StableCtor("same");

interface MixedGeneric {
    new <T extends string>(item: T): { result: "generic" };
    new (item: "mixed"): { result: "specialized" };
}
declare const MixedCtor: MixedGeneric;
const mixedSpecialized: { result: "specialized" } = new MixedCtor("mixed");
const mixedFallback: { result: "generic" } = new MixedCtor("other");

interface BroadBeforeInferredGeneric {
    new (value: string): { result: "broad" };
    new <T extends string>(value: T): { result: "generic" };
}
declare const BroadBeforeInferredGenericCtor: BroadBeforeInferredGeneric;
const broadBeforeInferredGeneric: { result: "broad" } =
    new BroadBeforeInferredGenericCtor("literal");

interface ContextualSpecialization {
    new (
        tag: "pick",
        callback: (value: number) => void
    ): { result: "specialized" };
    new <T>(
        tag: T,
        callback: (value: string) => void
    ): { result: "generic" };
}
declare const ContextualSpecializationCtor: ContextualSpecialization;
const contextualSpecialization: { result: "specialized" } =
    new ContextualSpecializationCtor("pick", value => value.toFixed());
const contextualFallback: { result: "generic" } =
    new ContextualSpecializationCtor("other", value => value.toUpperCase());
declare const maybeTag: "pick" | "other";
const contextualUnionFallback: { result: "generic" } =
    new ContextualSpecializationCtor(maybeTag, value => value.toUpperCase());

interface ContextualSpecializationGenericFirst {
    new <T>(
        discriminant: T,
        callback: (renamed: string) => void
    ): { result: "generic" };
    new (
        discriminant: "choose",
        callback: (renamed: number) => void
    ): { result: "specialized" };
}
declare const GenericFirstContextualCtor: ContextualSpecializationGenericFirst;
const genericFirstContextualSpecialization: { result: "specialized" } =
    new GenericFirstContextualCtor("choose", renamed => renamed.toFixed());

interface ArityFallback {
    new (
        discriminant: "choose",
        callback: (value: number) => void,
        required: boolean
    ): { result: "specialized" };
    new <T>(
        discriminant: T,
        callback: (value: string) => void
    ): { result: "generic" };
}
declare const ArityFallbackCtor: ArityFallback;
const arityFallback: { result: "generic" } =
    new ArityFallbackCtor("choose", value => value.toUpperCase());

declare const ExplicitOrderCtor: {
    new <T>(entry: string): { selected: "first"; inferred: T };
    new <U>(entry: U): { selected: "second"; inferred: U };
};
const explicitOrder: { selected: "first"; inferred: "entry" } =
    new ExplicitOrderCtor<"entry">("entry");

interface BroadResult {
    selected: "broad";
}
interface ExactResult {
    selected: "exact";
}

interface OptionalLiteral {
    new (value?: string): BroadResult;
    new (value?: "optional"): ExactResult;
}
declare const OptionalCtor: OptionalLiteral;
const optionalExact: ExactResult = new OptionalCtor("optional");

type LiteralAlias = "alias";
interface AliasIsNotSpecialized {
    new (value: string): BroadResult;
    new (value: LiteralAlias): ExactResult;
}
declare const AliasCtor: AliasIsNotSpecialized;
const aliasStaysBroad: BroadResult = new AliasCtor("alias");

interface UnionIsNotSpecialized {
    new (value: string): BroadResult;
    new (value: "left" | "right"): ExactResult;
}
declare const UnionCtor: UnionIsNotSpecialized;
const unionStaysBroad: BroadResult = new UnionCtor("left");

interface ExplicitCallbacks {
    new <T>(
        tag: "a",
        callback: (value: number) => T
    ): { selected: "a" };
    new <T>(
        tag: "b",
        callback: (value: string) => T
    ): { selected: "b" };
}
declare const ExplicitCallbacksCtor: ExplicitCallbacks;
const explicitCallback: { selected: "b" } =
    new ExplicitCallbacksCtor<number>("b", value => value.length);

interface ExplicitFallback {
    new <T>(
        tag: "pick",
        callback: (value: number) => void
    ): { selected: "pick" };
    new <T>(
        tag: string,
        callback: (value: string) => void
    ): { selected: "fallback" };
}
declare const ExplicitFallbackCtor: ExplicitFallback;
const explicitFallback: { selected: "fallback" } =
    new ExplicitFallbackCtor<boolean>("other", value => value.toUpperCase());

interface InferredOrder {
    new <const T>(
        tag: string,
        seed: T
    ): { selected: "first"; inferred: T };
    new <const U>(
        tag: U,
        seed: string
    ): { selected: "second"; inferred: U };
}
declare const InferredOrderCtor: InferredOrder;
const inferredOrder: { selected: "first"; inferred: "seed" } =
    new InferredOrderCtor("seed", "seed");

interface BaseConstruct {
    new (value: string): BroadResult;
}
interface InheritedConstruct extends BaseConstruct {
    new (value: "inherited"): ExactResult;
}
declare const InheritedCtor: InheritedConstruct;
const inheritedExact: ExactResult = new InheritedCtor("inherited");
declare const inheritedOther: string;
const inheritedBroad: BroadResult = new InheritedCtor(inheritedOther);

interface MergedConstruct {
    new (value: string): BroadResult;
}
interface MergedConstruct {
    new (value: "merged"): ExactResult;
}
declare const MergedCtor: MergedConstruct;
const mergedExact: ExactResult = new MergedCtor("merged");
declare const mergedOther: string;
const mergedBroad: BroadResult = new MergedCtor(mergedOther);

type MergedAliasValue = "merged-alias";
interface MergedAliasLater {
    new (value: string): BroadResult;
}
interface MergedAliasLater {
    new (value: MergedAliasValue): ExactResult;
}
declare const MergedAliasLaterCtor: MergedAliasLater;
const mergedAliasLater: ExactResult = new MergedAliasLaterCtor("merged-alias");
interface MergedBroadLater {
    new (value: MergedAliasValue): ExactResult;
}
interface MergedBroadLater {
    new (value: string): BroadResult;
}
declare const MergedBroadLaterCtor: MergedBroadLater;
const mergedBroadLater: BroadResult = new MergedBroadLaterCtor("merged-alias");

interface FirstMergedLiteralResult {
    selected: "first-literal";
}
interface SecondMergedLiteralResult {
    selected: "second-literal";
}
interface SecondMergedBroadResult {
    selected: "second-broad";
}
interface MultipleMergedGroups {
    new (value: "first"): FirstMergedLiteralResult;
    new (value: string): BroadResult;
}
interface MultipleMergedGroups {
    new (value: "second"): SecondMergedLiteralResult;
    new (value: string): SecondMergedBroadResult;
}
declare const MultipleMergedGroupsCtor: MultipleMergedGroups;
const firstMergedLiteral: FirstMergedLiteralResult =
    new MultipleMergedGroupsCtor("first");
const secondMergedLiteral: SecondMergedLiteralResult =
    new MultipleMergedGroupsCtor("second");
declare const mergedGroupOther: string;
const secondMergedBroad: SecondMergedBroadResult =
    new MultipleMergedGroupsCtor(mergedGroupOther);

type UnionAliasValue = "union";
interface UnionAnchorA {
    new (value: string): { selected: "a-broad" };
    new (value: UnionAliasValue): { selected: "a-alias" };
}
interface UnionAnchorB {
    new (value: string): { selected: "b-broad" };
    new (value: "union"): { selected: "b-literal" };
}
declare const UnionAnchorCtor: UnionAnchorA | UnionAnchorB;
const unionAnchorResult:
    | { selected: "a-broad" }
    | { selected: "b-broad" } =
    new UnionAnchorCtor("union");

interface LiteralDiamondBase {
    new (value: "diamond"): ExactResult;
}
interface WrappedDiamondBase {
    new (value: ("diamond")): ExactResult;
}
interface LiteralBaseFirst extends LiteralDiamondBase, WrappedDiamondBase {
    new (value: string): BroadResult;
}
declare const LiteralBaseFirstCtor: LiteralBaseFirst;
const literalBaseFirst: ExactResult = new LiteralBaseFirstCtor("diamond");
interface WrappedBaseFirst extends WrappedDiamondBase, LiteralDiamondBase {
    new (value: string): BroadResult;
}
declare const WrappedBaseFirstCtor: WrappedBaseFirst;
const wrappedBaseFirst: ExactResult = new WrappedBaseFirstCtor("diamond");

interface RestCallbacks {
    new (
        tag: string,
        ...callbacks: ((value: string) => void)[]
    ): BroadResult;
    new (
        tag: "rest",
        ...callbacks: ((value: number) => void)[]
    ): ExactResult;
}
declare const RestCtor: RestCallbacks;
const restExact: ExactResult =
    new RestCtor("rest", value => value.toFixed());
"#;
    let diagnostics = crate::test_utils::check_source(source, "test.ts", Default::default());
    assert!(
        diagnostics.is_empty(),
        "construct overloads must select the literal-specialized signature in either declaration order; got: {diagnostics:?}"
    );
}

/// A callback-body error under the selected specialized constructor must stay
/// attached to that candidate; a later generic overload is not a fallback after
/// contextual typing has selected the literal signature.
#[test]
fn specialized_construct_callback_body_error_does_not_fall_through() {
    let source = r#"
interface ContextualSpecialization {
    new (
        mode: "pick",
        callback: (value: number) => void
    ): { result: "specialized" };
    new <T>(
        mode: T,
        callback: (value: string) => void
    ): { result: "generic" };
}
declare const Ctor: ContextualSpecialization;
const selected: { result: "specialized" } =
    new Ctor("pick", value => value.toUpperCase());
"#;
    let errors = get_error_codes(source);
    assert_eq!(
        errors,
        vec![2339],
        "the literal-specialized callback parameter remains number, its return type remains selected, and no TS2769 fallback is emitted; got: {errors:?}"
    );
}

/// Tuple spreads contribute their expanded arity before overload selection.
/// The selected literal signature then supplies the callback's number context.
#[test]
fn specialized_construct_contextual_typing_accounts_for_tuple_spread_arity() {
    let source = r#"
interface Broad {
    selected: "broad";
}
interface Exact {
    selected: "exact";
}
interface SpreadCtor {
    new (
        tag: string,
        callback: (value: string) => void,
        ...tail: string[]
    ): Broad;
    new (
        tag: "pick",
        callback: (value: number) => void,
        first: boolean,
        second: boolean
    ): Exact;
}
declare const Ctor: SpreadCtor;
const result: Exact =
    new Ctor("pick", value => value.toFixed(), ...[true, true] as const);
"#;
    let diagnostics = crate::test_utils::check_source(source, "test.ts", Default::default());
    assert!(
        diagnostics.is_empty(),
        "tuple-spread arity must leave the specialized constructor eligible for contextual callback typing; got: {diagnostics:?}"
    );
}

/// Intersected constructor constituents model mixins: successful construction
/// combines every constituent instance type instead of selecting only one.
#[test]
fn construct_intersection_preserves_mixin_instance_members() {
    let source = r#"
interface A {
    a: true;
}
interface B {
    b: true;
}
type M = new (...args: any[]) => A;
type C = new (cb: (n: number) => void) => B;
declare const MC: M & C;
const both: A & B = new MC(n => n.toFixed());
"#;
    let diagnostics = crate::test_utils::check_source(source, "test.ts", Default::default());
    assert!(
        diagnostics.is_empty(),
        "constructor intersections must preserve all mixin instance members; got: {diagnostics:?}"
    );
}

/// Independently lowered global augmentations retain declaration order before
/// the single constructor-candidate reorder: literal groups stay source-stable,
/// while regular groups are tried newest-first.
#[test]
fn construct_global_augmentations_preserve_raw_declaration_order() {
    let source = r#"
export {};
declare global {
    interface FirstLiteralResult { selected: "first-literal" }
    interface FirstRegularResult { selected: "first-regular" }
    interface BuiltinAugmentedResult { selected: "builtin-augmentation" }
    interface AugmentedLiteralCtor {
        new (value: "pick"): FirstLiteralResult;
    }
    interface AugmentedRegularCtor {
        new (value: string): FirstRegularResult;
    }
    interface ErrorConstructor {
        new (message: string): BuiltinAugmentedResult;
    }
}
declare global {
    interface SecondLiteralResult { selected: "second-literal" }
    interface SecondRegularResult { selected: "second-regular" }
    interface AugmentedLiteralCtor {
        new (value: "pick"): SecondLiteralResult;
    }
    interface AugmentedRegularCtor {
        new (value: string): SecondRegularResult;
    }
}
declare const LiteralCtor: AugmentedLiteralCtor;
declare const RegularCtor: AugmentedRegularCtor;
const literalResult: FirstLiteralResult = new LiteralCtor("pick");
const regularResult: SecondRegularResult = new RegularCtor("value");
const builtinAugmentedResult: BuiltinAugmentedResult = new Error("value");
"#;
    let diagnostics = crate::test_utils::check_source(source, "test.ts", Default::default());
    assert!(
        diagnostics.is_empty(),
        "global augmentation constructor groups must retain raw declaration order; got: {diagnostics:?}"
    );
}

/// Cross-arena interface declarations are lowered in binder order before
/// constructor priority is applied.
#[test]
fn cross_file_construct_groups_preserve_raw_declaration_order() {
    let files = [
        (
            "first.ts",
            r#"
interface CrossFirstLiteralResult { selected: "first-literal" }
interface CrossFirstRegularResult { selected: "first-regular" }
interface CrossLiteralCtor {
    new (value: "pick"): CrossFirstLiteralResult;
}
interface CrossRegularCtor {
    new (value: string): CrossFirstRegularResult;
}
"#,
        ),
        (
            "second.ts",
            r#"
interface CrossSecondLiteralResult { selected: "second-literal" }
interface CrossSecondRegularResult { selected: "second-regular" }
interface CrossLiteralCtor {
    new (value: "pick"): CrossSecondLiteralResult;
}
interface CrossRegularCtor {
    new (value: string): CrossSecondRegularResult;
}
declare const LiteralCtor: CrossLiteralCtor;
declare const RegularCtor: CrossRegularCtor;
const literalResult: CrossFirstLiteralResult = new LiteralCtor("pick");
const regularResult: CrossSecondRegularResult = new RegularCtor("value");
"#,
        ),
    ];
    let diagnostics = crate::test_utils::check_all_multi_file_with_global_index(
        &files,
        crate::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.is_empty(),
        "cross-file constructor groups must retain raw binder order; got: {diagnostics:?}"
    );
}

#[test]
fn nested_cross_file_construct_groups_share_canonical_owner() {
    let files = [
        (
            "nested-first.ts",
            r#"
namespace Outer {
    export interface FirstLiteralResult { selected: "first-literal" }
    export interface FirstRegularResult { selected: "first-regular" }
    export interface LiteralCtor {
        new (value: "pick"): FirstLiteralResult;
    }
    export interface RegularCtor {
        new (value: string): FirstRegularResult;
    }
}
"#,
        ),
        (
            "nested-second.ts",
            r#"
namespace Outer {
    export interface SecondLiteralResult { selected: "second-literal" }
    export interface SecondRegularResult { selected: "second-regular" }
    export interface LiteralCtor {
        new (value: "pick"): SecondLiteralResult;
    }
    export interface RegularCtor {
        new (value: string): SecondRegularResult;
    }
}
declare const LiteralCtor: Outer.LiteralCtor;
declare const RegularCtor: Outer.RegularCtor;
const literalResult: Outer.FirstLiteralResult = new LiteralCtor("pick");
const regularResult: Outer.SecondRegularResult = new RegularCtor("value");
"#,
        ),
    ];
    let diagnostics = crate::test_utils::check_all_multi_file_with_global_index(
        &files,
        crate::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.is_empty(),
        "nested cross-file constructor groups must share one canonical owner; got: {diagnostics:?}"
    );
}

/// Class constructor overloads where the literal fits the *second* overload by
/// supplying a property the first overload lacks. tsc accepts this.
#[test]
fn class_ctor_overload_literal_fits_later_overload_no_ts2353() {
    let source = r#"
interface ConfigA { a: number }
interface PropsB { a: number; executor: string }
declare class K {
    constructor(args: ConfigA);
    constructor(args: PropsB);
}
const k = new K({ a: 1, executor: "x" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353),
        "overloaded ctor: literal fitting a later overload must not emit TS2353, got: {errors:?}"
    );
    assert!(
        !errors.contains(&2769),
        "a matching overload exists; TS2769 must not fire, got: {errors:?}"
    );
}

/// Same rule, different property/type names — proves the fix is structural, not
/// keyed to any particular identifier spelling.
#[test]
fn class_ctor_overload_renamed_members_no_ts2353() {
    let source = r#"
interface Plain { id: number }
interface Extended { id: number; label: string }
declare class Widget {
    constructor(opts: Plain);
    constructor(opts: Extended);
}
const w = new Widget({ id: 7, label: "hi" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353) && !errors.contains(&2769),
        "renamed overloaded ctor must accept later-overload literal, got: {errors:?}"
    );
}

/// Overload order should not matter: the wider overload listed first still works.
#[test]
fn class_ctor_overload_wider_first_no_error() {
    let source = r#"
interface ConfigA { a: number }
interface PropsB { a: number; executor: string }
declare class K {
    constructor(args: PropsB);
    constructor(args: ConfigA);
}
const k = new K({ a: 1, executor: "x" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353) && !errors.contains(&2769),
        "overload order must not change acceptance, got: {errors:?}"
    );
}

/// Interface construct signatures (not a class) exercise the same `new` path.
#[test]
fn interface_construct_signature_overload_no_ts2353() {
    let source = r#"
interface Ctor {
    new (args: { a: number }): unknown;
    new (args: { a: number; executor: string }): unknown;
}
declare const C: Ctor;
const k = new C({ a: 1, executor: "x" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353) && !errors.contains(&2769),
        "interface construct-signature overload must accept later-overload literal, got: {errors:?}"
    );
}

/// Alias wrapping the constructor type must behave identically.
#[test]
fn aliased_constructor_overload_no_ts2353() {
    let source = r#"
interface ConfigA { a: number }
interface PropsB { a: number; executor: string }
interface KCtor {
    new (args: ConfigA): unknown;
    new (args: PropsB): unknown;
}
type KAlias = KCtor;
declare const K: KAlias;
const k = new K({ a: 1, executor: "x" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353) && !errors.contains(&2769),
        "aliased overloaded ctor must accept later-overload literal, got: {errors:?}"
    );
}

/// `super(...)` to an overloaded base constructor follows the same `new` resolution.
#[test]
fn super_call_to_overloaded_base_ctor_no_ts2353() {
    let source = r#"
class Base {
    constructor(a: { x: number });
    constructor(a: { x: number; y: number });
    constructor(a: any) { void a; }
}
class Sub extends Base {
    constructor() { super({ x: 1, y: 2 }); }
}
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353),
        "super() to overloaded base ctor must not emit TS2353, got: {errors:?}"
    );
}

#[test]
fn super_call_uses_specialized_constructor_for_callback_context() {
    let source = r#"
declare class ContextualBase {
    constructor(tag: string, callback: (value: string) => void);
    constructor(tag: "pick", callback: (value: number) => void);
}
class ContextualDerived extends ContextualBase {
    constructor() {
        super("pick", value => value.toFixed());
    }
}
"#;
    let diagnostics = crate::test_utils::check_source(source, "test.ts", Default::default());
    assert!(
        diagnostics.is_empty(),
        "super() must contextually type callbacks through reordered construct overloads; got: {diagnostics:?}"
    );
}

/// Negative: a single (non-overloaded) constructor must STILL report excess
/// properties. The deferral only applies to genuine overload sets.
#[test]
fn single_constructor_still_reports_excess_ts2353() {
    let source = r#"
class K {
    constructor(a: { x: number }) { void a; }
}
const k = new K({ x: 1, y: 2 });
"#;
    let errors = get_error_codes(source);
    assert!(
        errors.contains(&2353),
        "single ctor with a fresh excess property must still emit TS2353, got: {errors:?}"
    );
}

/// Negative: when *no* overload accepts the literal (every candidate rejects an
/// excess property), tsc reports TS2769 — and tsz must not additionally emit a
/// spurious TS2353 against the first overload.
#[test]
fn no_matching_overload_reports_ts2769_only() {
    let source = r#"
declare class K {
    constructor(a: { x: number });
    constructor(a: { y: number });
}
const k = new K({ x: 1, y: 2, z: 3 });
"#;
    let errors = get_error_codes(source);
    assert!(
        errors.contains(&2769),
        "no overload accepts the literal; TS2769 must fire, got: {errors:?}"
    );
    assert!(
        !errors.contains(&2353),
        "the no-overload case must not also emit a spurious TS2353, got: {errors:?}"
    );
}

/// Negative: an overload mismatch driven by an incompatible property *type*
/// (not excess) still reports TS2769 when nothing matches.
#[test]
fn overload_type_mismatch_reports_ts2769() {
    let source = r#"
declare class K {
    constructor(a: { mode: "x" });
    constructor(a: { mode: "y" });
}
const k = new K({ mode: "z" });
"#;
    let errors = get_error_codes(source);
    assert!(
        errors.contains(&2769),
        "incompatible literal must report TS2769, got: {errors:?}"
    );
}
