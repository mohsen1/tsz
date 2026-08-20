//! Computed/`const`-keyed members in interface declarations and `declare
//! global` augmentations must resolve for property access, matching tsc.
//!
//! Two structural rules exercised here:
//!
//! 1. A computed property name `[K]` whose `K` is a **type-only import**
//!    (`import type { K }`) of a value `const K = 'x'` resolves to the property
//!    name `x` via the binding's declared literal type. The value-position
//!    evaluation has no value meaning and yields no key, so without the
//!    declared-type fallback the member is dropped (false TS2339). Owner:
//!    `state/type_resolution/computed_property_names.rs`
//!    (`resolve_local_computed_property_name`).
//!
//! 2. The same member declared in `declare global { interface Window { [K]: T } }`
//!    must be found when accessing it on a value of type `Window & typeof
//!    globalThis` (e.g. the lib's `window`). The lib `Window` type carries no
//!    augmentation members, so the globalThis property path must consult the
//!    augmentation map. Owners: `state/type_environment/type_node_resolution.rs`
//!    (`resolve_global_this_property_type`), `types/property_access_augmentation.rs`
//!    (computed-name precompute + intersection-arm lookup),
//!    `tsz-lowering signature_members.rs` (literal computed key in merged
//!    interface lowering).
//!
//! Witness: tanstack-router `packages/router-core/src/ssr/ssr-client.ts`
//! (`declare global { interface Window { [GLOBAL_TSR]?: TsrSsrGlobal } }` with
//! `import type { GLOBAL_TSR } from './constants'`).

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file, check_multi_file_with_libs_stamped, load_lib_files};
use tsz_common::common::ModuleKind;

const TS2339: u32 = 2339; // Property does not exist.
const TS2322: u32 = 2322; // Type X is not assignable to type Y.

fn strict() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::ESNext,
        strict: true,
        ..CheckerOptions::default()
    }
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn count(diags: &[crate::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

#[test]
fn type_only_import_const_computed_key_resolves_member() {
    // `import type { GLOBAL_TSR }` brings in only the type side of a value
    // `const GLOBAL_TSR = '$_TSR'`; `[GLOBAL_TSR]` must still key the member
    // under `$_TSR`. Accessing `f.$_TSR!.x` (number) as a string proves the
    // member resolved with its real type (TS2322), not dropped (TS2339).
    let constants = "export const GLOBAL_TSR = '$_TSR'\n";
    let main = r#"
import type { GLOBAL_TSR } from './constants'
interface Foo { [GLOBAL_TSR]?: { x: number } }
declare const f: Foo
const n: string = f.$_TSR!.x
"#;
    let diags = check_multi_file(
        &[("main.ts", main), ("constants.ts", constants)],
        "main.ts",
        strict(),
    );
    assert_eq!(
        count(&diags, TS2339),
        0,
        "type-only-import computed key must resolve (no TS2339): {:?}",
        codes(&diags)
    );
    assert_eq!(
        count(&diags, TS2322),
        1,
        "resolved member keeps its `number` type (string = number is TS2322): {:?}",
        codes(&diags)
    );
}

#[test]
fn type_only_import_const_computed_key_does_not_overbroaden() {
    // The resolved member must not turn the interface into an index signature:
    // an absent property still errors TS2339.
    let constants = "export const GLOBAL_TSR = '$_TSR'\n";
    let main = r#"
import type { GLOBAL_TSR } from './constants'
interface Foo { [GLOBAL_TSR]?: { x: number } }
declare const f: Foo
f.totallyAbsentProperty
"#;
    let diags = check_multi_file(
        &[("main.ts", main), ("constants.ts", constants)],
        "main.ts",
        strict(),
    );
    assert_eq!(
        count(&diags, TS2339),
        1,
        "absent property must still error TS2339 (member is not an index sig): {:?}",
        codes(&diags)
    );
}

#[test]
fn value_import_const_computed_key_still_resolves() {
    // Control: the value-import form (already worked) keeps working.
    let constants = "export const GLOBAL_TSR = '$_TSR'\n";
    let main = r#"
import { GLOBAL_TSR } from './constants'
interface Foo { [GLOBAL_TSR]?: { x: number } }
declare const f: Foo
const n: string = f.$_TSR!.x
"#;
    let diags = check_multi_file(
        &[("main.ts", main), ("constants.ts", constants)],
        "main.ts",
        strict(),
    );
    assert_eq!(count(&diags, TS2339), 0, "{:?}", codes(&diags));
    assert_eq!(count(&diags, TS2322), 1, "{:?}", codes(&diags));
}

#[test]
fn declare_global_window_computed_key_resolves_through_globalthis() {
    // A `declare global { interface Window { [K]?: T } }` augmentation keyed by a
    // `const` must be found when accessing the member on the lib's `window`
    // (whose type is `Window & typeof globalThis`). Reading `window` now resolves
    // its real intersection type instead of `any` (see #14742), so the
    // augmentation must be consulted through the `Window` arm.
    //
    // The cross-file (`import type { K } from './c'`) form of this exact witness
    // (tanstack-router) needs the full driver's cross-file resolution ordering to
    // fold the computed key, which the unit harness does not replicate; it is
    // verified end-to-end via the CLI. Here the `const` is same-file so the test
    // exercises the window-surface augmentation path faithfully. The stamped
    // helper is required because the `typeof globalThis` surface keys member
    // provenance off `decl_file_idx`.
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    if libs.iter().all(|l| l.file_name != "dom.d.ts") {
        // DOM lib not present in this checkout: the core fix is still covered by
        // the no-lib tests above; skip the end-to-end variant rather than fail.
        return;
    }
    let main = r#"
const GLOBAL_TSR = '$_TSR'
declare global {
  interface Window {
    [GLOBAL_TSR]?: { x: number }
  }
}
const n: string = window.$_TSR!.x
export {}
"#;
    let diags =
        check_multi_file_with_libs_stamped(&[("main.ts", main)], "main.ts", strict(), &libs);
    assert_eq!(
        count(&diags, TS2339),
        0,
        "window augmentation member must resolve through `Window & typeof globalThis`: {:?}",
        codes(&diags)
    );
    assert_eq!(
        count(&diags, TS2322),
        1,
        "resolved augmentation member keeps its `number` type: {:?}",
        codes(&diags)
    );
}

#[test]
fn declare_global_window_direct_string_literal_key_resolves() {
    // The direct string-literal form of the same augmentation member
    // (`['$_TSR']`) must also resolve through the merged-interface lowering.
    // Stamped helper required: see the computed-key sibling test above.
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    if libs.iter().all(|l| l.file_name != "dom.d.ts") {
        return;
    }
    let main = r#"
declare global {
  interface Window {
    ['$_TSR']?: { x: number }
  }
}
const n: string = window.$_TSR!.x
export {}
"#;
    let diags =
        check_multi_file_with_libs_stamped(&[("main.ts", main)], "main.ts", strict(), &libs);
    assert_eq!(
        count(&diags, TS2339),
        0,
        "direct-literal window augmentation key must resolve: {:?}",
        codes(&diags)
    );
    assert_eq!(count(&diags, TS2322), 1, "{:?}", codes(&diags));
}
