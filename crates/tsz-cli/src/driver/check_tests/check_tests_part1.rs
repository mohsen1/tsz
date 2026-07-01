    use super::*;
    use crate::args::CliArgs;
    use std::fs;
    use std::path::PathBuf;
    use tsz_common::common::ModuleKind;

    fn collect_test_diagnostics(files: &[(&str, &str)]) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &ResolvedCompilerOptions::default(),
                base_dir: std::path::Path::new("/"),
                reference_path_current_directory: None,
                checker_libs: &CheckerLibSet::default(),
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics
    }
    fn collect_test_diagnostics_with_options(
        files: &[(&str, &str)],
        options: &ResolvedCompilerOptions,
        base_dir: &Path,
    ) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options,
                base_dir,
                reference_path_current_directory: None,
                checker_libs: &CheckerLibSet::default(),
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics
    }

    struct FileSessionReuseOverrideGuard;

    impl Drop for FileSessionReuseOverrideGuard {
        fn drop(&mut self) {
            FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(None));
        }
    }

    fn collect_test_diagnostics_with_file_session_reuse(
        files: &[(&str, &str)],
        enabled: bool,
    ) -> Vec<Diagnostic> {
        FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(Some(enabled)));
        let _guard = FileSessionReuseOverrideGuard;
        let options = ResolvedCompilerOptions {
            no_emit: true,
            ..ResolvedCompilerOptions::default()
        };
        collect_test_diagnostics_with_options(files, &options, std::path::Path::new("/"))
    }

    fn merged_program_from_owned_files(files: Vec<(String, String)>) -> MergedProgram {
        let bind_results: Vec<_> = files
            .into_iter()
            .map(|(file_name, source)| parallel::parse_and_bind_single(file_name, source))
            .collect();
        parallel::merge_bind_results(bind_results)
    }

    #[test]
    fn project_mode_cross_file_class_type_reference_uses_instance_type() {
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/base.ts",
                    r#"
export abstract class Base {
  abstract self(): Base;
}
"#,
                ),
                (
                    "/p/derived.ts",
                    r#"
import { Base } from "./base";

export class Derived extends Base {
  self(): Derived {
    return this;
  }
}
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );

        assert!(
            diagnostics.iter().all(|diagnostic| diagnostic.code != 2416),
            "project mode should resolve imported class type annotations to the instance type, got: {diagnostics:?}"
        );
    }

    #[test]
    fn project_mode_cross_file_generic_class_self_reference_uses_instance_type() {
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/base.ts",
                    r#"
export abstract class Box<T> {
  value!: T;
  abstract self(): Box<T>;
}
"#,
                ),
                (
                    "/p/derived.ts",
                    r#"
import { Box } from "./base";

export class StringBox extends Box<string> {
  self(): StringBox {
    return this;
  }
}
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );

        assert!(
            diagnostics.iter().all(|diagnostic| diagnostic.code != 2416),
            "project mode should resolve generic imported class self references to the instance type, got: {diagnostics:?}"
        );
    }

    #[test]
    fn project_mode_imported_class_annotation_and_typeof_keep_instance_constructor_split() {
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/base.ts",
                    r#"
export class Token {
  value = 1;
  static create(): Token {
    return new Token();
  }
}
"#,
                ),
                (
                    "/p/use.ts",
                    r#"
import { Token } from "./base";

let okInstance: Token = Token.create();
let okCtor: typeof Token = Token;
let badCtor: typeof Token = Token.create();
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );

        assert_eq!(
            diagnostics.len(),
            1,
            "only the typeof constructor mismatch should be reported, got: {diagnostics:?}"
        );
        assert_eq!(
            diagnostics[0].code, 2739,
            "typeof Token should remain constructor-shaped, got: {diagnostics:?}"
        );
    }

    // Regression: a cross-file named import of a *type alias* whose body is a
    // union containing `null` must resolve to the full union, not collapse to
    // `null`. A named type reference routed through the value-in-type-position
    // "instance type" extraction (`instance_type_from_constructor_type`) used
    // to partially extract instance types from union members: `null` maps to
    // itself while a sibling primitive such as `string` is `NotConstructor`
    // and was silently dropped, leaving the alias as bare `null`. That produced
    // false TS2322 on valid values and false TS2344 ("does not satisfy the
    // constraint 'null'") in constraint position for any imported union alias
    // with a `null` member (e.g. type-fest's `Primitive`). Renamed binders and
    // varied null positions prove the rule is structural, not name-keyed.
    #[test]
    fn project_mode_cross_file_null_union_alias_import_does_not_collapse_to_null() {
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/scalars.ts",
                    r#"
export type Scalarish = null | string;            // null first
export type Tailable = boolean | null;            // null last
export type Mixed = null | string | number | boolean;
"#,
                ),
                (
                    "/p/consumer.ts",
                    r#"
import { Scalarish, Tailable, Mixed } from "./scalars";

// Valid values for the union members must be accepted (these previously
// failed because the alias collapsed to bare `null`).
const a: Scalarish = "ok";
const b: Tailable = true;
const c: Mixed = 42;
const d: Scalarish = null;

// Constraint position must accept a union member (previously false TS2344
// "does not satisfy the constraint 'null'").
type Constrain<Q extends Mixed> = Q;
type Probe = Constrain<string>;

// A genuine non-member value must still be rejected.
const bad: Scalarish = 123;
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );

        assert!(
            diagnostics.iter().all(|diagnostic| diagnostic.code != 2344),
            "imported union alias constraints must not collapse to `null` (no false TS2344), got: {diagnostics:?}"
        );
        let assignment_errors: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 2322)
            .collect();
        assert_eq!(
            assignment_errors.len(),
            1,
            "only `const bad: Scalarish = 123` should fail; valid union values must be accepted, got: {diagnostics:?}"
        );
    }

    // Kysely-shape regression coverage for the cross-file class instance-type
    // resolution fixed by #10686. These tests pin down adjacent shapes from
    // issue #10672 (Readonly mapped-type members, PromiseLike conformance via
    // a `then` method, merged interface + const value, private class fields)
    // that share the cross-file class identity root cause but were not yet
    // covered by the three project_mode_* tests #10686 added.
    //
    // Structural rule under test:
    //   In project mode, a cross-file class reference used in a type position
    //   (including as the argument of a mapped/utility type, as the
    //   constraint of a PromiseLike conformance check, or as a private-field
    //   annotation) must resolve to the class's instance type, not the
    //   constructor (static) type.
    //
    // Renamed identifiers (different class names, different property names,
    // different generic parameter names) are used across the matrix to prove
    // the rule is structural and not name-keyed.

    fn project_mode_es2015_strict_options() -> ResolvedCompilerOptions {
        let mut args = default_cli_args_for_test();
        args.ignore_config = true;
        args.no_emit = true;
        args.strict = true;
        args.target = Some(crate::args::Target::Es2015);

        let mut resolved = crate::config::resolve_compiler_options(None)
            .expect("resolve default compiler options");
        crate::driver::apply_cli_overrides(&mut resolved, &args).expect("apply cli overrides");
        if matches!(resolved.printer.module, ModuleKind::None) {
            resolved.printer.module = ModuleKind::ES2015;
            resolved.checker.module = ModuleKind::ES2015;
        }
        resolved
    }

    #[test]
    fn project_mode_readonly_imported_class_preserves_instance_members() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts"]);
        assert!(
            !lib_files.is_empty(),
            "es5.d.ts must be available to provide Readonly<T>"
        );
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_lib_files_and_options(
            &[
                (
                    "/p/node.ts",
                    r#"
export class AlterNode {
  readonly kind: 'AlterNode' = 'AlterNode';
  cloneWithProps(props: unknown): AlterNode {
    return this;
  }
  cloneWithAlt(alt: unknown): AlterNode {
    return this;
  }
}
"#,
                ),
                (
                    "/p/builder.ts",
                    r#"
import { AlterNode } from "./node";

export class Builder {
  // Cross-file class as the argument of Readonly<T>: the mapped instantiation
  // must preserve the instance methods declared on AlterNode.
  readonly node: Readonly<AlterNode>;

  constructor(node: Readonly<AlterNode>) {
    this.node = node;
  }

  // Reading members through Readonly<X> must see X's instance methods.
  clone(): Builder {
    const next = this.node.cloneWithProps({});
    return new Builder(next);
  }

  // Readonly<X> assignable to X (instance) is the structural rule under test.
  unwrap(): AlterNode {
    return this.node;
  }
}
"#,
                ),
            ],
            &lib_files,
            &options,
        );

        let blocking: Vec<_> = diagnostics
            .iter()
            .filter(|d| matches!(d.code, 2322 | 2339 | 2345 | 2739))
            .collect();
        assert!(
            blocking.is_empty(),
            "Readonly<ImportedClass> must preserve instance members; cross-file class identity in mapped-type position must resolve to the instance type. Blocking diagnostics: {blocking:?}. All: {diagnostics:?}"
        );
    }

    #[test]
    fn project_mode_imported_class_with_then_method_is_promise_like() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts", "es2015.d.ts"]);
        assert!(
            lib_files.len() >= 2,
            "es5.d.ts + es2015.d.ts must be available to provide PromiseLike and Promise"
        );
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_lib_files_and_options(
            &[
                (
                    "/p/builder.ts",
                    r#"
export class SchemaBuilder<O> {
  then<TResult1 = O, TResult2 = never>(
    onfulfilled?: ((value: O) => TResult1 | PromiseLike<TResult1>) | null,
    onrejected?: ((reason: unknown) => TResult2 | PromiseLike<TResult2>) | null,
  ): Promise<TResult1 | TResult2> {
    return Promise.resolve(undefined as unknown as O).then(onfulfilled, onrejected);
  }
}
"#,
                ),
                (
                    "/p/consumer.ts",
                    r#"
import { SchemaBuilder } from "./builder";

declare function expectPromiseLike<U>(p: PromiseLike<U>): void;

declare const b: SchemaBuilder<number>;

// Cross-file class is used in PromiseLike<U> argument position; the relation
// must see the class's instance `then` method to accept the conformance.
expectPromiseLike(b);
"#,
                ),
            ],
            &lib_files,
            &options,
        );

        let blocking: Vec<_> = diagnostics
            .iter()
            .filter(|d| matches!(d.code, 2322 | 2345 | 2739))
            .collect();
        assert!(
            blocking.is_empty(),
            "Cross-file class with a structural `then` method must be recognized as PromiseLike. Blocking diagnostics: {blocking:?}. All: {diagnostics:?}"
        );
    }

    #[test]
    fn project_mode_imported_class_with_private_field_assignable_across_files() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts"]);
        assert!(
            !lib_files.is_empty(),
            "es5.d.ts must be available to provide Array<T>"
        );
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_lib_files_and_options(
            &[
                (
                    "/p/store.ts",
                    r#"
export class Store<V> {
  #items: V[] = [];
  push(value: V): void {
    this.#items.push(value);
  }
  size(): number {
    return this.#items.length;
  }
}
"#,
                ),
                (
                    "/p/consumer.ts",
                    r#"
import { Store } from "./store";

declare function consume(s: Store<number>): void;

const s: Store<number> = new Store<number>();
s.push(1);
consume(s);
"#,
                ),
            ],
            &lib_files,
            &options,
        );

        let blocking: Vec<_> = diagnostics
            .iter()
            .filter(|d| matches!(d.code, 2322 | 2345 | 2339 | 2739))
            .collect();
        assert!(
            blocking.is_empty(),
            "Cross-file class with a private (#) field must remain assignable through type annotations. Blocking diagnostics: {blocking:?}. All: {diagnostics:?}"
        );
    }

    #[test]
    fn project_mode_cross_file_kysely_shape_negative_still_reports_genuine_mismatch() {
        // The cross-file class instance-type fix must not silence genuine
        // mismatches. Each kysely-shape pattern below still surfaces its
        // ordinary diagnostic when the source/target really differ.
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts", "es2015.d.ts"]);
        assert!(
            lib_files.len() >= 2,
            "es5.d.ts + es2015.d.ts must be available for negative coverage"
        );
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_lib_files_and_options(
            &[
                (
                    "/p/lib.ts",
                    r#"
export class NodeA {
  readonly kind: 'A' = 'A';
  alpha(): number {
    return 1;
  }
}

export class NodeB {
  readonly kind: 'B' = 'B';
  beta(): string {
    return "b";
  }
}

export class NotAwaitable {
  describe(): string {
    return "no then here";
  }
}
"#,
                ),
                (
                    "/p/use.ts",
                    r#"
import { NodeA, NodeB, NotAwaitable } from "./lib";

// Cross-file class identity preserved on the negative direction:
// NodeA and NodeB are distinct cross-file classes, so assigning one to
// Readonly<the other> must still report TS2322.
declare const a: NodeA;
const wrong: Readonly<NodeB> = a;

// Cross-file class without a `then` method is not PromiseLike: assigning it
// to PromiseLike<unknown> must still report TS2322.
declare const na: NotAwaitable;
const notPL: PromiseLike<unknown> = na;
"#,
                ),
            ],
            &lib_files,
            &options,
        );

        // tsc emits TS2741 (missing required property) for these specific
        // missing-member assignability failures. The fix must still produce a
        // mismatch diagnostic — accept TS2322 or TS2741, which both represent
        // a real cross-file assignability failure preserved by the fix.
        let mismatches = diagnostics
            .iter()
            .filter(|d| matches!(d.code, 2322 | 2741))
            .collect::<Vec<_>>();
        assert_eq!(
            mismatches.len(),
            2,
            "genuine cross-file Readonly mismatch and missing-then PromiseLike mismatch must still emit a mismatch diagnostic. Got: {diagnostics:?}"
        );
    }

    /// Issue #13484 (baseline): a generic base-class member typed by a base type
    /// parameter (`_def: Def`) must type-check against the concrete `Def` even
    /// when `Def` is a locally-declared interface that is *also* re-exported
    /// through a barrel (`export *`) and pulled in by a namespace import
    /// (`import * as ns`).
    ///
    /// At project scale (real driver, with module resolutions threaded through)
    /// the re-export + namespace materialization pointed the interface's
    /// canonical file index at the re-exporting barrel; tsz then delegated the
    /// interface's type computation to the barrel arena, which has no body for
    /// its `Lazy(DefId)`, so the delegation returned the `error` sentinel. That
    /// `error` flowed into the derived class's inherited `_def` member, so
    /// `this._def.checks` became `error[]` and `new Schema({ ...this._def, ... })`
    /// reported a spurious `TS2345`. `tsc` accepts this program. The structural
    /// reproduction is project-scale (see the PR's standalone repro and the zod
    /// corpus row); this end-to-end assertion pins that the re-export + namespace
    /// + generic-heritage shape stays clean through the driver pipeline.
    ///
    /// Names here intentionally avoid the zod/kysely identifiers to keep the
    /// assertion structural (no identifier/file-name dependence).
    #[test]
    fn project_mode_reexport_namespace_interface_heritage_no_error_leak() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts", "es2015.d.ts"]);
        let options = project_mode_es2015_strict_options();

        let diagnostics = collect_test_diagnostics_with_lib_files_and_options(
            &[
                (
                    "/p/schema.ts",
                    r#"
export interface SchemaDef { readonly tag: string }
export abstract class Schema<Out, Def extends SchemaDef = SchemaDef, In = Out> {
  readonly _out!: Out;
  readonly _in!: In;
  readonly _def!: Def;
  constructor(def: Def) { this._def = def; }
  abstract parse(data: unknown): Out;
}
export interface NumberCheck { readonly kind: "min" | "max"; readonly value: number }
export interface NumberDef extends SchemaDef { readonly checks: NumberCheck[] }
export class NumberSchema extends Schema<number, NumberDef> {
  parse(_data: unknown): number { return 0; }
  addCheck(check: NumberCheck): NumberSchema {
    return new NumberSchema({ ...this._def, checks: [...this._def.checks, check] });
  }
}
"#,
                ),
                ("/p/barrel.ts", "export * from \"./schema\";\n"),
                (
                    "/p/index.ts",
                    "import * as ns from \"./barrel\";\nexport * from \"./barrel\";\nexport { ns };\n",
                ),
            ],
            &lib_files,
            &options,
        );

        let leaked: Vec<_> = diagnostics
            .iter()
            .filter(|d| matches!(d.code, 2345 | 2322 | 2339))
            .collect();
        assert!(
            leaked.is_empty(),
            "inherited base-type-parameter member degraded to `error` across a \
             re-export barrel + namespace import (issue #13484); expected no \
             assignability/property diagnostics, got: {leaked:#?}"
        );
    }

    #[test]
    fn project_mode_merged_interface_and_const_value_keep_distinct_shapes_across_files() {
        let options = project_mode_es2015_strict_options();

        // Kysely-shape: an interface `Op` declares the data shape;
        // a const `Op` carries static-like helpers (`is` / `create`). Cross-file
        // consumers must see the interface (no helpers) when `Op` is in a type
        // position, and the const (with helpers) when it is in a value position.
        let diagnostics = collect_test_diagnostics_with_options(
            &[
                (
                    "/p/op.ts",
                    r#"
export interface Op {
  readonly kind: string;
}

export const Op = {
  is(value: unknown): value is Op {
    return typeof value === "object" && value !== null && "kind" in value;
  },
  create(kind: string): Op {
    return { kind };
  },
};
"#,
                ),
                (
                    "/p/use.ts",
                    r#"
import { Op } from "./op";

// `Op` in a type position is the interface (no helpers).
declare const item: Op;
const k: string = item.kind;

// `Op` in a value position is the const (helpers available).
const made: Op = Op.create("X");
const checked: boolean = Op.is(made);
"#,
                ),
            ],
            &options,
            Path::new("/p"),
        );

        let blocking: Vec<_> = diagnostics
            .iter()
            .filter(|d| matches!(d.code, 2322 | 2345 | 2339 | 2739 | 2306))
            .collect();
        assert!(
            blocking.is_empty(),
            "Merged interface + const must keep distinct cross-file shapes; type-position and value-position resolutions must not collapse. Blocking diagnostics: {blocking:?}. All: {diagnostics:?}"
        );
    }

    /// Asserts the post-PR-#7521 file-session reuse env policy: OFF unless
    /// the user opts back in via `TSZ_FILE_SESSION_REUSE=1`. Before
    /// PR #7521 the default was ON (set by PRs #6870 / #6893) which
    /// regressed wall time 4-14x at 1k+ files; see
    /// `docs/architecture/LSP_PERF_EXPERIMENTS_2026-05-16.md`.
    ///
    /// Failure modes this test catches:
    ///   * someone accidentally reverts the env default-OFF policy
    ///     (`file_session_reuse_from_env(false, false)` returns true)
    ///   * `TSZ_FILE_SESSION_REUSE=1` opt-in stops working
    ///   * `TSZ_DISABLE_FILE_SESSION_REUSE=1` opt-out stops working
    ///   * the disable knob stops taking precedence over the enable knob
    #[test]
    fn file_session_reuse_env_policy_pr_7521() {
        // Default (no env vars set): reuse OFF.
        assert!(
            !file_session_reuse_from_env(false, false),
            "PR #7521: default reuse policy must be OFF (no env vars set)"
        );

        // Explicit opt-in: TSZ_FILE_SESSION_REUSE=1 turns reuse back on.
        assert!(
            file_session_reuse_from_env(false, true),
            "TSZ_FILE_SESSION_REUSE=1 must opt back in"
        );

        // Explicit opt-out: TSZ_DISABLE_FILE_SESSION_REUSE=1 forces OFF.
        assert!(
            !file_session_reuse_from_env(true, false),
            "TSZ_DISABLE_FILE_SESSION_REUSE=1 must force reuse OFF"
        );

        // Disable beats enable: both set => OFF.
        assert!(
            !file_session_reuse_from_env(true, true),
            "TSZ_DISABLE_FILE_SESSION_REUSE=1 must take precedence over TSZ_FILE_SESSION_REUSE=1"
        );
    }

    #[test]
    fn file_session_reuse_workload_policy_keeps_reuse_opt_in_for_tiny_batches() {
        assert!(
            file_session_reuse_from_workload(false, false, 10, false),
            "non-JS/JSX tiny no-emit batches may reuse by default"
        );
        assert!(
            file_session_reuse_from_workload(
                false,
                false,
                FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES,
                false
            ),
            "the documented tiny-project boundary is the default reuse limit for non-JS/JSX workloads"
        );
        assert!(
            !file_session_reuse_from_workload(false, false, 10, true),
            "JS/JSX tiny no-emit batches stay fresh by default to preserve diagnostic identity"
        );
        assert!(
            !file_session_reuse_from_workload(
                false,
                false,
                FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES + 1,
                false
            ),
            "larger batch CLI projects must keep the post-#7521 reuse-off default"
        );
        assert!(
            file_session_reuse_from_workload(
                false,
                true,
                FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES + 1,
                true
            ),
            "TSZ_FILE_SESSION_REUSE=1 must still opt larger and JSX projects into reuse"
        );
        assert!(
            !file_session_reuse_from_workload(true, true, 10, false),
            "TSZ_DISABLE_FILE_SESSION_REUSE=1 must override tiny-project auto reuse"
        );
    }

