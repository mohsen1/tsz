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

// --- renamed re-export through nested barrels ---
//
// Follow-up: a *renamed* re-export (`export type { Original as Renamed }`)
// forwarded again through `export *` keys the application base to the renaming
// hop rather than the declaration, so the receiver is not recognized as a
// non-lib interface application and the type argument is still dropped. This is
// the re-export-chain def-identity gap (resolution layer), distinct from the
// receiver-materialization path fixed here. Pinned (ignored) so it is tracked.
#[test]
#[ignore = "renamed re-export forwarded through export * keys the base def to the renaming hop (#13212 / #10663 resolution-layer follow-up)"]
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

// --- a member *inherited* via `extends` from a cross-file generic interface
// must substitute the heritage type argument. The heritage base is named through
// a cross-file import / re-export alias, whose own declaration (the import
// specifier) carries no type-parameter list; the base body still embeds the
// base's free type parameter, so with no params to bind the supplied arguments
// the heritage `instantiate_type` was a silent no-op and the inherited member
// stayed bound to the base's free `T` (false TS2322 on `c.value`). The merge now
// recovers the base's expanded body and matching params atomically so the
// substitution applies (#13767 / #13212 cross-file heritage residual).
//
// Binder names are varied across cases so no identifier is load-bearing.

#[test]
fn extends_reexported_generic_interface_inherited_member() {
    // Type-only barrel re-export of the generic base.
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

#[test]
fn extends_direct_cross_file_generic_interface_inherited_member() {
    // Direct cross-file import (no barrel hop), differently-named binders.
    assert_clean(
        &[
            ("./holder.ts", "export interface Holder<P> { slot: P; }\n"),
            (
                "./consumer.ts",
                "import type { Holder } from './holder';\ninterface Bag extends Holder<number> { label: string; }\ndeclare const b: Bag;\nconst n: number = b.slot;\n",
            ),
        ],
        "./consumer.ts",
    );
}

#[test]
fn extends_value_reexport_generic_interface_inherited_member() {
    // Value (`export { X } from`) barrel rather than type-only.
    assert_clean(
        &[
            ("./store.ts", "export interface Store<E> { entry: E; }\n"),
            ("./gateway.ts", "export { Store } from './store';\n"),
            (
                "./app.ts",
                "import { Store } from './gateway';\ninterface Shelf extends Store<string> { id: number; }\ndeclare const s: Shelf;\nconst v: string = s.entry;\n",
            ),
        ],
        "./app.ts",
    );
}

#[test]
fn extends_renamed_reexport_generic_interface_inherited_member() {
    // Renamed re-export: the import-site name differs from the declaration name.
    assert_clean(
        &[
            (
                "./origin.ts",
                "export interface Original<Q> { field: Q; }\n",
            ),
            (
                "./mid.ts",
                "export type { Original as Renamed } from './origin';\n",
            ),
            (
                "./client.ts",
                "import type { Renamed } from './mid';\ninterface Crate extends Renamed<number> { extra: string; }\ndeclare const c: Crate;\nconst n: number = c.field;\n",
            ),
        ],
        "./client.ts",
    );
}

#[test]
fn extends_cross_file_generic_interface_two_type_args() {
    // Two type parameters, both substituted through the heritage clause.
    assert_clean(
        &[
            (
                "./pair.ts",
                "export interface Pair<A, B> { left: A; right: B; }\n",
            ),
            ("./barrel.ts", "export type { Pair } from './pair';\n"),
            (
                "./use.ts",
                "import type { Pair } from './barrel';\ninterface Combo extends Pair<number, string> { tag: boolean; }\ndeclare const c: Combo;\nconst l: number = c.left;\nconst r: string = c.right;\n",
            ),
        ],
        "./use.ts",
    );
}

#[test]
#[ignore = "deeper residual: the inherited member reaches through TWO chained \
            cross-file generic interfaces (Crate extends Wrap<number>, \
            Wrap<V> extends Box<V>); the second hop's type argument is not yet \
            threaded through the chained heritage materialization (#13212)"]
fn extends_cross_file_generic_interface_two_level_chain() {
    // The inherited member reaches through *two* cross-file generic interfaces:
    // `Crate extends Wrap<number>` (cross-file) and `Wrap<V> extends Box<V>`
    // (also cross-file). The deepest member `item` must still resolve to number.
    assert_clean(
        &[
            ("./box.ts", "export interface Box<U> { item: U; }\n"),
            (
                "./wrap.ts",
                "import type { Box } from './box';\nexport interface Wrap<V> extends Box<V> { tag: string; }\n",
            ),
            (
                "./top.ts",
                "import type { Wrap } from './wrap';\ninterface Crate extends Wrap<number> { extra: boolean; }\ndeclare const c: Crate;\nconst n: number = c.item;\n",
            ),
        ],
        "./top.ts",
    );
}

#[test]
fn extends_same_file_generic_interface_inherited_member_control() {
    // Control: an all-local generic base was already correct; it must stay clean.
    assert_clean(
        &[(
            "./local.ts",
            "interface Container<T> { value: T; }\ninterface Crate extends Container<number> { extra: string; }\ndeclare const c: Crate;\nconst n: number = c.value;\n",
        )],
        "./local.ts",
    );
}

#[test]
fn extends_cross_file_generic_interface_wrong_arg_still_errors() {
    // Negative control: a genuine type-argument mismatch on the inherited member
    // must still report TS2322 (the substitution is applied, not skipped).
    let diags = check(
        &[
            ("./carrier.ts", "export interface Carrier<T> { load: T; }\n"),
            ("./hub.ts", "export type { Carrier } from './carrier';\n"),
            (
                "./bad.ts",
                "import type { Carrier } from './hub';\ninterface Crate extends Carrier<string> { extra: number; }\ndeclare const c: Crate;\nconst n: number = c.load;\n",
            ),
        ],
        "./bad.ts",
    );
    assert!(
        diags
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected TS2322 for string-vs-number mismatch on inherited member, got: {diags:?}"
    );
}
