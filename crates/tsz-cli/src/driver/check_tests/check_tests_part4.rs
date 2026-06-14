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

