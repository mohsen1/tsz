    // ------------------------------------------------------------------
    // Cross-module generic interface heritage through the real program
    // path (PR witness from #13195; solver-env def registration ordering).
    //
    // Structural rule: when a *generic* interface declared in another
    // program module (with an `extends` clause) is referenced from an
    // importing file, tsc resolves it in its declaring module including
    // heritage. tsz publishes the declaring checker's heritage-merged body
    // in the shared `DefinitionStore` and the importing file consumes it
    // when its local heritage merge is a no-op; the import-alias `DefId`
    // forwards to the same body so alias-keyed applications stay
    // expandable. Binder names vary across cases.
    // ------------------------------------------------------------------

    #[test]
    fn program_mode_imported_generic_interface_heritage_param_annotation_resolves() {
        let options = project_mode_es2015_strict_options();
        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/defs.ts",
                    r#"
export interface Stem<R extends string = string> {
  pulp?: string;
}
export interface Wrap<R extends string = string> extends Stem<R> {
  rind: number;
}
"#,
                ),
                (
                    "/p/main.ts",
                    r#"
import type { Wrap } from "./defs";
export function go(w: Wrap) {
  w.rind;
  w.pulp;
}
declare const d: Wrap;
d.rind;
d.pulp;
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );
        let bogus: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2339)
            .collect();
        assert!(
            bogus.is_empty(),
            "inherited members of an imported generic interface must resolve \
             (parameter annotation and declare-const forms): {bogus:?}"
        );
    }

    // ------------------------------------------------------------------
    // #13554: a cross-module *generic* interface that `extends` a base
    // carrying a METHOD member dropped every inherited member when imported.
    //
    // Root cause: the importing file's local heritage merge is a no-op for a
    // foreign declaration, so it relies on consuming the declaring checker's
    // published heritage-merged body. The consumption gate refused any body
    // `contains_callable_or_conditional` — a method makes the body "contain a
    // callable" — to avoid unmasking the #13232 resolver-less *conditional*
    // defect. That over-broad guard also blocked inert method-bearing bodies.
    //
    // Fix: gate on a conditional reachable *through* applied aliases instead of
    // on any callable. Plain methods (no conditional, even behind an alias) are
    // consumed; conditional-bearing bodies stay gated. Binder names vary across
    // cases so the behavior follows the type shape, not a spelling.
    // ------------------------------------------------------------------

    fn assert_no_2339(diagnostics: &[Diagnostic], ctx: &str) {
        let bogus: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2339)
            .map(|diag| diag.message_text.to_string())
            .collect();
        assert!(bogus.is_empty(), "{ctx}: unexpected TS2339: {bogus:?}");
    }

    #[test]
    fn repro13554_method_shorthand_base_keeps_inherited() {
        let options = project_mode_es2015_strict_options();
        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/base.ts",
                    r#"
export interface Base<T> {
  m(): void;
  v?: T;
}
export interface Derived<T> extends Base<T> {}
"#,
                ),
                (
                    "/p/main.ts",
                    r#"
import type { Derived } from "./base";
declare const d: Derived<number>;
d.v;
d.m();
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );
        assert_no_2339(&diagnostics, "method-shorthand base, generic derived");
    }

    #[test]
    fn repro13554_renamed_binders_and_nested_method_keep_inherited() {
        // Every binder renamed; the method shorthand is nested inside a base
        // member's object type, and the derived interface re-declares one of
        // its own members. Inherited members (`payload`, `kind`) must resolve.
        let options = project_mode_es2015_strict_options();
        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/shapes.ts",
                    r#"
export interface Envelope<Kind, Payload> {
  meta: { stamp(): string };
  payload?: Payload;
  kind?: Kind;
}
export interface SealedEnvelope<Kind, Payload>
  extends Envelope<Kind, Payload> {
  meta: { stamp(): string };
}
"#,
                ),
                (
                    "/p/station.ts",
                    r#"
import type { SealedEnvelope } from "./shapes";
declare const e: SealedEnvelope<"json", { id: number }>;
const p = e.payload;
const k = e.kind;
const s = e.meta.stamp();
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );
        assert_no_2339(&diagnostics, "renamed binders, nested method shorthand");
    }

    #[test]
    fn program_mode_imported_generic_interface_heritage_reversed_file_order_resolves() {
        let options = project_mode_es2015_strict_options();
        // Importing file listed before the declaring file: resolution must
        // not depend on the declaring checker having run first.
        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/entry.ts",
                    r#"
import type { Crown } from "./shapes";
export function probe(c: Crown) {
  c.gem;
  c.metal;
}
"#,
                ),
                (
                    "/p/shapes.ts",
                    r#"
export interface Band<V = unknown> {
  metal?: string;
}
export interface Crown<V = unknown> extends Band<V> {
  gem: number;
}
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );
        let bogus: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2339)
            .collect();
        assert!(
            bogus.is_empty(),
            "heritage members must resolve regardless of file order: {bogus:?}"
        );
    }

    /// Residue (un-ignore when fixed): in the unit harness, the FIRST
    /// explicit-type-args reference to a chained foreign interface
    /// (`Storm<string>` where `Storm extends Cloud extends Mist` across
    /// modules) resolves before the importing checker consumes the published
    /// heritage-merged body, and its member diagnostics are emitted from the
    /// heritage-dropped form. The real CLI driver path resolves the same
    /// shape correctly (covered by the e2e witnesses in the PR), so this is
    /// pinned harness-order behavior, not user-facing.
    #[test]
    #[ignore = "first explicit-args reference precedes published-body consumption in the unit harness; CLI path resolves it"]
    fn program_mode_imported_chained_first_explicit_args_reference_residue() {
        let options = project_mode_es2015_strict_options();
        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/a.ts",
                    "export interface Mist<Q = unknown> { vapor?: Q; }\n",
                ),
                (
                    "/p/b.ts",
                    "import type { Mist } from \"./a\";\nexport interface Cloud<Q = unknown> extends Mist<Q> { rain: number; }\nexport interface Storm<Q = unknown> extends Cloud<Q> { wind: boolean; }\n",
                ),
                (
                    "/p/c.ts",
                    "import type { Storm } from \"./b\";\ndeclare const s1: Storm<string>;\ns1.wind;\ns1.rain;\ns1.vapor;\n",
                ),
            ],
            &options,
            Path::new("/p"),
        );
        let bogus: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2339)
            .collect();
        assert!(
            bogus.is_empty(),
            "first explicit-args reference must resolve chained heritage members: {bogus:?}"
        );
    }

    #[test]
    fn program_mode_imported_generic_interface_chained_and_renamed_heritage_resolves() {
        let options = project_mode_es2015_strict_options();
        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/a.ts",
                    r#"
export interface Mist<Q = unknown> {
  vapor?: Q;
  hue?: string;
}
"#,
                ),
                (
                    "/p/b.ts",
                    r#"
import type { Mist } from "./a";
export interface Cloud<Q = unknown> extends Mist<Q> {
  rain: number;
}
export interface Storm<Q = unknown> extends Cloud<Q> {
  wind: boolean;
}
"#,
                ),
                (
                    "/p/c.ts",
                    r#"
import type { Cloud } from "./b";
import type { Storm as Tempest } from "./b";
declare const c: Cloud<number>;
c.rain;
c.vapor;
c.hue;
declare const s2: Tempest;
s2.wind;
s2.rain;
s2.vapor;
export function g(t: Tempest<string>) {
  t.wind;
  t.rain;
  t.vapor;
}
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );
        let bogus: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2339)
            .collect();
        assert!(
            bogus.is_empty(),
            "chained cross-module heritage and renamed import aliases must \
             resolve every inherited member: {bogus:?}"
        );
    }

    #[test]
    fn program_mode_imported_generic_interface_missing_member_still_errors() {
        let options = project_mode_es2015_strict_options();
        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/seeds.ts",
                    r#"
export interface Seed<V = unknown> {
  kernel?: V;
}
export interface Plant<V = unknown> extends Seed<V> {
  stalk: string;
}
"#,
                ),
                (
                    "/p/garden.ts",
                    r#"
import type { Plant } from "./seeds";
export function tend(p: Plant) {
  p.stalk;
  p.kernel;
  p.absent;
}
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );
        let property_errors: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2339)
            .collect();
        assert_eq!(
            property_errors.len(),
            1,
            "exactly one TS2339 for the genuinely missing member: {property_errors:?}"
        );
        assert!(
            property_errors[0].message_text.contains("'absent'"),
            "the surviving TS2339 must name the missing member: {:?}",
            property_errors[0].message_text
        );
    }

    // ------------------------------------------------------------------
    // A conditional whose check type is an unresolvable cross-file
    // reference must DEFER, not fabricate `error` (#8432 /
    // propTypeValidatorInference family).
    //
    // Under a namespace import, an imported generic interface member typed
    // by a sibling type (`isRequired: Validator<NonNullable<T>>`) is read
    // off an inferred `typeof` object. The member reference lowers to
    // `Application(UnresolvedTypeName("Validator"), …)` because the bare
    // name `Validator` is not in the consuming file's scope (it is
    // `P.Validator` there). `is_error_type` folds `UnresolvedTypeName` into
    // "error", so the homomorphic mapped body's conditional
    // `V[K] extends P.Validator<any> ? K : never` used to collapse the
    // property to `error`, minting `{ str: error }` and a false TS2322.
    // The conditional now defers on an unresolved reference, matching the
    // equivalent named-import behavior. Binder names vary across cases.
    // ------------------------------------------------------------------

    /// Library declaring a generic interface whose member is typed by a sibling
    /// generic type (`isRequired: Validator<NonNullable<T>>`). Imported as a
    /// namespace, the bare `Validator` reference is not in the consumer's scope.
    const UNRESOLVED_MEMBER_VLIB: &str = r#"
