use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn compile_entry_file(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

fn compile_entry_file_with_es5_lib(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    let libs = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    tsz_checker::test_utils::check_multi_file_with_libs(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

#[test]
fn exported_undefined_alias_does_not_shadow_intrinsic_undefined_in_other_module() {
    let zod_like_exports = r#"
const undefinedType = (params?: {}) => params;
export { undefinedType as undefined };
"#;

    let zod_like_util = r#"
export function find<T>(value: T): T | undefined {
    if (false) return value;
    return undefined;
}
"#;

    let diagnostics = compile_entry_file(
        &[
            ("types.ts", zod_like_exports),
            ("helpers/util.ts", zod_like_util),
        ],
        1,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();

    assert!(
        !codes.contains(&2322),
        "exported alias named undefined from another module must not shadow intrinsic undefined; got {diagnostics:#?}"
    );
}

#[test]
fn imported_numeric_boolean_alias_indexes_type_literal_maps() {
    let diagnostics = compile_entry_file(
        &[
            ("Boolean/_Internal.ts", "export type Boolean = 0 | 1;\n"),
            (
                "Boolean/And.ts",
                r#"
import {Boolean} from './_Internal';

export type And<B1 extends Boolean, B2 extends Boolean> = {
    0: {
      0: 0
      1: 0
    }
    1: {
      0: 0
      1: 1
    }
}[B1][B2];
"#,
            ),
        ],
        1,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2536),
        "imported numeric Boolean alias should index Boolean maps without TS2536: {diagnostics:#?}"
    );
}

#[test]
fn imported_numeric_boolean_alias_indexes_type_literal_maps_with_libs() {
    let diagnostics = compile_entry_file_with_es5_lib(
        &[
            ("Boolean/_Internal.ts", "export type Boolean = 0 | 1;\n"),
            (
                "Boolean/And.ts",
                r#"
import {Boolean} from './_Internal';

export type And<B1 extends Boolean, B2 extends Boolean> = {
    0: {
      0: 0
      1: 0
    }
    1: {
      0: 0
      1: 1
    }
}[B1][B2];
"#,
            ),
        ],
        1,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2536),
        "imported numeric Boolean alias should shadow global Boolean when libs are loaded: {diagnostics:#?}"
    );
}

#[test]
fn imported_numeric_boolean_alias_validates_type_argument_constraints_with_libs() {
    let diagnostics = compile_entry_file_with_es5_lib(
        &[
            (
                "Any/Key.ts",
                "export type Key = string | number | symbol;\n",
            ),
            ("Boolean/_Internal.ts", "export type Boolean = 0 | 1;\n"),
            (
                "List/List.ts",
                "export interface List<A = any> extends ReadonlyArray<A> {}\n",
            ),
            (
                "List/ObjectOf.ts",
                r#"
import {List} from './List';

export type ObjectOf<L extends List> = {
    [K in keyof L]: L[K]
};
"#,
            ),
            (
                "Object/Either.ts",
                r#"
import {Boolean} from '../Boolean/_Internal';
import {Key} from '../Any/Key';

type __Either<O, K extends Key> =
    ({
        [P in K & keyof O]: O[P]
    }[K & keyof O]);

type EitherStrict<O, K extends Key> = __Either<O, K>;
type EitherLoose<O, K extends Key> = __Either<O, K>;

export type _Either<O, K extends Key, strict extends Boolean> = {
    1: EitherStrict<O, K>
    0: EitherLoose<O, K>
}[strict];

export type Either<O, K extends Key, strict extends Boolean = 1> =
    O extends unknown
    ? _Either<O, K, strict>
    : never;
"#,
            ),
            (
                "List/Either.ts",
                r#"
import {Key} from '../Any/Key';
import {Boolean} from '../Boolean/_Internal';
import {Either as OEither} from '../Object/Either';
import {ObjectOf} from './ObjectOf';
import {List} from './List';

export type Either<strict extends Boolean = 1> = strict;
export type UseZero = Either<0>;
export type UseOne = Either<1>;
export type UseGeneric<T extends Boolean> = Either<T>;
export type UseImportedTarget<T extends Boolean> = OEither<{a: 1}, 'a', T>;
export type ListEither<strict extends Boolean = 1> = OEither<{a: 1}, 'a', strict>;
export type RealEither<L extends List, K extends Key, strict extends Boolean = 1> =
    OEither<ObjectOf<L>, `${K & number}` | K, strict>;
"#,
            ),
        ],
        5,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2344),
        "imported numeric Boolean alias should be used for type argument constraints: {diagnostics:#?}"
    );
}

#[test]
fn imported_string_aliases_index_nested_type_literal_maps() {
    let diagnostics = compile_entry_file(
        &[
            (
                "Function/_Internal.ts",
                "export type Mode = 'sync' | 'async';\nexport type Input = 'multi' | 'list';\n",
            ),
            (
                "Function/Compose.ts",
                r#"
import {Input, Mode} from './_Internal';

type ComposeMultiSync = { syncMulti: true };
type ComposeListSync = { syncList: true };
type ComposeMultiAsync = { asyncMulti: true };
type ComposeListAsync = { asyncList: true };

export type Compose<mode extends Mode = 'sync', input extends Input = 'multi'> = {
    'sync' : {
        'multi': ComposeMultiSync
        'list' : ComposeListSync
    }
    'async': {
        'multi': ComposeMultiAsync
        'list' : ComposeListAsync
    }
}[mode][input];
"#,
            ),
        ],
        1,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2536),
        "imported string aliases should index nested maps without TS2536: {diagnostics:#?}"
    );
}