#[test]
fn large_reuse_off_batches_keep_fresh_parallel_eligible() {
    let large_project = FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES + 1;

    assert!(
        !should_use_sequential_fresh_checking(large_project, false, false, false),
        "large fresh-checker batches should stay parallel-eligible even when session reuse is off"
    );
    assert!(
        should_use_sequential_fresh_checking(10, false, false, false),
        "tiny batches stay sequential for deterministic cross-file behavior"
    );
    assert!(
        should_use_sequential_fresh_checking(10, true, true, false),
        "the force-parallel experiment must not bypass tiny-batch policy"
    );
    assert!(
        should_use_sequential_fresh_checking(large_project, true, false, false),
        "order-sensitive global libraries stay on the deterministic sequential fallback"
    );
    assert!(
        !should_use_sequential_fresh_checking(large_project, true, true, false),
        "the force-parallel experiment should bypass only the order-sensitive global-lib gate"
    );
    // The dedicated tiny-batch override forces the genuine `par_iter`
    // fresh-checker path even below the small-project floor, so the
    // schedule-determinism regression guards stop silently degrading to
    // sequential-vs-sequential no-ops.
    assert!(
        !should_use_sequential_fresh_checking(10, false, false, true),
        "TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY must force tiny batches onto the parallel path"
    );
    assert!(
        should_use_sequential_fresh_checking(10, true, false, true),
        "the tiny-batch override must still respect the order-sensitive global-lib gate"
    );
    assert!(
        !should_use_sequential_fresh_checking(10, true, true, true),
        "both force overrides together must reach the parallel path"
    );
}