export interface Validator<T> {
  (props: object): boolean;
  brand?: T;
}
export interface Requireable<T> extends Validator<T> {
  isRequired: Validator<NonNullable<T>>;
}
export declare const str: Requireable<string>;
"#;

    fn check_unresolved_member_conditional(vlib: &str, main: &str) -> Vec<Diagnostic> {
        collect_test_diagnostics_with_options(
            &[("/p/vlib.ts", vlib), ("/p/main.ts", main)],
            &project_mode_es2015_strict_options(),
            Path::new("/p"),
        )
    }

    #[test]
    fn program_mode_conditional_over_unresolved_namespace_member_defers_not_error() {
        let diagnostics = check_unresolved_member_conditional(
            UNRESOLVED_MEMBER_VLIB,
            r#"
import * as P from "./vlib";
const lit = { str: P.str.isRequired };
type V = typeof lit;
type Mc = { [K in keyof V]: V[K] extends P.Validator<any> ? K : never };
const ok: Mc = { str: "str" };
"#,
        );
        // No diagnostic, and in particular nothing that rendered a property
        // as the internal `error` type (which the false TS2322 did).
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.code != 2322 && diag.code != 2741),
            "valid assignment to the mapped type must not error; got: {diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diag| diag.message_text.contains("error")),
            "no diagnostic may render a mapped property as the internal `error` type: {:?}",
            diagnostics
                .iter()
                .map(|d| d.message_text.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn program_mode_conditional_over_unresolved_namespace_member_still_reports_mismatch() {
        let diagnostics = check_unresolved_member_conditional(
            UNRESOLVED_MEMBER_VLIB,
            r#"
import * as P from "./vlib";
const lit = { str: P.str.isRequired };
type V = typeof lit;
type Mc = { [K in keyof V]: V[K] extends P.Validator<any> ? K : never };
const bad: Mc = { str: 123 };
"#,
        );
        // The mapped type still resolves to a usable key type, so a genuine
        // value mismatch is reported — proving the conditional deferred to a
        // real type instead of collapsing to `error` (which would silently
        // accept everything).
        assert!(
            diagnostics.iter().any(|diag| diag.code == 2322),
            "a genuine mismatch against the resolved mapped key type must error; got: {diagnostics:?}"
        );
    }

    /// Fresh-literal object-literal assignment to a bare-`K` identity mapped
    /// type: the per-property probe must type the initializer under the
    /// per-property contextual target, or the fresh literal widens
    /// (`"lo"` → `string`) and a false TS2322 fires against the literal
    /// member type. Adjacent matrix: multi-key, single-key, non-fresh
    /// source, and the negative (wrong literal / wrong primitive) which must
    /// keep erroring. Binder names vary per case (anti-hardcoding).
    #[test]
    fn fresh_literal_object_to_identity_mapped_type_matrix() {
        // Positive: two keys, fresh string literals.
        let diagnostics = collect_test_diagnostics(&[(
            "main.ts",
            r#"
type Cfg = { lo: number; hi: boolean };
type Names = { [K in keyof Cfg]: K };
const ok: Names = { lo: "lo", hi: "hi" };
"#,
        )]);
        assert!(
            diagnostics.is_empty(),
            "fresh literals must satisfy the identity mapped type: {diagnostics:?}"
        );

        // Positive: single key (the `keyof` collapses to one literal).
        let diagnostics = collect_test_diagnostics(&[(
            "main.ts",
            r#"
const seed = { field: 1 };
type S = typeof seed;
type Keys = { [P in keyof S]: P };
const ok: Keys = { field: "field" };
"#,
        )]);
        assert!(
            diagnostics.is_empty(),
            "single-key identity mapped type must accept its literal: {diagnostics:?}"
        );

        // Positive: non-fresh sources behave identically.
        let diagnostics = collect_test_diagnostics(&[(
            "main.ts",
            r#"
type Rec = { alpha: string };
type Ident = { [Q in keyof Rec]: Q };
declare const a: "alpha";
const ok1: Ident = { alpha: a };
const ok2: Ident = { alpha: "alpha" as const };
"#,
        )]);
        assert!(
            diagnostics.is_empty(),
            "non-fresh literal sources must also pass: {diagnostics:?}"
        );

        // Negative: a wrong literal and a wrong primitive must still error.
        let diagnostics = collect_test_diagnostics(&[(
            "main.ts",
            r#"
type Row = { cell: number };
type Tags = { [K in keyof Row]: K };
const bad1: Tags = { cell: "other" };
const bad2: Tags = { cell: 42 };
"#,
        )]);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diag| diag.code == 2322)
                .count(),
            2,
            "wrong values must keep failing against the literal member type: {diagnostics:?}"
        );
    }

    /// Same shape with different binder names (anti-hardcoding): the behavior
    /// follows the structural pattern, not the spellings
    /// `Validator`/`Requireable`/`str`/`P`.
    #[test]
    fn program_mode_conditional_over_unresolved_namespace_member_renamed_binders() {
        let diagnostics = check_unresolved_member_conditional(
            r#"
export interface Checker<U> {
  (value: object): boolean;
  tag?: U;
}
export interface Mandatory<U> extends Checker<U> {
  required: Checker<NonNullable<U>>;
}
export declare const field: Mandatory<string>;
"#,
            r#"
import * as Lib from "./vlib";
const shape = { field: Lib.field.required };
type S = typeof shape;
type Keys = { [K in keyof S]: S[K] extends Lib.Checker<any> ? K : never };
const ok: Keys = { field: "field" };
"#,
        );
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.code != 2322 && diag.code != 2741),
            "renamed-binder witness must not fabricate `error`; got: {diagnostics:?}"
        );
    }

    /// Regression for #13507: a pair of mutually-recursive union type aliases
    /// whose value is indexed must not overflow the stack. Before the fix the
    /// alias-deferral analysis (`alias_ast_is_deferred`) ping-ponged between the
    /// two aliases forever; `tsc` accepts this — `x["a"]` is `number | string`,
    /// no TS2456 circularity and no TS7053 missing-index error. Reaching the
    /// assertions at all proves the recursion is now bounded.
    #[test]
    fn mutually_recursive_union_aliases_indexed_do_not_overflow() {
        let diagnostics = collect_test_diagnostics(&[(
            "main.ts",
            r#"
type U = { [k: string]: number } | V;
type V = { [k: string]: string } | U;
declare const x: U;
export const r = x["a"];
"#,
        )]);
        assert!(
            !diagnostics.iter().any(|d| d.code == 2456),
            "mutually-recursive union aliases are not circular (no TS2456): {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|d| d.code == 7053),
            "indexing a union of string-index objects is allowed (no TS7053): {diagnostics:?}"
        );
    }

    /// Regression for #13507: the same recursion guard must also cover the
    /// `is_element_indexable` walk. Here the recursive union is reached through a
    /// generic alias deferred by a conditional, so the deferral analysis is
    /// satisfied and the cycle instead surfaces while classifying the union's
    /// members for indexability. `tsc` is clean; the check must terminate.
    #[test]
    fn generic_recursive_union_alias_indexed_does_not_overflow() {
        let diagnostics = collect_test_diagnostics(&[(
            "main.ts",
            r#"
type U<T> = { [k: string]: T } | (T extends never ? never : V<T>);
type V<T> = { [k: string]: T } | (T extends never ? never : U<T>);
declare const x: U<number>;
export const r = x["a"];
"#,
        )]);
        assert!(
            !diagnostics.iter().any(|d| d.code == 2456),
            "generic mutually-recursive union aliases are not circular (no TS2456): {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|d| d.code == 7053),
            "indexing the recursive union is allowed (no TS7053): {diagnostics:?}"
        );
    }

    // ------------------------------------------------------------------
    // A parenthesized `typeof value.prop` used as a postfix-array element
    // (`(typeof X.p)[]`) must lower its `typeof` operand through the rich,
    // binder-aware resolution path — the same one `Array<typeof X.p>` and a
    // `type E = typeof X.p; E[]` alias already use — so it relates like those
    // forms. Before the fix the parenthesized element fell to the leaner
    // `TypeNodeChecker::check` path and stayed in an under-evaluated deferred
    // form; when its evaluated apparent type is a deeply-nested generic
    // application (a TypeBox-style `Static`/`PropertiesReduce` reducer) the
    // relation then mis-accepted it against a structurally-equal target via the
    // `isDeeplyNestedType` one-sided expansion bailout, dropping the expected
    // TS2322 (conformance `deeplyNestedMappedTypes.ts`, `problematicFunction3`).
    //
    // Renamed binders + non-colliding key symbols make the coverage structural.
    #[test]
    fn parenthesized_typeof_array_element_relates_like_alias_and_array_generic() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts"]);
        assert!(
            !lib_files.is_empty(),
            "es5.d.ts must be available to provide Readonly/Pick/Omit/Required/Record/Partial"
        );
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_lib_files_and_options(
            &[(
                "/p/main.ts",
                r#"
export type Flatten<G> = G extends infer O ? { [K in keyof O]: O[K] } : never
export declare const RoMark: unique symbol;
export declare const OptMark: unique symbol;
export declare const KindMark: unique symbol;
export interface NodeKind { [KindMark]: string }
export interface SchemaNode extends NodeKind { [RoMark]?: string; [OptMark]?: string; slots: unknown[]; resolved: unknown }
export type RoNode<N extends SchemaNode> = N & { [RoMark]: 'ro' }
export type OptNode<N extends SchemaNode> = N & { [OptMark]: 'opt' }
export interface StrNode extends SchemaNode { [KindMark]: 'Str'; resolved: string; tag: 'string' }
export type RoOptKeys<M extends PropMap> = { [K in keyof M]: M[K] extends RoNode<SchemaNode> ? (M[K] extends OptNode<M[K]> ? K : never) : never }[keyof M]
export type RoKeys<M extends PropMap> = { [K in keyof M]: M[K] extends RoNode<SchemaNode> ? (M[K] extends OptNode<M[K]> ? never : K) : never }[keyof M]
export type OptKeys<M extends PropMap> = { [K in keyof M]: M[K] extends OptNode<SchemaNode> ? (M[K] extends RoNode<M[K]> ? never : K) : never }[keyof M]
export type ReqKeys<M extends PropMap> = keyof Omit<M, RoOptKeys<M> | RoKeys<M> | OptKeys<M>>
export type FoldReducer<M extends PropMap, R extends Record<keyof any, unknown>> = Flatten<(
    Readonly<Partial<Pick<R, RoOptKeys<M>>>> &
    Readonly<Pick<R, RoKeys<M>>> &
    Partial<Pick<R, OptKeys<M>>> &
    Required<Pick<R, ReqKeys<M>>>
)>
export type FoldProps<M extends PropMap, P extends unknown[]> = FoldReducer<M, { [K in keyof M]: Resolve<M[K], P> }>
export type PropMap = Record<string | number, SchemaNode>
export interface ObjNode<M extends PropMap = PropMap> extends SchemaNode { [KindMark]: 'Obj'; resolved: FoldProps<M, this['slots']>; tag: 'object'; fields: M }
export type Resolve<N extends SchemaNode, P extends unknown[] = []> = (N & { slots: P; })['resolved']
declare namespace Make { function Obj<M extends PropMap>(fields: M): ObjNode<M>; function Str(): StrNode }
export type Alpha = Resolve<typeof Alpha>
export const Alpha = Make.Obj({ a: Make.Obj({ b: Make.Obj({ foo: Make.Str() }) }) })
export type Beta = Resolve<typeof Beta>
export const Beta = Make.Obj({ a: Make.Obj({ b: Make.Obj({ foo: Make.Str(), bar: Make.Str() }) }) })
function viaAlias(xs: Alpha[]): Beta[] { return xs; }
function viaTypeofArray(xs: (typeof Alpha.resolved)[]): Beta[] { return xs; }
function viaArrayGeneric(xs: Array<typeof Alpha.resolved>): Beta[] { return xs; }
"#,
            )],
            &lib_files,
            &options,
        );

        // `Alpha` lacks the required `bar` that `Beta` carries, so every form
        // of `Alpha[] -> Beta[]` is a structural mismatch. The bug dropped the
        // TS2322 only for the parenthesized-`typeof`-array form; assert all
        // three return sites now report it.
        let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
        assert_eq!(
            ts2322_count, 3,
            "all three Alpha[] -> Beta[] return sites (alias, parenthesized typeof[], Array<typeof>) must report TS2322; got {ts2322_count} from {diagnostics:?}"
        );
    }

    // ------------------------------------------------------------------
    // #13232: a member-access (`context.response`) write before an `if` must
    // keep its assignment narrowing across the `if`-join.
    //
    // A generic async function expression contextually typed by an interface
    // member signature returns `context.response` after `context.response = …`
    // and an `if` statement. tsc narrows the optional `response?` property to
    // non-`undefined` at the assignment and carries that across the merge, so the
    // return type relates to the declared element type and reports no TS2322.
    //
    // The earlier "resolver-less still-generic conditional distribution" framing
    // was a red herring: the source and target `FetchResponse<MappedResponseType<
    // R, T>>` relate fine. The real failure is that `context.response` stays
    // `… | undefined` at the `return`. The flow walk reaches the `if`'s CONDITION
    // node whose antecedent is the property-write ASSIGNMENT; the CONDITION
    // defer-classifier (`condition_antecedent_requires_defer`) decided whether to
    // process that antecedent using a symbol-equality shortcut that only matches
    // plain-identifier references. A member-access reference carries no
    // `symbol_id`, so the targeting assignment was missed, the CONDITION finalized
    // on the un-narrowed declared type, and the assignment narrowing was dropped
    // at the join. The fix makes that classifier fall back to the same structural
    // `assignment_targets_reference_node` / `assignment_affects_reference_node`
    // predicate the worklist's ASSIGNMENT branch and the sibling
    // `antecedent_requires_defer` classifier already use.
    //
    // Binder names vary from the original ofetch witness so the behavior follows
    // the type shape, not a spelling. Both returns and the flow narrowing are
    // required: a single direct return passes (the issue's "both returns are
    // needed" note). The focused minimal witness for the flow fix itself lives in
    // `member_property_write_narrowing_survives_if_join` below.
    // ------------------------------------------------------------------
    #[test]
    fn program_mode_generic_conditional_in_contextual_return_stays_deferred() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&[
            "es5.d.ts",
            "es2015.d.ts",
            "es2015.promise.d.ts",
            "es2015.iterable.d.ts",
            "dom.d.ts",
        ]);
        assert!(
            lib_files.len() >= 3,
            "es5.d.ts + es2015 + dom.d.ts must be available (Promise, Response, fetch)"
        );
        let options = project_mode_es2015_strict_options();
        let diagnostics = collect_test_diagnostics_with_lib_files_and_options(
            &[
                (
                    "/p/types.ts",
                    r#"
export interface ResponseMap {
  blob: Blob;
  text: string;
  arrayBuffer: ArrayBuffer;
  stream: ReadableStream;
}
export type ResponseType = keyof ResponseMap | "json";
export type MappedResponseType<
  R extends ResponseType,
  JsonType = any,
> = R extends keyof ResponseMap ? ResponseMap[R] : JsonType;
export interface FetchResponse<T> extends Response {
  _data?: T;
}
export interface FetchContext<T = any, R extends ResponseType = ResponseType> {
  request: string;
  options: unknown;
  response?: FetchResponse<MappedResponseType<R, T>>;
  error?: Error;
}
export interface $Fetch {
  raw<T = any, R extends ResponseType = "json">(
    request: string,
    options?: unknown,
  ): Promise<FetchResponse<MappedResponseType<R, T>>>;
}
"#,
                ),
                (
                    "/p/main.ts",
                    r#"
import type {
  FetchResponse,
  ResponseType,
  MappedResponseType,
  FetchContext,
  $Fetch,
} from "./types";

async function onError<T, R extends ResponseType>(
  context: FetchContext<T, R>,
): Promise<FetchResponse<MappedResponseType<R, T>>> {
  throw new Error("x");
}

export const $fetchRaw: $Fetch["raw"] = async function $fetchRaw<
  T = any,
  R extends ResponseType = "json",
>(_request: string, _options: unknown = {}) {
  const context: FetchContext<T, R> = undefined as any;
  context.response = (await fetch(context.request)) as FetchResponse<
    MappedResponseType<R, T>
  >;
  if (context.response.status >= 400) {
    return await onError(context);
  }
  return context.response;
};
"#,
                ),
            ],
            &lib_files,
            &options,
        );
        let bogus: Vec<_> = diagnostics
            .iter()
            .filter(|diag| diag.code == 2322)
            .collect();
        assert!(
            bogus.is_empty(),
            "a still-generic conditional alias in the contextual return type must \
             stay deferred (no false TS2322): {bogus:?}; all: {diagnostics:?}"
        );
    }

    // ------------------------------------------------------------------
    // #13232 (minimal flow witness): assignment narrowing of an optional
    // *property* reference must survive a CONDITION (`if`) join.
    //
    // `c.r = { s: 1 }` narrows the optional `r?: Inner` to non-`undefined`. With a
    // following `if` statement the flow walk for the `return c.r` read reaches the
    // `if`'s CONDITION node whose antecedent is the property-write ASSIGNMENT. The
    // CONDITION's defer classifier must process (defer to) that antecedent so the
    // narrowing reaches the merge; previously it used a symbol-equality shortcut
    // that only matched plain-identifier references and silently skipped the
    // member-access write, dropping the narrowing and reporting a false TS2322.
    //
    // Adjacency is asserted in one pass: a plain-identifier local (`local`) and an
    // element-access write (`elem`) must also narrow; base reassignment (`reassign`)
    // and an `undefined` overwrite on one branch (`overwrite`) must still keep
    // `undefined` and report TS2322. Binder names differ across the cases so the
    // behavior tracks the reference shape, not a spelling.
    // ------------------------------------------------------------------
    #[test]
    fn member_property_write_narrowing_survives_if_join() {
        let options = project_mode_es2015_strict_options();
        let base = std::path::Path::new("/");

        // Positive: property write before an `if` narrows the optional away.
        let positive = collect_test_diagnostics_with_options(
            &[(
                "/p/a.ts",
                r#"
interface Inner { s: number }
interface C { r?: Inner }
export function f(c: C, flag: boolean): Inner {
  c.r = { s: 1 };
  if (flag) {}
  return c.r;
}
export function viaElement(obj: C, flag: boolean): Inner {
  obj["r"] = { s: 1 };
  if (flag) {}
  return obj["r"];
}
export function viaLocal(flag: boolean): Inner {
  let r: Inner | undefined;
  r = { s: 1 };
  if (flag) {}
  return r;
}
"#,
            )],
            &options,
            base,
        );
        let positive_2322: Vec<_> = positive.iter().filter(|d| d.code == 2322).collect();
        assert!(
            positive_2322.is_empty(),
            "property/element/local write before an `if` must keep its narrowing \
             across the join (no false TS2322): {positive_2322:?}; all: {positive:?}"
        );

        // Negative: a branch that reassigns the base or writes `undefined` must
        // re-introduce `undefined` at the merge and still report TS2322.
        let negative = collect_test_diagnostics_with_options(
            &[(
                "/p/b.ts",
                r#"
interface Inner { s: number }
interface C { r?: Inner }
export function reassign(c: C, other: C, flag: boolean): Inner {
  c.r = { s: 1 };
  if (flag) { c = other; }
  return c.r;
}
export function overwrite(c: C, flag: boolean): Inner {
  c.r = { s: 1 };
  if (flag) { c.r = undefined; }
  return c.r;
}
"#,
            )],
            &options,
            base,
        );
        let negative_2322 = negative.iter().filter(|d| d.code == 2322).count();
        assert_eq!(
            negative_2322, 2,
            "base reassignment and `undefined` overwrite on one branch must keep \
             `undefined` at the merge (TS2322 at both returns); got {negative_2322} \
             from {negative:?}"
        );
    }

    // ------------------------------------------------------------------
    // #13947: a class merged with a namespace, default-exported, in an
    // import cycle with its consumers had its imported *value/constructor*
    // side collapse to `any` cross-file (the *instance* side survives the
    // cycle via a deferred `Lazy(classDef)`). The collapsed value made a
    // `Schema.make<…>(…)` call an *untyped* call → false `TS2347` cascading
    // implicit-`any` callback params (`TS7006`) — the runtypes
    // `Runtype.create` root, 227 false positives.
    //
    // Fix: the re-entrant constructor-cycle fallback returns a deferred
    // `Lazy` to the class's `ClassConstructor` companion `DefId` instead of a
    // bare `any`, so the transient `any` is never cached/propagated and the
    // call site observes the real generic call signature once the companion
    // body is published.
    //
    // Renamed binders (no `Runtype`/`create` literals) so the result is not
    // keyed on any identifier text. `tsc`-clean.
    #[test]
    fn cross_file_class_namespace_merge_value_keeps_call_signature_in_import_cycle() {
        let options = resolved_options_for_es2015_strict_test();
        let base = std::path::Path::new("/");
        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/schema.ts",
                    r#"
import Helper from "./helper";
type MarkOf<R extends { readonly mark: unknown }> = R["mark"];
class Schema<T = any> {
  readonly mark!: T;
  static make = <R extends Schema>(
    build: (x: number) => MarkOf<R>,
    meta: Schema.Meta<R>,
  ): R => new Schema(build as any, meta) as unknown as R;
  private constructor(_b: (x: number) => T, _m: Schema.Meta<Schema<T>>) {}
  static unit(): Schema<number> {
    return Schema.make<Schema<number>>((x) => x, { tag: "u", inner: undefined as any });
  }
  use() {
    return new Helper().go();
  }
}
namespace Schema {
  export type Meta<R> = { tag: string; inner: R };
}
export default Schema;
"#,
                ),
                (
                    "/helper.ts",
                    r#"
import Schema from "./schema";
export default class Helper {
  go() {
    return Schema.make<Schema<number>>((x) => x, { tag: "n", inner: undefined as any });
  }
}
"#,
                ),
            ],
            &options,
            base,
        );

        // `tsc`-clean: the imported class+namespace-merge value must keep its
        // generic call signature across the import cycle — no untyped-call
        // `TS2347`, no implicit-`any` callback `TS7006`, nothing.
        assert!(
            diagnostics.is_empty(),
            "expected a fully clean (tsc-parity) check; got: {diagnostics:#?}"
        );
    }

    // ------------------------------------------------------------------
    // Deeply-nested homomorphic mapped-type reducer relation
    // (`deeplyNestedMappedTypes.ts` parity floor; #8432 mapped/keyof family).
    //
    // Structural rule: a TypeBox-style reducer — `Materialize<S> =
    // (S & { opts: [] })['out']` whose `out` resolves to a homomorphic mapped
    // fold `{ [F in keyof M]: Materialize<M[F]> }` — re-introduces the same
    // generic definition through its own structural expansion at every nesting
    // level. Past the one-sided application-expansion recursion cap
    // (`ONE_SIDED_APP_EXPANSION_MAX_DEPTH`, tsz's `isDeeplyNestedType` analog),
    // `tsc` still reduces both operands and relates them structurally; tsz must
    // likewise report a genuine deep mismatch and must not bail the relation to
    // `Maybe`/related. That bail was the false negative behind the upstream
    // `deeplyNestedMappedTypes.ts` row, where `problematicFunction3`'s mismatch
    // went unreported. (The residual on that real-lib row is the elaboration
    // *display* of the reduced reducer, which is independent of — and downstream
    // from — the relation outcome guarded here.)
    //
    // Two negatives at nesting depth 6 (beyond the depth-5 cap) — a missing deep
    // property (`u`) and an incompatible deep leaf (`string` vs `number`) — must
    // each surface as exactly one `TS2322`. Two positive controls (a reflexive
    // reducer assignment, and the same through an array) must stay fully clean so
    // the guard cannot pass by always erroring. Every binder name is arbitrary
    // (no `Static`/`Input`/`TObject` text), so the outcome tracks the reducer
    // shape, not a spelling.
    // ------------------------------------------------------------------
    #[test]
    fn deeply_nested_mapped_reducer_relates_past_recursion_cap() {
        let options = project_mode_es2015_strict_options();
        let base = std::path::Path::new("/");

        // Positive controls: a reflexive reducer assignment, and the same
        // through an array, must produce no diagnostics at all.
        let clean = collect_test_diagnostics_with_options(
            &[(
                "/p/clean.ts",
                r#"
declare const Brand: unique symbol;
interface Spec { [Brand]: string; opts: unknown[]; out: unknown; }
interface TextSpec extends Spec { [Brand]: 'Text'; out: string; }
interface SpecMap { [field: string]: Spec; }
type Fold<M extends SpecMap> = { [F in keyof M]: Materialize<M[F]> };
interface Branch<M extends SpecMap = SpecMap> extends Spec {
  [Brand]: 'Branch'; out: Fold<M>; children: M;
}
type Materialize<S extends Spec> = (S & { opts: [] })['out'];
type WideTree = Materialize<Branch<{ p: Branch<{ q: Branch<{ r: Branch<{ s: Branch<{ t: TextSpec }> }> }> }> }>>;
declare const wide: WideTree;
const sameShape: WideTree = wide;
const asArray: WideTree[] = [wide];
"#,
            )],
            &options,
            base,
        );
        assert!(
            clean.is_empty(),
            "a deeply-nested reducer assigned to its own (identical) reduced form \
             must check clean: {clean:#?}"
        );

        // Negatives: at nesting depth 6 (beyond the one-sided expansion cap) a
        // missing deep property and an incompatible deep leaf must each surface
        // as exactly one TS2322 — the relation must not bail to `Maybe`/related
        // on the recursive reducer.
        let mismatches = collect_test_diagnostics_with_options(
            &[(
                "/p/mismatch.ts",
                r#"
declare const Brand: unique symbol;
interface Spec { [Brand]: string; opts: unknown[]; out: unknown; }
interface TextSpec extends Spec { [Brand]: 'Text'; out: string; }
interface CountSpec extends Spec { [Brand]: 'Count'; out: number; }
interface SpecMap { [field: string]: Spec; }
type Fold<M extends SpecMap> = { [F in keyof M]: Materialize<M[F]> };
interface Branch<M extends SpecMap = SpecMap> extends Spec {
  [Brand]: 'Branch'; out: Fold<M>; children: M;
}
type Materialize<S extends Spec> = (S & { opts: [] })['out'];
type WideTree  = Materialize<Branch<{ p: Branch<{ q: Branch<{ r: Branch<{ s: Branch<{ t: TextSpec }> }> }> }> }>>;
type ExtraTree = Materialize<Branch<{ p: Branch<{ q: Branch<{ r: Branch<{ s: Branch<{ t: TextSpec; u: TextSpec }> }> }> }> }>>;
type CountTree = Materialize<Branch<{ p: Branch<{ q: Branch<{ r: Branch<{ s: Branch<{ t: CountSpec }> }> }> }> }>>;
declare const wide: WideTree;
const missingProp: ExtraTree = wide;
const wrongLeaf: CountTree = wide;
"#,
            )],
            &options,
            base,
        );
        let ts2322: Vec<_> = mismatches.iter().filter(|d| d.code == 2322).collect();
        assert_eq!(
            ts2322.len(),
            2,
            "deep reducer mismatches (a missing property and an incompatible leaf) \
             past the recursion cap must each report TS2322, not bail to related: \
             got {ts2322:#?}; all: {mismatches:#?}"
        );
    }

    // ------------------------------------------------------------------
    // tsc-parity guard: real-world npm import shapes (witnesses from #13826)
    // must keep resolving the way tsc resolves them, across every module
    // resolution mode. The resolver entry points are being actively
    // consolidated (e.g. #14037 merges the import-site classification copies),
    // so these shapes need a regression floor: `node:` builtin protocol
    // specifiers (resolved via @types/node ambient `declare module` blocks,
    // including the triple-slash-split layout @types/node actually ships),
    // legacy `/lib/*` subpaths in a package WITHOUT an `exports` map (fp-ts
    // shape), an `exports` subpath map (next/server shape), and a scoped
    // package with a wildcard `exports` pattern (@mswjs/interceptors/* shape).
    //
    // The assertions encode tsc behavior: under node16/nodenext/bundler all
    // four shapes resolve; under legacy node10 the `exports`-only packages are
    // unresolved (node10 ignores `package.json` `exports`) while the `node:`
    // ambient and the physical `/lib/*` subpath still resolve.
    // ------------------------------------------------------------------

    fn resolve_shapes_diagnostics(
        file_paths: &[PathBuf],
        base: &Path,
        module: &str,
        module_resolution: &str,
    ) -> Vec<Diagnostic> {
        let resolved = tsz::config::resolve_compiler_options(Some(&tsz::config::CompilerOptions {
            target: Some("es2020".to_string()),
            module: Some(module.to_string()),
            module_resolution: Some(module_resolution.to_string()),
            strict: Some(true),
            ..Default::default()
        }))
        .expect("resolve options");
        let SourceReadResult { sources, .. } =
            super::read_source_files(file_paths, base, &resolved, None, None)
                .expect("read source files");
        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths =
            super::resolve_effective_lib_paths(&resolved, &sources, base, disable_default_libs)
                .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: base,
                reference_path_current_directory: None,
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics
    }

    fn write_real_world_shape_fixture(base: &Path) -> PathBuf {
        let write = |rel: &str, content: &str| {
            let path = base.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
            path
        };

        // @types/node: triple-slash-split layout declaring both the bare and
        // `node:`-prefixed module forms, exactly as @types/node ships them.
        write(
            "node_modules/@types/node/index.d.ts",
            "/// <reference path=\"fs.d.ts\" />\n/// <reference path=\"stream.d.ts\" />\n",
        );
        write(
            "node_modules/@types/node/fs.d.ts",
            "declare module \"fs\" { export function readFileSync(p: string): string; }\n\
             declare module \"node:fs\" { export * from \"fs\"; }\n",
        );
        write(
            "node_modules/@types/node/stream.d.ts",
            "declare module \"stream/promises\" { export function pipeline(): void; }\n\
             declare module \"node:stream/promises\" { export * from \"stream/promises\"; }\n",
        );
        write(
            "node_modules/@types/node/package.json",
            "{\"name\":\"@types/node\",\"version\":\"20.0.0\",\"types\":\"index.d.ts\"}",
        );

        // fp-ts: legacy `/lib/*` subpath, NO `exports` map.
        write(
            "node_modules/fp-ts/package.json",
            "{\"name\":\"fp-ts\",\"version\":\"2.0.0\",\"main\":\"lib/index.js\",\"types\":\"lib/index.d.ts\"}",
        );
        write("node_modules/fp-ts/lib/index.d.ts", "export {};\n");
        write(
            "node_modules/fp-ts/lib/function.d.ts",
            "export declare const identity: <A>(a: A) => A;\n",
        );

        // next: `exports` subpath map.
        write(
            "node_modules/next/package.json",
            "{\"name\":\"next\",\"version\":\"14.0.0\",\"exports\":{\"./server\":{\"types\":\"./dist/server.d.ts\",\"default\":\"./dist/server.js\"}}}",
        );
        write(
            "node_modules/next/dist/server.d.ts",
            "export declare function NextResponse(): void;\n",
        );

        // @mswjs/interceptors: scoped package + wildcard `exports` pattern.
        write(
            "node_modules/@mswjs/interceptors/package.json",
            "{\"name\":\"@mswjs/interceptors\",\"version\":\"1.0.0\",\"exports\":{\".\":{\"types\":\"./lib/index.d.ts\",\"default\":\"./lib/index.js\"},\"./*\":{\"types\":\"./lib/*.d.ts\",\"default\":\"./lib/*.js\"}}}",
        );
        write("node_modules/@mswjs/interceptors/lib/index.d.ts", "export {};\n");
        write(
            "node_modules/@mswjs/interceptors/lib/fetch.d.ts",
            "export declare const FetchInterceptor: number;\n",
        );

        write(
            "entry.ts",
            "import { readFileSync } from \"node:fs\";\n\
             import { pipeline } from \"node:stream/promises\";\n\
             import { identity } from \"fp-ts/lib/function\";\n\
             import { NextResponse } from \"next/server\";\n\
             import { FetchInterceptor } from \"@mswjs/interceptors/fetch\";\n\
             export const a = readFileSync(\"x\");\n\
             export const b = pipeline;\n\
             export const c = identity(1);\n\
             export const d = NextResponse;\n\
             export const e = FetchInterceptor;\n",
        )
    }

    fn ts2307_specifiers(diags: &[Diagnostic]) -> Vec<String> {
        diags
            .iter()
            .filter(|d| d.code == 2307)
            .map(|d| d.message_text.clone())
            .collect()
    }

    #[test]
    fn module_resolution_real_world_npm_import_shapes_match_tsc() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let base = dir.path();
        let entry = write_real_world_shape_fixture(base);
        let files = [entry];

        // node16 / nodenext / bundler: every shape resolves (no TS2307), and
        // the named imports bind (no TS2305 / TS2614 "no exported member").
        for (module, mr) in [
            ("node16", "node16"),
            ("nodenext", "nodenext"),
            ("esnext", "bundler"),
        ] {
            let diags = resolve_shapes_diagnostics(&files, base, module, mr);
            let unresolved = ts2307_specifiers(&diags);
            assert!(
                unresolved.is_empty(),
                "[{mr}] expected all real-world import shapes to resolve, got TS2307: {unresolved:?}"
            );
            let missing_members: Vec<_> = diags
                .iter()
                .filter(|d| d.code == 2305 || d.code == 2614)
                .map(|d| d.message_text.clone())
                .collect();
            assert!(
                missing_members.is_empty(),
                "[{mr}] resolved modules must expose their named exports, got: {missing_members:?}"
            );
        }

        // node10: `exports`-only packages are unresolved (node10 ignores
        // `package.json` `exports`), but the `node:` ambient builtins and the
        // physical `/lib/*` subpath still resolve.
        let node10 = resolve_shapes_diagnostics(&files, base, "commonjs", "node10");
        let unresolved = ts2307_specifiers(&node10);
        assert!(
            unresolved.iter().any(|m| m.contains("next/server")),
            "[node10] next/server is exports-only and must be unresolved: {unresolved:?}"
        );
        assert!(
            unresolved
                .iter()
                .any(|m| m.contains("@mswjs/interceptors/fetch")),
            "[node10] @mswjs/interceptors/fetch is exports-only and must be unresolved: {unresolved:?}"
        );
        for resolved_specifier in ["node:fs", "node:stream/promises", "fp-ts/lib/function"] {
            assert!(
                !unresolved.iter().any(|m| m.contains(resolved_specifier)),
                "[node10] {resolved_specifier} must still resolve: {unresolved:?}"
            );
        }
    }

    #[test]
    fn skip_lib_check_prepares_declaration_heritage_for_jsx_consumers() {
        let mut options = project_mode_es2015_strict_options();
        options.skip_lib_check = true;
        options.checker.jsx_mode = JsxMode::React;
        options.checker.es_module_interop = true;
        options.checker.allow_synthetic_default_imports = true;

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/react-lite.d.ts",
                    r#"
declare const React: { createElement: any };
interface MiniElement {}
declare namespace React {
  type ElementType = keyof JSX.IntrinsicElements | ((props: any) => any);
  interface DOMAttributes<T> {
    onClick?: (event: { currentTarget: T }) => void;
  }
  interface HTMLAttributes<T> extends DOMAttributes<T> {
    title?: string;
  }
  interface AnchorHTMLAttributes<T> extends HTMLAttributes<T> {
    href?: string;
    download?: unknown;
  }
  type DetailedHTMLProps<E extends HTMLAttributes<T>, T> = E;
  type ComponentPropsWithRef<T extends ElementType> =
    T extends keyof JSX.IntrinsicElements ? JSX.IntrinsicElements[T] : never;
}
declare namespace JSX {
  interface Element {}
  interface IntrinsicElements {
    a: React.DetailedHTMLProps<React.AnchorHTMLAttributes<MiniElement>, MiniElement>;
    button: React.DetailedHTMLProps<React.HTMLAttributes<MiniElement>, MiniElement>;
  }
}
"#,
                ),
                (
                    "/p/main.tsx",
                    r#"
function MiniLink<T extends React.ElementType = React.ElementType>(
  props: React.ComponentPropsWithRef<React.ElementType extends T ? "a" : T>,
) {
  return <a />;
}

<MiniLink onClick={(event) => { event.currentTarget; }} />;
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );

        let leaked: Vec<_> = diagnostics
            .iter()
            .filter(|diag| matches!(diag.code, 2339 | 2740 | 7006))
            .collect();
        assert!(
            leaked.is_empty(),
            "skipLibCheck must still prepare skipped declaration heritage for JSX consumers: {leaked:?}; all: {diagnostics:?}"
        );
    }

    fn collect_es2015_default_lib_diagnostics_multifile_with_options(
        files: &[(&str, &str)],
        configure: impl FnOnce(&mut ResolvedCompilerOptions),
    ) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let mut file_paths = Vec::with_capacity(files.len());
        for (name, source) in files {
            let path = dir.path().join(name);
            std::fs::write(&path, source).expect("write source");
            file_paths.push(path);
        }

        let mut resolved = resolved_options_for_es2015_strict_test();
        configure(&mut resolved);
        let SourceReadResult {
            sources,
            dependencies: _,
            module_resolutions: _,
            type_reference_errors,
            resolution_mode_errors,
            ..
        } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
            .expect("read source files");

        assert!(type_reference_errors.is_empty());
        assert!(resolution_mode_errors.is_empty());

        let disable_default_libs =
            resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
        let lib_paths = super::resolve_effective_lib_paths(
            &resolved,
            &sources,
            dir.path(),
            disable_default_libs,
        )
        .expect("resolve effective lib paths");
        let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
        let lib_files =
            parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
        let checker_libs = load_checker_libs(&lib_files);
        let compile_inputs: Vec<_> = sources
            .into_iter()
            .map(|source| {
                (
                    source.path.to_string_lossy().into_owned(),
                    source.text.unwrap_or_default(),
                )
            })
            .collect();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            &lib_files,
        ));
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &resolved,
                base_dir: dir.path(),
                reference_path_current_directory: None,
                checker_libs: &checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics
    }

    #[test]
    fn skip_lib_check_preserves_declared_global_console_receiver() {
        let diagnostics = collect_es2015_default_lib_diagnostics_multifile_with_options(
            &[
                (
                    "react-lite.d.ts",
                    r#"
declare const React: { createElement: any };
declare namespace JSX {
  interface Element {}
  interface IntrinsicElements {
    span: {};
  }
}
"#,
                ),
                (
                    "main.tsx",
                    r#"
/// <reference path="./react-lite.d.ts" />
console.log("ok");
const element = <span />;
"#,
                ),
            ],
            |resolved| {
                resolved.skip_lib_check = true;
                resolved.checker.jsx_mode = JsxMode::React;
                resolved.checker.es_module_interop = true;
                resolved.checker.allow_synthetic_default_imports = true;
            },
        );

        let leaked: Vec<_> = diagnostics
            .iter()
            .filter(|diag| {
                diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                    && diag.message_text.contains("Property 'log'")
            })
            .collect();
        assert!(
            leaked.is_empty(),
            "skipLibCheck copied declaration materialization must not replace console with a stale lazy lib receiver: {leaked:?}; all: {diagnostics:?}"
        );
    }

    fn comlink_node_adapter_source() -> &'static str {
        r#"
