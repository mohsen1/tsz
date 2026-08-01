use tsz_checker::context::CheckerOptions;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn declaration_diagnostic_codes(files: &[(&str, &str)], entry_file: &str) -> Vec<u32> {
    declaration_diagnostic_codes_with_module(files, entry_file, ModuleKind::NodeNext)
}

fn declaration_diagnostic_codes_with_module(
    files: &[(&str, &str)],
    entry_file: &str,
    module: ModuleKind,
) -> Vec<u32> {
    // `Object.assign` is declared in `es2015.core.d.ts`, not `es5.d.ts`; the
    // portability check this file exercises can only fire when the call
    // resolves. `_stamped` wires `global_symbol_file_index` so cross-file
    // symbol lookups match the driver's production fidelity instead of
    // falling back to the order-dependent dynamic overlay.
    let libs = tsz_checker::test_utils::load_lib_files(&["es5.d.ts", "es2015.core.d.ts"]);
    tsz_checker::test_utils::check_multi_file_with_libs_stamped(
        files,
        entry_file,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module,
            strict: true,
            emit_declarations: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn local_object_assign_does_not_trigger_builtin_object_assign_portability_check() {
    let codes = declaration_diagnostic_codes(
        &[
            (
                "/node_modules/pkg/node_modules/inner/index.d.ts",
                r#"
export interface Hidden {
  value: string;
}
"#,
            ),
            (
                "/src/index.ts",
                r#"
import type { Hidden } from "../node_modules/pkg/node_modules/inner";
declare const hiddenValue: Hidden;
const Object = {
  assign<T, U>(target: T, source: U): T & U {
    return target as T & U;
  }
};

export default Object.assign({}, { hiddenValue });
"#,
            ),
        ],
        "/src/index.ts",
    );

    assert!(
        !codes.contains(&2883),
        "local Object.assign must not be treated as the built-in Object.assign declaration-emit portability path: {codes:?}"
    );
}

#[test]
fn builtin_object_assign_still_reports_nonportable_default_export() {
    let codes = declaration_diagnostic_codes(
        &[
            (
                "/node_modules/pkg/node_modules/inner/index.d.ts",
                r#"
export interface Hidden {
  value: string;
}
"#,
            ),
            (
                "/src/index.ts",
                r#"
import type { Hidden } from "../node_modules/pkg/node_modules/inner";
declare const hiddenValue: Hidden;

export default Object.assign({}, { hiddenValue });
"#,
            ),
        ],
        "/src/index.ts",
    );

    assert!(
        codes.contains(&2883),
        "built-in Object.assign should still use the declaration-emit portability path: {codes:?}"
    );
}

// Harness-only false failure (#15983): a minimal reduction of this fixture
// (drop the mapped type / tagged template, keep only "outer" doing
// `import * as x from "<cjs export= dep>"` alongside its own
// `export default`) makes a *default* import of "outer" from a third file
// resolve to the re-exported CJS dependency's namespace instead of outer's
// own default export, inside this crate's isolated per-file
// `check_multi_file_with_libs*` harness. The same reduction built as real
// files on disk and run through the actual pipeline is clean in both
// compilers: `tsc@7.0.2 -p tsconfig.json` exits 0, and
// `.target/release/tsz -p tsconfig.json --noEmit` exits 0 too — so
// production's driver-computed module resolution does not take this path;
// only the harness's per-file-binder + `build_module_resolution_maps`
// reconstruction does, even with `check_multi_file_with_libs_stamped`'s
// production-faithful `global_symbol_file_index` wired up. Not re-scoped
// further this session; the next probe is comparing the harness's
// `resolved_module_paths`/`global_symbol_file_index` against what the real
// `tsz-core` driver builds for the same nested-`node_modules` layout with a
// re-exported `export =` dependency, to find which input the harness
// constructs differently.
#[test]
#[ignore = "harness-only false failure, see comment above (#15983)"]
fn builtin_object_assign_reports_nonportable_tagged_template_mapped_alias_default_export() {
    let codes = declaration_diagnostic_codes_with_module(
        &[
            (
                "/node_modules/outer/node_modules/inner/index.d.ts",
                r#"
interface LocalStatics {
  hidden: string;
}
declare namespace innerStatics {
  type PickPublic<T> = { [K in Exclude<keyof T, keyof LocalStatics>]: T[K] };
}
export = innerStatics;
"#,
            ),
            (
                "/node_modules/outer/index.d.ts",
                r#"
import * as innerStatics from "inner";
export interface BaseShape {
  tag: string;
}
export type PublicShape<T extends string> =
  string
  & BaseShape
  & innerStatics.PickPublic<T>;
export interface Factory {
  div(a: TemplateStringsArray): PublicShape<"renamed">;
}
declare const factory: Factory;
export default factory;
"#,
            ),
            (
                "/src/index.ts",
                r#"
import factory from "outer";

const first = factory.div``;
const second = factory.div``;
export const third = factory.div``;

export default Object.assign(first, {
  second,
  third,
});
"#,
            ),
        ],
        "/src/index.ts",
        ModuleKind::CommonJS,
    );

    assert!(
        codes.contains(&2883),
        "built-in Object.assign should report TS2883 for tagged-template nested mapped aliases from transitive dependencies: {codes:?}"
    );
}