#[test]
fn checker_pool_refuses_order_sensitive_global_lib_by_default() {
    // The bounded checker pool shares one `DefinitionStore` across parallel
    // partitions, so it must honor the same DOM/webworker serialization gate
    // as the fresh-parallel lane — otherwise an explicit `TSZ_CHECKER_POOL=N`
    // (or a default-on pool) routes a DOM project onto the pool and produces
    // non-deterministic diagnostics.
    assert!(
        pool_refused_for_order_sensitive_global_lib(true, false),
        "DOM/webworker programs must be refused the pool and take the sequential path"
    );
    assert!(
        !pool_refused_for_order_sensitive_global_lib(false, false),
        "non-DOM programs stay pool-eligible"
    );
    assert!(
        !pool_refused_for_order_sensitive_global_lib(true, true),
        "the force-parallel diagnosis override lifts the refusal for byte-diff testing"
    );
    assert!(
        !pool_refused_for_order_sensitive_global_lib(false, true),
        "the override is a no-op when no order-sensitive global lib is present"
    );
}

/// Asserts the Stage-B default-on policy for the bounded checker pool: the
/// pool turns ON by default for the large non-DOM parallel lane, while the
/// explicit env width, the explicit `=0`/empty off, and the kill switch keep
/// their precedence.
#[test]
fn checker_pool_defaults_on_for_large_non_dom_parallel_lane() {
    const AP: usize = 8;
    use CheckerPoolEnv::{ForceOff, Unset, Width};

    // Default-on: large non-DOM parallel lane (eligible) with no env knobs.
    assert_eq!(
        resolve_checker_pool_size(Unset, false, true, AP),
        Some(AP),
        "large non-DOM parallel lane must default the pool on, sized to available parallelism"
    );

    // Ineligible lanes (small project / DOM / explicit file-session reuse)
    // keep the pool off by default.
    assert_eq!(
        resolve_checker_pool_size(Unset, false, false, AP),
        None,
        "ineligible lanes (small/DOM/reuse-opt-in) keep the pool off by default"
    );

    // Explicit width wins on any lane, eligible or not.
    assert_eq!(
        resolve_checker_pool_size(Width(4), false, false, AP),
        Some(4),
        "explicit TSZ_CHECKER_POOL=<n> must win even on an ineligible lane"
    );
    assert_eq!(
        resolve_checker_pool_size(Width(4), true, true, AP),
        Some(4),
        "explicit TSZ_CHECKER_POOL=<n> must win over the kill switch and the default"
    );

    // Explicit `=0`/empty forces off, overriding the eligible-lane default.
    assert_eq!(
        resolve_checker_pool_size(ForceOff, false, true, AP),
        None,
        "explicit TSZ_CHECKER_POOL=0 must override the eligible-lane default"
    );

    // Kill switch suppresses the default-on behavior.
    assert_eq!(
        resolve_checker_pool_size(Unset, true, true, AP),
        None,
        "TSZ_DISABLE_CHECKER_POOL must override the eligible-lane default"
    );
}

    #[test]
    fn tiny_no_emit_reuse_path_covers_boxed_prime_checker() {
        assert!(
            !needs_separate_boxed_prime_checker(true, false, true, 10, true),
            "tiny no-emit reuse should prime on the reused checker, not a duplicate checker"
        );
        assert!(
            needs_separate_boxed_prime_checker(true, false, false, 10, true),
            "fresh-checker tiny runs still need the separate prime checker when reuse is forced off"
        );
        assert!(
            needs_separate_boxed_prime_checker(
                true,
                false,
                true,
                FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES + 1,
                true,
            ),
            "large projects do not use the tiny reused-checker coverage rule"
        );
        assert!(
            needs_separate_boxed_prime_checker(true, true, true, 10, true),
            "declaration emit consumes per-file state and cannot use tiny no-emit coverage"
        );
        assert!(
            !needs_separate_boxed_prime_checker(true, false, true, 10, false),
            "projects without libs have nothing to prime"
        );
    }



    fn checker_lib_set_for_test(libs: &[(&str, &str)]) -> CheckerLibSet {
        let files = libs
            .iter()
            .map(|(file_name, source)| {
                std::sync::Arc::new(tsz::binder::lib_loader::LibFile::from_source(
                    (*file_name).to_string(),
                    (*source).to_string(),
                ))
            })
            .collect::<Vec<_>>();
        let contexts = files
            .iter()
            .map(|lib| LibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();

        CheckerLibSet {
            files,
            contexts: std::sync::Arc::new(contexts),
        }
    }

    #[test]
    fn user_only_global_interfaces_do_not_trigger_lib_recheck() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.test.d.ts",
            r#"
interface Window {
    document: object;
}
"#,
        )]);

        let program = merged_program_from_owned_files(vec![(
            "file.ts".to_string(),
            r#"
interface Result<T> {
    value?: T;
}
"#
            .to_string(),
        )]);

        let affected = affected_lib_interface_names(&program, &checker_libs);
        assert!(
            affected.is_empty(),
            "user-only global interfaces should not request default-lib recheck, got: {affected:?}"
        );
    }

    #[test]
    fn user_global_interfaces_matching_lib_names_still_trigger_lib_recheck() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.test.d.ts",
            r#"