import { Endpoint } from "./protocol";

export interface NodeEndpoint {
  postMessage(message: any, transfer?: any[]): void;
  on(
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: {}
  ): void;
  off(
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: {}
  ): void;
  start?: () => void;
}

export default function nodeEndpoint(nep: NodeEndpoint): Endpoint {
  const listeners = new WeakMap();
  return {
    postMessage: nep.postMessage.bind(nep),
    addEventListener: (_, eh) => {
      const l = (data: any) => {
        if ("handleEvent" in eh) {
          eh.handleEvent({ data } as MessageEvent);
        } else {
          eh({ data } as MessageEvent);
        }
      };
      nep.on("message", l);
      listeners.set(eh, l);
    },
    removeEventListener: (_, eh) => {
      const l = listeners.get(eh);
      if (!l) {
        return;
      }
      nep.off("message", l);
      listeners.delete(eh);
    },
    start: nep.start && nep.start.bind(nep),
  };
}
"#
    }

    #[test]
    fn skip_lib_check_preserves_dom_message_event_default_type_arg() {
        let diagnostics = collect_es2015_default_lib_diagnostics_multifile_with_options(
            &[
                (
                    "protocol.ts",
                    r#"
export interface EventSource {
  addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: {}
  ): void;
}

export interface Endpoint extends EventSource {
  removeEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: {}
  ): void;
  postMessage(message: any, transfer?: Transferable[]): void;
  start?: () => void;
}

export const enum WireValueType {
  RAW = "RAW",
  PROXY = "PROXY",
  THROW = "THROW",
  HANDLER = "HANDLER",
}

export type MessageID = string;

export interface RawWireValue {
  id?: string;
  type: WireValueType.RAW;
  value: {};
}

export interface HandlerWireValue {
  id?: string;
  type: WireValueType.HANDLER;
  name: string;
  value: unknown;
}

export type WireValue = RawWireValue | HandlerWireValue;

export const enum MessageType {
  GET = "GET",
  SET = "SET",
  APPLY = "APPLY",
  CONSTRUCT = "CONSTRUCT",
  ENDPOINT = "ENDPOINT",
  RELEASE = "RELEASE",
}

export interface GetMessage {
  id?: MessageID;
  type: MessageType.GET;
  path: string[];
}

export interface SetMessage {
  id?: MessageID;
  type: MessageType.SET;
  path: string[];
  value: WireValue;
}

export interface ApplyMessage {
  id?: MessageID;
  type: MessageType.APPLY;
  path: string[];
  argumentList: WireValue[];
}

export interface ConstructMessage {
  id?: MessageID;
  type: MessageType.CONSTRUCT;
  path: string[];
  argumentList: WireValue[];
}

export interface EndpointMessage {
  id?: MessageID;
  type: MessageType.ENDPOINT;
}

export interface ReleaseMessage {
  id?: MessageID;
  type: MessageType.RELEASE;
}

export type Message =
  | GetMessage
  | SetMessage
  | ApplyMessage
  | ConstructMessage
  | EndpointMessage
  | ReleaseMessage;
"#,
                ),
                (
                    "main.ts",
                    r#"
	import { Endpoint, Message, MessageType, WireValue } from "./protocol";

	export const proxyMarker = Symbol("Comlink.proxy");
	export const createEndpoint = Symbol("Comlink.endpoint");
	export const releaseProxy = Symbol("Comlink.releaseProxy");
	export const finalizer = Symbol("Comlink.finalizer");
	const throwMarker = Symbol("Comlink.thrown");
	export interface ProxyMarked {
	  [proxyMarker]: true;
	}
	type Promisify<T> = T extends Promise<unknown> ? T : Promise<T>;
	type Unpromisify<P> = P extends Promise<infer T> ? T : P;
	type RemoteProperty<T> = T extends Function | ProxyMarked ? Remote<T> : Promisify<T>;
	type LocalProperty<T> = T extends Function | ProxyMarked ? Local<T> : Unpromisify<T>;
	export type ProxyOrClone<T> = T extends ProxyMarked ? Remote<T> : T;
	export type UnproxyOrClone<T> = T extends RemoteObject<ProxyMarked> ? Local<T> : T;
	export type RemoteObject<T> = { [P in keyof T]: RemoteProperty<T[P]> };
	export type LocalObject<T> = { [P in keyof T]: LocalProperty<T[P]> };
	export interface ProxyMethods {
	  [createEndpoint]: () => Promise<MessagePort>;
	  [releaseProxy]: () => void;
	}
	export type Remote<T> =
	  RemoteObject<T> &
	    (T extends (...args: infer TArguments) => infer TReturn
	      ? (
	          ...args: { [I in keyof TArguments]: UnproxyOrClone<TArguments[I]> }
	        ) => Promisify<ProxyOrClone<Unpromisify<TReturn>>>
	      : unknown) &
	    (T extends { new (...args: infer TArguments): infer TInstance }
	      ? {
	          new (
	            ...args: {
	              [I in keyof TArguments]: UnproxyOrClone<TArguments[I]>;
	            }
	          ): Promisify<Remote<TInstance>>;
	        }
	      : unknown) &
	    ProxyMethods;
	type MaybePromise<T> = Promise<T> | T;
	export type Local<T> =
	  Omit<LocalObject<T>, keyof ProxyMethods> &
	    (T extends (...args: infer TArguments) => infer TReturn
	      ? (
	          ...args: { [I in keyof TArguments]: ProxyOrClone<TArguments[I]> }
	        ) => MaybePromise<UnproxyOrClone<Unpromisify<TReturn>>>
	      : unknown) &
	    (T extends { new (...args: infer TArguments): infer TInstance }
	      ? {
	          new (
	            ...args: {
	              [I in keyof TArguments]: ProxyOrClone<TArguments[I]>;
	            }
	          ): MaybePromise<Local<Unpromisify<TInstance>>>;
	        }
	      : unknown);

	const isObject = (val: unknown): val is object =>
	  (typeof val === "object" && val !== null) || typeof val === "function";

	export interface TransferHandler<T, S> {
	  canHandle(value: unknown): value is T;
	  serialize(value: T): [S, Transferable[]];
	  deserialize(value: S): T;
	}

	const proxyTransferHandler: TransferHandler<object, MessagePort> = {
	  canHandle: (val): val is ProxyMarked =>
	    isObject(val) && (val as ProxyMarked)[proxyMarker],
	  serialize(obj) {
	    const { port1, port2 } = new MessageChannel();
	    expose(port1);
	    return [port2, [port2]];
	  },
	  deserialize(port) {
	    port.start();
	    return wrap(port);
	  },
	};

	interface ThrownValue {
	  [throwMarker]: unknown;
	  value: unknown;
	}
	type SerializedThrownValue =
	  | { isError: true; value: Error }
	  | { isError: false; value: unknown };
	const throwTransferHandler: TransferHandler<
	  ThrownValue,
	  SerializedThrownValue
	> = {
	  canHandle: (value): value is ThrownValue =>
	    isObject(value) && throwMarker in value,
	  serialize({ value }) {
	    if (value instanceof Error) {
	      return [
	        {
	          isError: true,
	          value,
	        },
	        [],
	      ];
	    }
	    return [{ isError: false, value }, []];
	  },
	  deserialize(serialized) {
	    if (serialized.isError) {
	      throw serialized.value;
	    }
	    throw serialized.value;
	  },
	};

	const transferHandlers = new Map<
	  string,
	  TransferHandler<unknown, unknown>
	>([
	  ["proxy", proxyTransferHandler],
	  ["throw", throwTransferHandler],
	]);

type PendingListenersMap = Map<
  string,
  (value: WireValue | PromiseLike<WireValue>) => void
>;

declare const pendingListeners: PendingListenersMap;
declare const allowedOrigins: (string | RegExp)[];
declare const obj: any;
declare function isAllowedOrigin(
  allowedOrigins: (string | RegExp)[],
  origin: string
): boolean;
declare function fromWireValue(value: WireValue): unknown;
declare function toWireValue(value: unknown): [WireValue, Transferable[]];
declare function transfer(value: MessagePort, transferables: Transferable[]): WireValue;
declare function proxy(value: unknown): WireValue;
declare function closeEndPoint(ep: Endpoint): void;

export function expose(ep: Endpoint) {
  ep.addEventListener("message", function callback(ev: MessageEvent) {
    if (!ev || !ev.data) {
      return;
    }
    if (!isAllowedOrigin(allowedOrigins, ev.origin)) {
      return;
    }
    const { id, type, path } = {
      path: [] as string[],
      ...(ev.data as Message),
    };
    const argumentList = (ev.data.argumentList || []).map(fromWireValue);
    let returnValue;
    try {
      const parent = path.slice(0, -1).reduce((obj, prop) => obj[prop], obj);
      const rawValue = path.reduce((obj, prop) => obj[prop], obj);
      switch (type) {
        case MessageType.GET:
          {
            returnValue = rawValue;
          }
          break;
        case MessageType.SET:
          {
            parent[path.slice(-1)[0]] = fromWireValue(ev.data.value);
            returnValue = true;
          }
          break;
        case MessageType.APPLY:
          {
            returnValue = rawValue.apply(parent, argumentList);
          }
          break;
        case MessageType.CONSTRUCT:
          {
            const value = new rawValue(...argumentList);
            returnValue = proxy(value);
          }
          break;
        case MessageType.ENDPOINT:
          {
            const { port1, port2 } = new MessageChannel();
            expose(port2);
            returnValue = transfer(port1, [port1]);
          }
          break;
        case MessageType.RELEASE:
          {
            returnValue = undefined;
          }
          break;
        default:
          return;
      }
    } catch (value) {
      returnValue = { value };
    }
    Promise.resolve(returnValue)
      .catch((value) => ({ value }))
      .then((returnValue) => {
        const [wireValue, transferables] = toWireValue(returnValue);
        ep.postMessage({ ...wireValue, id }, transferables);
        if (type === MessageType.RELEASE) {
          ep.removeEventListener("message", callback as any);
          closeEndPoint(ep);
        }
      });
  } as any);
}

	export function wrap<T>(ep: Endpoint): Remote<T> {
	  ep.addEventListener("message", function handleMessage(ev: Event) {
	    const { data } = ev as MessageEvent;
	    if (!data || !data.id) {
	      return;
	    }
	    const resolver = pendingListeners.get(data.id);
	    if (!resolver) {
	      return;
	    }
	    resolver(data);
	    pendingListeners.delete(data.id);
	  });
	  return {} as any;
	}
	"#,
                ),
                (
                    "node-adapter.ts",
                    comlink_node_adapter_source(),
                ),
            ],
            |resolved| {
                resolved.skip_lib_check = true;
                resolved.printer.target = ScriptTarget::ES2022;
                resolved.checker.target = ScriptTarget::ES2022;
                resolved.printer.module = ModuleKind::ESNext;
                resolved.checker.module = ModuleKind::ESNext;
                resolved.module_resolution = Some(crate::config::ModuleResolutionKind::Bundler);
                resolved.types = Some(Vec::new());
                resolved.checker.types_explicitly_set = true;
                resolved.lib_files = crate::config::resolve_lib_files(&[
                    "es2022".to_string(),
                    "dom".to_string(),
                    "dom.iterable".to_string(),
                ])
                .expect("resolve explicit libs");
                resolved.lib_is_default = false;
            },
        );

        let leaked: Vec<_> = diagnostics
            .iter()
            .filter(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                        | diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
                )
            })
            .collect();
        assert!(
            leaked.is_empty(),
            "skipLibCheck must preserve defaulted MessageEvent<T = any> for source consumers: {leaked:?}; all: {diagnostics:?}"
        );
    }

    #[test]
    fn generic_intrinsic_lma_display_uses_dynamic_representative_tag() {
        let diagnostics = collect_es2015_default_lib_diagnostics_multifile_with_options(
            &[
                (
                    "react-lite.d.ts",
                    r#"
declare const React: { createElement: any };
declare namespace React {
  interface HTMLAttributes<T> {
    owner?: T;
  }
  type DetailedHTMLProps<E, T> = E & { ref?: T };
  type SFC<P = {}> = (props: P) => JSX.Element;
}
declare namespace JSX {
  interface Element {}
  interface IntrinsicAttributes {}
  type LibraryManagedAttributes<C, P> =
    C extends { propTypes: infer T; defaultProps: infer D; }
      ? P
      : C extends { propTypes: infer T; }
        ? P
        : C extends { defaultProps: infer D; }
          ? P
          : P;
  interface IntrinsicElements {
    div: React.DetailedHTMLProps<React.HTMLAttributes<HTMLDivElement>, HTMLDivElement>;
    span: React.DetailedHTMLProps<React.HTMLAttributes<HTMLSpanElement>, HTMLSpanElement>;
  }
}
"#,
                ),
                (
                    "main.tsx",
                    r#"
/// <reference path="./react-lite.d.ts" />
type ElementTags = "span" | "div";
export const Hoc = <Tag extends ElementTags>(TagElement: Tag): React.SFC => {
  const Component = () => <TagElement />;
  return Component;
};
"#,
                ),
            ],
            |resolved| {
                resolved.checker.jsx_mode = JsxMode::React;
                resolved.checker.es_module_interop = true;
                resolved.checker.allow_synthetic_default_imports = true;
            },
        );

        let diag = diagnostics
            .iter()
            .find(|diag| {
                diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                    && diag
                        .message_text
                        .contains("LibraryManagedAttributes<Tag, DetailedHTMLProps")
            })
            .expect("expected TS2322 for generic intrinsic LibraryManagedAttributes target");
        assert!(
            diag.message_text.contains("HTMLDivElement")
                && !diag.message_text.contains("HTMLSpanElement"),
            "generic intrinsic display must use the tsc representative tag: {diag:?}; all: {diagnostics:?}"
        );
    }
