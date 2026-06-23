//! Property access and assignability on a *generic interface reached through a
//! barrel re-export* must instantiate the interface with its type arguments.
//!
//! Structural rule: when the receiver of a property access is an application
//! `I<Args>` whose base `I` is a program-declared (non-lib) interface — including
//! when `I` is imported through a barrel that forwards it
//! (`export { I } from './decl'`, `export type { I } from './decl'`, or
//! `export *`) — `tsc` resolves the member against the instantiated body, so
//! `I<number>["value"]` is `number`. tsz previously evaluated such a re-exported
//! receiver through the lighter application evaluator, which left the cross-arena
//! application opaque, so the member resolved to the *unsubstituted* declared
//! member (a free `T`) and produced a false TS2322. The fix routes a non-lib
//! generic-interface application receiver through the env evaluator (the same
//! materialization the assignability path already uses), replacing a prior
//! hardcoded property-name (`"select"`) special case with a structural one.
//!
//! Refs #13212 / #10663. Binder names are varied across cases so no identifier
//! is load-bearing.

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::test_utils::{check_multi_file_with_libs_stamped, load_default_lib_files};
use tsz_common::common::ModuleKind;

fn opts() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::ESNext,
        strict: true,
        ..CheckerOptions::default()
    }
}

fn check(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs_stamped(files, entry, opts(), &libs)
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn assert_clean(files: &[(&str, &str)], entry: &str) {
    let diags = check(files, entry);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

// --- member access through each re-export form (consumer uses `import type`) ---

#[test]
fn member_access_through_value_reexport() {
    assert_clean(
        &[
            ("./decl.ts", "export interface Holder<T> { item: T; }\n"),
            ("./barrel.ts", "export { Holder } from './decl';\n"),
            (
                "./main.ts",
                "import type { Holder } from './barrel';\ndeclare const h: Holder<number>;\nconst n: number = h.item;\n",
            ),
        ],
        "./main.ts",
    );
}

#[test]
fn member_access_through_type_only_reexport() {
    assert_clean(
        &[
            ("./model.ts", "export interface Cell<V> { payload: V; }\n"),
            ("./index.ts", "export type { Cell } from './model';\n"),
            (
                "./consumer.ts",
                "import type { Cell } from './index';\ndeclare const c: Cell<string>;\nconst s: string = c.payload;\n",
            ),
        ],
        "./consumer.ts",
    );
}

#[test]
fn member_access_through_export_star() {
    assert_clean(
        &[
            (
                "./shapes.ts",
                "export interface Wrapper<E> { contents: E; }\n",
            ),
            ("./api.ts", "export * from './shapes';\n"),
            (
                "./app.ts",
                "import type { Wrapper } from './api';\ndeclare const w: Wrapper<boolean>;\nconst b: boolean = w.contents;\n",
            ),
        ],
        "./app.ts",
    );
}

// --- value-position consumer (`import { ... }`) ---

#[test]
fn member_access_value_consumer() {
    assert_clean(
        &[
            ("./base.ts", "export interface Box<U> { value: U; }\n"),
            ("./hub.ts", "export { Box } from './base';\n"),
            (
                "./use.ts",
                "import { Box } from './hub';\ndeclare const b: Box<number>;\nconst n: number = b.value;\n",
            ),
        ],
        "./use.ts",
    );
}

// --- renamed re-export (`export { Original as Renamed } from`) ---
//
// A renamed re-export resolves the chain to a declaration whose own name
// differs from the name used at the import site, so the type-reference lowering
// keyed the application base to the renaming *hop* (an alias symbol minted as a
// `TypeAlias` named "Renamed", carrying no type parameters) rather than to the
// `Original` interface declaration. `Renamed<number>` then stayed an opaque
// `Application` and its argument was never substituted, leaking a free type
// parameter to member reads (false TS2322). The lowering now canonicalizes a
// re-export alias that reaches a generic interface/class declaration to the
// declaration def (the same chase the entity-name and heritage paths use),
// keying `def_id_for_declaration_in_file` to the declaration's own name.
#[test]
fn member_access_through_renamed_nested_barrels() {
    assert_clean(
        &[
            (
                "./origin.ts",
                "export interface Original<P> { field: P; }\n",
            ),
            (
                "./mid.ts",
                "export type { Original as Renamed } from './origin';\n",
            ),
            ("./top.ts", "export * from './mid';\n"),
            (
                "./client.ts",
                "import type { Renamed } from './top';\ndeclare const r: Renamed<number>;\nconst n: number = r.field;\n",
            ),
        ],
        "./client.ts",
    );
}

// Rename applied at the import site (`import { Original as Renamed }`): the
// alias already carries `import_name = "Original"`, so this worked before — kept
// as a control that the canonicalization does not perturb it.
#[test]
fn member_access_through_renamed_direct_import() {
    assert_clean(
        &[
            (
                "./origin.ts",
                "export interface Original<P> { field: P; }\n",
            ),
            (
                "./client.ts",
                "import type { Original as Renamed } from './origin';\ndeclare const r: Renamed<number>;\nconst n: number = r.field;\n",
            ),
        ],
        "./client.ts",
    );
}

// Rename applied at a single named barrel, consumed directly (no `export *`):
// the minimal renamed-hop reproduction.
#[test]
fn member_access_through_renamed_single_barrel() {
    assert_clean(
        &[
            (
                "./origin.ts",
                "export interface Original<P> { field: P; }\n",
            ),
            (
                "./mid.ts",
                "export type { Original as Renamed } from './origin';\n",
            ),
            (
                "./client.ts",
                "import type { Renamed } from './mid';\ndeclare const r: Renamed<number>;\nconst n: number = r.field;\n",
            ),
        ],
        "./client.ts",
    );
}

// Control: the same two-hop `export *` chain *without* a rename was already
// clean; the rename is the only differing factor above.
#[test]
fn member_access_through_nonrenamed_nested_star() {
    assert_clean(
        &[
            (
                "./origin.ts",
                "export interface Original<P> { field: P; }\n",
            ),
            ("./mid.ts", "export type { Original } from './origin';\n"),
            ("./top.ts", "export * from './mid';\n"),
            (
                "./client.ts",
                "import type { Original } from './top';\ndeclare const r: Original<number>;\nconst n: number = r.field;\n",
            ),
        ],
        "./client.ts",
    );
}

// Control: same-file `interface Crate extends Container<number>` substitutes the
// inherited member correctly (the cross-file heritage case below does not).
#[test]
fn extends_same_file_generic_interface_inherited_member() {
    assert_clean(
        &[(
            "./hmain.ts",
            "interface Container<T> { value: T; }\ninterface Crate extends Container<number> { extra: string; }\ndeclare const c: Crate;\nconst n: number = c.value;\n",
        )],
        "./hmain.ts",
    );
}

// --- assignment TO a re-exported generic interface ---

#[test]
fn assignment_to_reexported_generic_interface() {
    assert_clean(
        &[
            ("./types.ts", "export interface Slot<T> { stored: T; }\n"),
            ("./bundle.ts", "export type { Slot } from './types';\n"),
            (
                "./writer.ts",
                "import type { Slot } from './bundle';\nconst s: Slot<number> = { stored: 1 };\n",
            ),
        ],
        "./writer.ts",
    );
}

// --- controls ---

#[test]
fn direct_import_control() {
    assert_clean(
        &[
            ("./d.ts", "export interface Pair<K> { key: K; }\n"),
            (
                "./m.ts",
                "import type { Pair } from './d';\ndeclare const p: Pair<number>;\nconst n: number = p.key;\n",
            ),
        ],
        "./m.ts",
    );
}

#[test]
fn nongeneric_reexport_control() {
    assert_clean(
        &[
            ("./plain.ts", "export interface Flat { amount: number; }\n"),
            ("./reexport.ts", "export type { Flat } from './plain';\n"),
            (
                "./reader.ts",
                "import type { Flat } from './reexport';\ndeclare const f: Flat;\nconst n: number = f.amount;\n",
            ),
        ],
        "./reader.ts",
    );
}

// --- negative control: a genuine type-argument mismatch must still error ---

#[test]
fn wrong_type_argument_still_errors() {
    let diags = check(
        &[
            ("./g.ts", "export interface Carrier<T> { load: T; }\n"),
            ("./gateway.ts", "export type { Carrier } from './g';\n"),
            (
                "./bad.ts",
                "import type { Carrier } from './gateway';\ndeclare const c: Carrier<string>;\nconst n: number = c.load;\n",
            ),
        ],
        "./bad.ts",
    );
    assert!(
        diags
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected TS2322 for string-vs-number mismatch, got: {diags:?}"
    );
}

// --- documented follow-up: a member *inherited* from a generic base declared
// in another file (`interface Local extends Base<number>`) still drops the type
// argument. This is NOT re-export-specific — it reproduces with a *direct*
// cross-file import too (see `extends_same_file_generic_interface_inherited_member`
// for the passing same-file control), so the barrel here is incidental. The
// owner is the cross-file generic-interface *heritage* materialization path
// (#13767 / #13803 family), distinct from the type-reference application path
// fixed in this change. Pinned (ignored) so the remaining gap is tracked. ---

#[test]
#[ignore = "inherited-via-heritage member of a cross-file generic base needs cross-file heritage materialization; reproduces with a direct import too (#13767 / #13212)"]
fn extends_reexported_generic_interface_inherited_member() {
    assert_clean(
        &[
            (
                "./hbase.ts",
                "export interface Container<T> { value: T; }\n",
            ),
            (
                "./hbarrel.ts",
                "export type { Container } from './hbase';\n",
            ),
            (
                "./hmain.ts",
                "import type { Container } from './hbarrel';\ninterface Crate extends Container<number> { extra: string; }\ndeclare const c: Crate;\nconst n: number = c.value;\n",
            ),
        ],
        "./hmain.ts",
    );
}