interface Window {
    document: object;
}
"#,
        )]);

        let program = merged_program_from_owned_files(vec![(
            "file.ts".to_string(),
            r#"
interface Window {
    custom: string;
}
"#
            .to_string(),
        )]);

        let affected = affected_lib_interface_names(&program, &checker_libs);
        assert!(
            affected.contains("Window"),
            "lib-matching global interfaces must still request default-lib recheck, got: {affected:?}"
        );
    }

    #[test]
    fn parallel_order_sensitive_lib_detection_is_scoped_to_dom_like_globals() {
        let es_libs = checker_lib_set_for_test(&[("lib.es2018.d.ts", "interface Promise<T> {}\n")]);
        assert!(
            !has_parallel_order_sensitive_global_lib(&es_libs),
            "plain ES libs should stay eligible for parallel project checking"
        );

        let dom_libs =
            checker_lib_set_for_test(&[("lib.dom.d.ts", "interface Console { log(): void; }\n")]);
        assert!(
            has_parallel_order_sensitive_global_lib(&dom_libs),
            "DOM-style globals should use deterministic project checking"
        );
    }

    fn collect_test_diagnostics_with_checker_libs(
        files: &[(&str, &str)],
        checker_libs: &CheckerLibSet,
    ) -> Vec<Diagnostic> {
        let bind_results: Vec<_> = files
            .iter()
            .map(|(file_name, source)| {
                parallel::parse_and_bind_single((*file_name).to_string(), (*source).to_string())
            })
            .collect();
        let program = parallel::merge_bind_results(bind_results);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options: &ResolvedCompilerOptions::default(),
                base_dir: std::path::Path::new("/"),
                reference_path_current_directory: None,
                checker_libs,
                typescript_dom_replacement_globals: (false, false, false),
                has_deprecation_diagnostics: false,
                collect_compile_stats: false,
            },
            None,
            &type_cache_output,
        )
        .diagnostics
    }

    fn collect_test_diagnostics_with_lib_files(
        files: &[(&str, &str)],
        lib_files: &[std::sync::Arc<tsz::binder::lib_loader::LibFile>],
    ) -> Vec<Diagnostic> {
        collect_test_diagnostics_with_lib_files_and_options(
            files,
            lib_files,
            &ResolvedCompilerOptions::default(),
        )
    }

    fn collect_test_diagnostics_with_lib_files_and_options(
        files: &[(&str, &str)],
        lib_files: &[std::sync::Arc<tsz::binder::lib_loader::LibFile>],
        options: &ResolvedCompilerOptions,
    ) -> Vec<Diagnostic> {
        let compile_inputs = files
            .iter()
            .map(|(file_name, source)| ((*file_name).to_string(), (*source).to_string()))
            .collect::<Vec<_>>();
        let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
            compile_inputs,
            lib_files,
        ));
        let checker_libs = load_checker_libs(lib_files);
        let type_cache_output = std::sync::Mutex::new(FxHashMap::default());

        collect_diagnostics(
            &CollectDiagnosticsInput {
                program: &program,
                options,
                base_dir: std::path::Path::new("/"),
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

    fn default_cli_args_for_test() -> CliArgs {
        clap::Parser::try_parse_from(["tsz"]).expect("default args should parse")
    }

    fn resolved_options_for_es2015_strict_test() -> ResolvedCompilerOptions {
        let mut args = default_cli_args_for_test();
        args.ignore_config = true;
        args.strict = true;
        args.target = Some(crate::args::Target::Es2015);

        let mut resolved = crate::config::resolve_compiler_options(None)
            .expect("resolve default compiler options");
        crate::driver::apply_cli_overrides(&mut resolved, &args).expect("apply cli overrides");
        if matches!(resolved.printer.module, ModuleKind::None) {
            resolved.printer.module = ModuleKind::ES2015;
            resolved.checker.module = ModuleKind::ES2015;
        }
        resolved
    }

    /// A cross-file alias whose body nests a homomorphic utility type over an
    /// imported type (`Omit<Partial<X>, "k">`) must resolve to the real object
    /// shape, even when the consumer reaches the alias first through a property
    /// access (so the alias is first evaluated in a nested context).
    ///
    /// Regression for #10682 (kysely): while computing `keyof T` for the
    /// enclosing `Omit`, `Partial`'s structural body has not been registered
    /// yet, so the resolver hands back `Partial`'s own self-lazy wrapper.
    /// Substituting the argument into that wrapper dropped it, collapsing
    /// `Partial<X>` to bare `Partial` and the whole alias to `{}`; a fresh
    /// object literal with a valid optional subset then failed with a false
    /// `TS2345`, and the cached degenerate result poisoned later uses. The fix
    /// keeps the application opaque until the body is ready.
    ///
    /// The helper runs two unrelated name/key choices so a fix keyed to a
    /// single spelling (the interface name, property names, or the omitted
    /// key) would not satisfy it.
    #[test]
    fn cross_file_omit_partial_alias_param_resolves_via_property_access() {
        // `iface` gains a required `omit_key: string` plus optional number
        // members `props`; the alias drops `omit_key` via `Omit<Partial<_>, _>`.
        fn run(case: &str, iface: &str, omit_key: &str, props: &[&str], good: &str) {
            let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts"]);
            assert!(
                !lib_files.is_empty(),
                "[{case}] es5.d.ts must be available for this regression"
            );
            let members = props
                .iter()
                .map(|p| format!("  readonly {p}?: number;\n"))
                .collect::<String>();
            let wrong_prop = props[0];
            let defs = format!(
                "export interface {iface} {{\n  readonly {omit_key}: string;\n{members}}}\n\
                 export type Props = Omit<Partial<{iface}>, \"{omit_key}\">;\n\
                 // type/value name collision, as in kysely's frozen node factories.\n\
                 export declare const {iface}: {{ with(p: Props): {iface} }};\n"
            );
            // Consumer file listed first so the alias is first evaluated through
            // the nested property-access path (the order that triggered #10682).
            let use_src = format!(
                "import {{ {iface} }} from \"./defs.js\";\n\
                 export const okMulti = {iface}.with({{ {good} }});\n\
                 export const okEmpty = {iface}.with({{}});\n\
                 export const wrong = {iface}.with({{ {wrong_prop}: \"not-a-number\" }});\n\
                 export const excess = {iface}.with({{ definitelyNotAKey: 1 }});\n"
            );
            let files = [("/p/use.ts", use_src.as_str()), ("/p/defs.ts", defs.as_str())];
            let diagnostics = collect_test_diagnostics_with_lib_files(&files, &lib_files);

            // The fix: the nested-utility alias no longer collapses to `{}`, so
            // valid optional-subset literals (and `{}`) produce no false TS2345.
            assert!(
                !diagnostics.iter().any(|d| d.code == 2345),
                "[{case}] valid optional-subset literals must be assignable to \
                 Omit<Partial<{iface}>, \"{omit_key}\"> resolved through property \
                 access; no TS2345 expected. Diagnostics: {diagnostics:?}"
            );

            // The alias must still resolve to a real object (not `any`/`{}`):
            // an unknown property and a wrong-typed value are still rejected.
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.code == 2353 && d.message_text.contains("definitelyNotAKey")),
                "[{case}] an unknown property must still be rejected (TS2353), \
                 proving the alias resolved to a structural object. \
                 Diagnostics: {diagnostics:?}"
            );
            assert!(
                diagnostics.iter().any(|d| d.code == 2322),
                "[{case}] a wrong-typed property must still be rejected (TS2322). \
                 Diagnostics: {diagnostics:?}"
            );
        }

        // Two unrelated spellings prove the fix is structural, not name-keyed.
        run("widget", "Widget", "kind", &["w", "label"], "w: 1, label: 2");
        run("record", "RecordNode", "tag", &["alpha", "beta"], "alpha: 2");
    }

    #[test]
    fn readonly_alias_annotation_survives_consumer_first_program_check() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts"]);
        assert!(
            !lib_files.is_empty(),
            "es5.d.ts must be available for this regression"
        );
        let files = [
            (
                "/p/b.ts",
                r#"
import { Factory } from "./a.js";

Factory.cloneWith("x");
"#,
            ),
            (
                "/p/a.ts",
                r#"
import { freeze } from "./object-utils.js";

type Factory = Readonly<{
  create(name: string): string;
  cloneWith(value: string): string;
}>;

export const Factory: Factory = freeze<Factory>({
  create(name) {
    return name;
  },
  cloneWith(value) {
    return value;
  },
});
"#,
            ),
            (
                "/p/object-utils.ts",
                r#"
export function freeze<T>(value: T): Readonly<T> {
  return value;
}
"#,
            ),
        ];

        let diagnostics = collect_test_diagnostics_with_lib_files(&files, &lib_files);
        let ts2339 = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 2339)
            .collect::<Vec<_>>();

        assert!(
            ts2339.is_empty(),
            "Readonly alias annotations should not collapse to unknown in consumer-first program checks. Got: {ts2339:?}. All: {diagnostics:?}"
        );
    }

    #[test]
    fn large_project_checking_preserves_parallel_dom_globals() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts", "dom.d.ts"]);
        assert!(
            lib_files.len() >= 2,
            "es5.d.ts and dom.d.ts must be available for this regression"
        );

        let owned_files = (0..40)
            .map(|idx| {
                (
                    format!("pkg{idx}/file{idx}.ts"),
                    format!("console.log(\"file{idx}\");\nconsole.warn(\"file{idx}\");\n"),
                )
            })
            .collect::<Vec<_>>();
        let files = owned_files
            .iter()
            .map(|(file_name, source)| (file_name.as_str(), source.as_str()))
            .collect::<Vec<_>>();
        let options = ResolvedCompilerOptions {
            no_emit: true,
            ..ResolvedCompilerOptions::default()
        };

        let reused_diagnostics = {
            FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(Some(true)));
            let _guard = FileSessionReuseOverrideGuard;
            collect_test_diagnostics_with_lib_files_and_options(&files, &lib_files, &options)
        };
        let disabled_diagnostics = {
            FILE_SESSION_REUSE_TEST_OVERRIDE.with(|override_value| override_value.set(Some(false)));
            let _guard = FileSessionReuseOverrideGuard;
            collect_test_diagnostics_with_lib_files_and_options(&files, &lib_files, &options)
        };
        let console_member_errors = reused_diagnostics
            .iter()
            .chain(disabled_diagnostics.iter())
            .filter(|diagnostic| diagnostic.code == 2339)
            .collect::<Vec<_>>();

        assert!(
            console_member_errors.is_empty(),
            "large-project DOM globals must not be order-dependent. TS2339: {console_member_errors:?}. Reused: {reused_diagnostics:?}. Disabled: {disabled_diagnostics:?}"
        );
    }

    #[test]
    fn arbitrary_extension_html_import_preserves_dom_class_constructor_heritage() {
        let lib_files = tsz::checker::test_utils::load_lib_files(&[
            "es5.d.ts",
            "es2015.iterable.d.ts",
            "es2015.symbol.d.ts",
            "es2015.symbol.wellknown.d.ts",
            "es2020.d.ts",
            "dom.d.ts",
        ]);
        assert!(
            !lib_files.is_empty(),
            "DOM libs must be available for this regression"
        );

        let files = [
            (
                "/p/component.d.html.ts",
                r#"
declare var doc: Document;
export default doc;
export const blogPost: Element;
export class HTML5Element extends HTMLElement {
    connectedCallback(): void;
}
"#,
            ),
            (
                "/p/file.ts",
                r#"
import * as mod from "./component.html";

window.customElements.define("my-html5-element", mod.HTML5Element);

if (document !== mod.default) {
    document.body.appendChild(mod.blogPost);
}
const instance: HTMLElement = new mod.HTML5Element();
"#,
            ),
        ];
        let options = ResolvedCompilerOptions {
            no_emit: true,
            allow_arbitrary_extensions: false,
            ..ResolvedCompilerOptions::default()
        };
        let diagnostics =
            collect_test_diagnostics_with_lib_files_and_options(&files, &lib_files, &options);
        let ts2345 = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 2345)
            .collect::<Vec<_>>();

        assert!(
            ts2345.is_empty(),
            "imported HTML declaration classes should stay assignable to CustomElementConstructor. Got: {ts2345:?}. All: {diagnostics:?}"
        );
    }

    #[test]
    fn class_extends_identity_shortcut_preserves_member_relation_checks() {
        let diagnostics = collect_test_diagnostics(&[(
            "test.ts",
            r#"
class C {
    foo(x: number) { }
}

class D extends C {
    foo() { }
}

class E extends D {
    foo(x?: string) { }
}

declare var c: C;
declare var e: E;
c = e;
"#,
        )]);

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code == 2322),
            "class ancestry identity must not bypass structural member checks. Got: {diagnostics:?}"
        );
    }

    #[test]
    fn file_session_reuse_preserves_multifile_diagnostics() {
        let files = [
            (
                "a.ts",
                "interface Alpha { kind: \"alpha\"; count: number }\nconst a: Alpha = { kind: \"alpha\", count: \"nope\" };\n",
            ),
            (
                "b.ts",
                "interface Beta { kind: \"beta\"; count: number }\nconst b: Beta = { kind: \"beta\", count: \"nope\" };\n",
            ),
            (
                "c.ts",
                "interface Gamma { kind: \"gamma\"; count: number }\nconst c: Gamma = { kind: \"gamma\", count: \"nope\" };\n",
            ),
        ];

        let default_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, false);
        let reused_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, true);

        assert_eq!(
            reused_diagnostics, default_diagnostics,
            "file-session reuse must preserve byte-identical diagnostics"
        );
        assert!(
            !default_diagnostics.is_empty(),
            "fixture should exercise real checker diagnostics"
        );
    }

    #[test]
    fn file_session_reuse_preserves_parallel_multifile_diagnostics() {
        let owned_files = (0..40)
            .map(|idx| {
                (
                    format!("pkg{idx}/file{idx}.ts"),
                    format!("export {{}};\nconst value{idx}: number = \"nope\";\n"),
                )
            })
            .collect::<Vec<_>>();
        let files = owned_files
            .iter()
            .map(|(file_name, source)| (file_name.as_str(), source.as_str()))
            .collect::<Vec<_>>();

        let default_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, false);
        let reused_diagnostics = collect_test_diagnostics_with_file_session_reuse(&files, true);

        assert_eq!(
            reused_diagnostics, default_diagnostics,
            "parallel file-session reuse must preserve byte-identical diagnostics"
        );
        assert_eq!(
            default_diagnostics.len(),
            owned_files.len(),
            "fixture should produce one checker diagnostic per file"
        );
    }

    #[test]
    fn no_check_collect_diagnostics_keeps_parse_errors_and_skips_type_errors() {
        let options = ResolvedCompilerOptions {
            no_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "const value: string = 1;\nconst broken = ;\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&1109),
            "expected --noCheck diagnostics to keep TS1109 parse error, got: {diagnostics:?}"
        );
        assert!(
            !codes.contains(&2322),
            "expected --noCheck diagnostics to skip TS2322 type error, got: {diagnostics:?}"
        );
    }

    #[test]
    fn no_check_path_emits_isolated_declarations_ts9007() {
        // Issue #3709: `--noCheck --isolatedDeclarations` previously dropped
        // TS9007/TS9011/etc. tsc still reports these because they gate
        // declaration emission, not type checking.
        let mut options = ResolvedCompilerOptions {
            no_check: true,
            ..ResolvedCompilerOptions::default()
        };
        options.checker.isolated_declarations = true;

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "export function f(x) { return x; }\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            codes.contains(&9007),
            "expected --noCheck --isolatedDeclarations to surface TS9007, got: {diagnostics:?}"
        );
    }

    #[test]
    fn no_check_without_isolated_declarations_does_not_run_isolated_decl_pass() {
        // Without --isolatedDeclarations, the isolated-decl pass must not
        // fire and produce TS9007.
        let options = ResolvedCompilerOptions {
            no_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "export function f(x) { return x; }\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            !codes.contains(&9007),
            "TS9007 must not fire under --noCheck without --isolatedDeclarations, got: {diagnostics:?}"
        );
    }

    #[test]
    fn no_check_with_declaration_emit_still_suppresses_type_errors() {
        // Issue #3733: under `--noCheck --declaration`, the regular checker
        // pipeline must run so declaration emit can pick up inferred types
        // (return types, contextual property types). But type-error
        // diagnostics (TS2322 etc.) must still be suppressed — `--noCheck`
        // means "don't surface type checking errors".
        let options = ResolvedCompilerOptions {
            no_check: true,
            emit_declarations: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[("file.ts", "export const x: string = 1;\n")],
            &options,
            std::path::Path::new("/"),
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

        assert!(
            !codes.contains(&2322),
            "TS2322 must not fire under --noCheck --declaration, got: {diagnostics:?}"
        );
    }

    #[test]
    fn skip_lib_check_pure_declaration_no_emit_skips_semantic_diagnostics() {
        let options = ResolvedCompilerOptions {
            no_emit: true,
            skip_lib_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[(
                "index.d.ts",
                r#"
export type UsesMissing = Missing;
export interface Broken {
    value: ;
}
"#,
            )],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code < 2000),
            "parse diagnostics must still surface under skipLibCheck: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2304),
            "skipLibCheck must suppress declaration-file semantic diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn skip_lib_check_mixed_project_still_checks_source_files() {
        let options = ResolvedCompilerOptions {
            no_emit: true,
            skip_lib_check: true,
            ..ResolvedCompilerOptions::default()
        };

        let diagnostics = collect_test_diagnostics_with_options(
            &[
                ("types.d.ts", "export type UsesMissing = Missing;\n"),
                ("main.ts", "const value: string = 1;\n"),
            ],
            &options,
            std::path::Path::new("/"),
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2322),
            "non-declaration source files must still be checked under skipLibCheck: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|diag| diag.code == 2304),
            "declaration-file semantic diagnostics must remain suppressed: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_preserves_builtin_lib_ts2552_spelling_baseline() {
        let checker_libs = checker_lib_set_for_test(&[
            (
                "lib.esnext.intl.d.ts",
                r#"
declare namespace Intl {
    interface DateTimeFormat {
        formatToParts(): DateTimeFormatPart[];
    }
}
"#,
            ),
            (
                "lib.esnext.temporal.d.ts",
                r#"
declare namespace Temporal {
    interface Instant {}
}
"#,
            ),
        ]);

        let diagnostics = collect_test_diagnostics_with_checker_libs(
            &[("test.ts", "const value = new Intl.DateTimeFormat();\n")],
            &checker_libs,
        );
        let ts2552 = diagnostics
            .iter()
            .filter(|diag| diag.code == 2552)
            .collect::<Vec<_>>();

        assert_eq!(
            ts2552.len(),
            1,
            "expected one baseline lib TS2552 diagnostic, got: {diagnostics:?}"
        );
        assert!(
            ts2552[0]
                .message_text
                .contains("Cannot find name 'DateTimeFormatPart'. Did you mean 'DateTimeFormat'?"),
            "expected DateTimeFormatPart spelling suggestion, got: {ts2552:?}"
        );
        assert_eq!(ts2552[0].file, "lib.esnext.intl.d.ts");
    }

    #[test]
    fn collect_diagnostics_skips_builtin_lib_ts2552_without_temporal_trigger_lib() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.esnext.intl.d.ts",
            r#"
declare namespace Intl {
    interface DateTimeFormat {
        formatToParts(): DateTimeFormatPart[];
    }
}
"#,
        )]);

        let diagnostics = collect_test_diagnostics_with_checker_libs(
            &[("test.ts", "const value = new Intl.DateTimeFormat();\n")],
            &checker_libs,
        );

        assert!(
            diagnostics.iter().all(|diag| diag.code != 2552),
            "expected DateTimeFormatPart baseline to require Temporal/Date libs, got: {diagnostics:?}"
        );
    }

    #[test]
    fn collect_diagnostics_ignores_unrelated_builtin_lib_ts2552_spelling_baseline() {
        let checker_libs = checker_lib_set_for_test(&[(
            "lib.esnext.intl.d.ts",
            r#"
declare namespace Intl {
    interface DateTimeFormatPart {}
    interface DateTimeFormat {
        formatToParts(): DateTimeFormatParts[];
    }
}
"#,
        )]);

        let diagnostics = collect_test_diagnostics_with_checker_libs(
            &[("test.ts", "const value = new Intl.DateTimeFormat();\n")],
            &checker_libs,
        );

        assert!(
            diagnostics.iter().all(|diag| diag.code != 2552),
            "expected unrelated baseline lib TS2552 diagnostics to stay filtered, got: {diagnostics:?}"
        );
    }

    #[test]
    fn datetimeformatpart_spelling_baseline_keys_on_span_not_message() {
        // The Intl baseline predicate must identify the diagnostic by the
        // unresolved identifier at its span, NOT by the rendered TS2552 sentence
        // (a formatted-diagnostic-string predicate is forbidden and brittle across
        // locales/wording). These two cases pin that contract.
        let source = "declare namespace Intl {\n    interface DateTimeFormat {\n        formatToParts(): DateTimeFormatPart[];\n    }\n}\n";
        let checker_libs = checker_lib_set_for_test(&[("lib.esnext.intl.d.ts", source)]);

        let intl_ts2552 = |offset: u32, length: usize, message: &str| {
            Diagnostic::error("lib.esnext.intl.d.ts", offset, length as u32, message, 2552)
        };
        let name_offset = |needle: &str| source.find(needle).expect("token present") as u32;

        // Correct span, deliberately non-canonical message: still matched.
        let span_diag = intl_ts2552(
            name_offset("DateTimeFormatPart["),
            "DateTimeFormatPart".len(),
            "a completely different wording",
        );
        assert!(
            is_datetimeformatpart_spelling_baseline_diagnostic(&span_diag, &checker_libs),
            "must match on the span identifier regardless of the rendered message"
        );

        // Canonical-looking message, but the span covers a different identifier:
        // not matched.
        let mismatched_diag = intl_ts2552(
            name_offset("DateTimeFormat {"),
            "DateTimeFormat".len(),
            "Cannot find name 'DateTimeFormatPart'. Did you mean 'DateTimeFormat'?",
        );
        assert!(
            !is_datetimeformatpart_spelling_baseline_diagnostic(&mismatched_diag, &checker_libs),
            "must not match when the span identifier is not DateTimeFormatPart"
        );
    }

    fn collect_es2015_default_lib_diagnostics(source: &str) -> Vec<Diagnostic> {
        collect_es2015_default_lib_diagnostics_with_options(source, |_: &mut _| {})
    }

    /// Multi-file variant of [`collect_es2015_default_lib_diagnostics`]. Each
    /// `(relative_name, source)` pair is written into the same temp directory so
    /// `./name` imports resolve cross-file, exercising the program/CLI path
    /// (distinct from the single-`main.ts` in-process path). The default es2015
    /// lib set is loaded (includes `lib.dom.d.ts`).
    fn collect_es2015_default_lib_diagnostics_multifile(files: &[(&str, &str)]) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let mut file_paths = Vec::with_capacity(files.len());
        for (name, source) in files {
            let path = dir.path().join(name);
            std::fs::write(&path, source).expect("write source");
            file_paths.push(path);
        }

        let resolved = resolved_options_for_es2015_strict_test();
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

    fn collect_es2015_default_lib_diagnostics_with_options(
        source: &str,
        configure: impl FnOnce(&mut ResolvedCompilerOptions),
    ) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("main.ts");
        std::fs::write(&file_path, source).expect("write source");

        let mut resolved = resolved_options_for_es2015_strict_test();
        configure(&mut resolved);
        let file_paths = vec![file_path];
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
    fn cloned_checker_libs_preserve_strict_builtin_iterator_return() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
