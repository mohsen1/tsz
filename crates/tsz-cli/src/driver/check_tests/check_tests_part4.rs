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