#[test]
fn in_module_renamed_export_does_not_shadow_string_mapping_intrinsic() {
    // `export { Local as Capitalize }` records `Capitalize` only on the module's
    // export surface — never as an in-module type binding — so an in-module
    // reference to `Capitalize` resolves to the string-mapping intrinsic, not the
    // non-generic local. tsc is clean here; tsz used to emit TS2315.
    let diagnostics = compile_entry_file(
        &[(
            "hkt.ts",
            r#"
export type Cap = Capitalize<'a'>;
interface Local { tag: 'kind' }
export { Local as Capitalize };
"#,
        )],
        0,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2315),
        "renamed export must not shadow the Capitalize intrinsic in-module: {diagnostics:#?}"
    );
}

#[test]
fn in_module_type_only_renamed_export_does_not_shadow_string_mapping_intrinsic() {
    // Same as above for the `export type { Local as Capitalize }` form: the
    // type-only renamed export must not create an in-module type binding.
    let diagnostics = compile_entry_file(
        &[(
            "hkt.ts",
            r#"
export type Cap = Capitalize<'a'>;
interface Local { tag: 'kind' }
export type { Local as Capitalize };
"#,
        )],
        0,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2315),
        "type-only renamed export must not shadow the Capitalize intrinsic: {diagnostics:#?}"
    );
}

#[test]
fn in_module_renamed_export_does_not_shadow_global_value_or_type() {
    // A renamed export aliasing a name that also exists as a global must not
    // shadow that global inside the same module (value or type space).
    let diagnostics = compile_entry_file_with_es5_lib(
        &[(
            "shadow.ts",
            r#"
const x = 1;
export { x as Array };
export { x as parseInt };
const list: Array<number> = [1, 2, 3];
const parsed = parseInt("3");
export type Use = typeof list;
export type Use2 = typeof parsed;
"#,
        )],
        0,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == 2315 || *code == 2304 || *code == 2349),
        "renamed export must not shadow global Array/parseInt in-module: {diagnostics:#?}"
    );
}

#[test]
fn in_module_renamed_export_name_is_not_an_in_module_binding() {
    // The renamed export name is NOT introduced as a usable in-module name; an
    // in-module reference to it (when it is not a global) is TS2304, matching tsc.
    let diagnostics = compile_entry_file(
        &[(
            "rename.ts",
            r#"
interface Local { tag: 'kind' }
export { Local as Renamed };
type X = Renamed;
"#,
        )],
        0,
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2304),
        "in-module use of a renamed export name must be TS2304: {diagnostics:#?}"
    );
}

#[test]
fn cross_file_renamed_exports_still_resolve() {
    // The fix keeps renamed exports on the public export surface, so cross-file
    // imports of renamed type/value/class exports continue to resolve cleanly.
    let diagnostics = compile_entry_file(
        &[
            (
                "mod.ts",
                r#"
interface Local { tag: 'kind' }
const val = 42;
class Cls { m() {} }
export { Local as RenamedType };
export { val as renamedVal };
export { Cls as RenamedCls };
export type { Local as TypeOnlyRenamed };
"#,
            ),
            (
                "consumer.ts",
                r#"
import { RenamedType, renamedVal, RenamedCls, TypeOnlyRenamed } from './mod';
const a: RenamedType = { tag: 'kind' };
const b: number = renamedVal;
const c: RenamedCls = new RenamedCls();
const d: TypeOnlyRenamed = { tag: 'kind' };
export { a, b, c, d };
"#,
            ),
        ],
        1,
    );

    assert!(
        diagnostics.is_empty(),
        "cross-file imports of renamed exports must resolve cleanly: {diagnostics:#?}"
    );
}