declare const map: Map<string, number>;
const value: number = map.values().next().value;
interface Next<A> {
    readonly done?: boolean;
    readonly value: A;
}
const result: Next<number> = map.values().next();
"#,
        );
        let ts2322_count = diagnostics
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count();
        assert_eq!(
            ts2322_count, 2,
            "expected cloned checker libs to preserve strict built-in iterator return diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn es2015_local_interface_t_shadows_lib_heritage_type_parameters() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
interface T { f(x: number): void }
declare var t: T;
t.f("s");
"#,
        );

        assert!(
            diagnostics.iter().any(|diag| diag.code == 2345),
            "expected TS2345 for T.f argument type, got: {diagnostics:?}"
        );
        assert!(
            diagnostics.iter().all(|diag| diag.code != 2339),
            "did not expect TS2339 from a stale local T shape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn es2015_destructuring_reduce_concat_reports_overload_and_iterability() {
        let diagnostics = collect_es2015_default_lib_diagnostics(
            r#"
declare var tuple: [boolean, number, ...string[]];

const [a, b, c, ...rest] = tuple;

declare var receiver: typeof tuple;

[...receiver] = tuple;

const [oops1] = [1, 2, 3].reduce((accu, el) => accu.concat(el), []);
"#,
        );
        let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();

        assert!(
            codes.contains(&2488),
            "expected TS2488 for destructuring the failed reduce result, got: {diagnostics:?}"
        );
        assert!(
            codes.contains(&2769),
            "expected TS2769 for the nested reduce/concat overload failure, got: {diagnostics:?}"
        );
    }
