//! TS1361 vs TS1362 attribution across multi-file alias chains.
//!
//! Invariant: when a value use of `X` is rejected because `X` is type-only,
//! the checker walks the alias chain and attributes the error to the
//! innermost declaration that *directly* uses a `type` keyword —
//! `import type`, `export type`, `import { type X }`, `export { type X }`,
//! or `import type X = require(...)`. A plain `export { Y as X }` or
//! `import { X }` that merely re-exports or re-imports a type-only symbol
//! is not a direct marker — the chain walk must continue past it.
//!
//! Reproduces `conformance/externalModules/typeOnly/chained.ts`.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn compile_module_files(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code != 2318)
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// Chain: `export type {{ A as B }}` → plain `export {{ B as C }}` →
/// `import type {{ C }}` + plain `export {{ C as D }}` → `import {{ D }}; new D()`.
///
/// `tsc` attributes the error to the innermost `import type` (TS1361), not
/// the outermost `export type` (TS1362). The plain re-exports in between
/// are not direct markers.
#[test]
fn chained_type_only_attributes_to_innermost_import_type() {
    let a = r#"
class A { a!: string }
export type { A as B };
export type Z = A;
"#;
    let b = r#"
import { Z as Y } from './a';
export { B as C } from './a';
"#;
    let c = r#"
import type { C } from './b';
export { C as D };
"#;
    let d = r#"
import { D } from './c';
new D();
"#;

    let diagnostics = compile_module_files(
        &[("./a.ts", a), ("./b.ts", b), ("./c.ts", c), ("./d.ts", d)],
        3,
    );

    let ts1361 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1361)
        .collect::<Vec<_>>();
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();

    assert_eq!(
        ts1361.len(),
        1,
        "Expected exactly one TS1361 attributing to the `import type` marker. \
         Got TS1361={ts1361:?}, TS1362={ts1362:?}, all={diagnostics:?}",
    );
    assert!(
        ts1362.is_empty(),
        "Did not expect TS1362 — the outer `export type` is not the nearest direct marker. \
         Got TS1362={ts1362:?}, all={diagnostics:?}",
    );
}

/// Sanity check: when the only direct type-only marker in the chain is
/// `export type`, TS1362 is still the correct attribution.
#[test]
fn export_type_only_chain_still_attributes_to_export_type() {
    let a = r#"
class A { a!: string }
export type { A as B };
"#;
    let b = r#"
import { B } from './a';
new B();
"#;

    let diagnostics = compile_module_files(&[("./a.ts", a), ("./b.ts", b)], 1);

    let ts1361 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1361)
        .collect::<Vec<_>>();
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();

    assert!(
        ts1361.is_empty(),
        "Did not expect TS1361 when no `import type` appears in the chain. \
         Got TS1361={ts1361:?}, all={diagnostics:?}",
    );
    assert_eq!(
        ts1362.len(),
        1,
        "Expected TS1362 for `export type` chain. \
         Got TS1362={ts1362:?}, all={diagnostics:?}",
    );
}

#[test]
fn imported_interface_used_as_value_reports_ts2693_not_ts1362() {
    let a = r#"
export interface Animal {
  legs: number;
}
"#;
    let b = r#"
import { Animal } from "./a";
const x = Animal;
"#;

    let diagnostics = compile_module_files(&[("./a.ts", a), ("./b.ts", b)], 1);

    let ts2693 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2693)
        .collect::<Vec<_>>();
    let ts1362 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 1362)
        .collect::<Vec<_>>();

    assert_eq!(
        ts2693.len(),
        1,
        "Expected TS2693 for imported interface used as a value. \
         Got TS2693={ts2693:?}, TS1362={ts1362:?}, all={diagnostics:?}",
    );
    assert!(
        ts1362.is_empty(),
        "An intrinsic type-only export should not be attributed to `export type`. \
         Got TS1362={ts1362:?}, all={diagnostics:?}",
    );
}

#[test]
fn imported_type_alias_used_as_value_reports_ts2693_not_ts1362() {
    let a = r#"
export type Animal = {
  legs: number;
};
"#;
    let b = r#"
import { Animal } from "./a";
const x = Animal;
"#;

    let diagnostics = compile_module_files(&[("./a.ts", a), ("./b.ts", b)], 1);

    let ts2693 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2693)
        .collect::<Vec<_>>();
    let ts1362 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 1362)
        .collect::<Vec<_>>();

    assert_eq!(
        ts2693.len(),
        1,
        "Expected TS2693 for imported type alias used as a value. \
         Got TS2693={ts2693:?}, TS1362={ts1362:?}, all={diagnostics:?}",
    );
    assert!(
        ts1362.is_empty(),
        "A type alias declaration should not be attributed to `export type`. \
         Got TS1362={ts1362:?}, all={diagnostics:?}",
    );
}

/// `export { type X, Y } from "./mod"` carries a *per-specifier* `type`
/// modifier — only `X` is type-only, `Y` is value-bearing. Before the fix the
/// re-export branch dropped the per-spec flag, so a downstream `import { X }
/// from "./barrel"; new X();` silently succeeded instead of reporting
/// `TS1362`.
///
/// Structural rule: a re-export specifier marked `type` must mark the
/// synthesized alias type-only, independent of whether the enclosing
/// declaration is `export {}` or `export type {}`.
#[test]
fn reexport_per_specifier_type_modifier_emits_ts1362() {
    let source = r#"
export class Foo { x = 1 }
export class Bar { y = 1 }
"#;
    let barrel = r#"
export { type Foo, Bar } from "./source";
"#;
    let user = r#"
import { Foo, Bar } from "./barrel";
new Foo();
new Bar();
"#;

    let diagnostics = compile_module_files(
        &[
            ("./source.ts", source),
            ("./barrel.ts", barrel),
            ("./user.ts", user),
        ],
        2,
    );

    let ts1362: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 1362).collect();
    assert_eq!(
        ts1362.len(),
        1,
        "Expected exactly one TS1362 for value use of per-spec type re-export. \
         Got TS1362={ts1362:?}, all={diagnostics:?}",
    );
    assert!(
        ts1362[0].1.contains("Foo"),
        "TS1362 should attribute to the type-only spec `Foo`, not its value-spec sibling `Bar`. \
         Got message={:?}",
        ts1362[0].1,
    );
}

/// Two barrels re-exporting the same source module — one with per-spec
/// `type`, one without — must produce different value-meaning answers.
/// Before the fix the per-spec type-only flag was dropped, so both barrels
/// resolved `Foo` as a value (TS1362 missed) and the modules "diverged in
/// identity" — same name, same source, but type-only-ness inconsistent.
#[test]
fn type_only_and_value_barrels_for_same_source_disagree_on_value_meaning() {
    let source = r#"
export class Foo { x = 1 }
"#;
    let barrel_type = r#"
export { type Foo } from "./source";
"#;
    let barrel_value = r#"
export { Foo } from "./source";
"#;
    let user = r#"
import { Foo as FooT } from "./barrel_type";
import { Foo as FooV } from "./barrel_value";
new FooT(); // TS1362 — type-only barrel
new FooV(); // OK
"#;

    let diagnostics = compile_module_files(
        &[
            ("./source.ts", source),
            ("./barrel_type.ts", barrel_type),
            ("./barrel_value.ts", barrel_value),
            ("./user.ts", user),
        ],
        3,
    );

    let ts1362: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 1362).collect();
    assert_eq!(
        ts1362.len(),
        1,
        "Expected exactly one TS1362 — only the type-only barrel side should reject `new FooT()`. \
         Got TS1362={ts1362:?}, all={diagnostics:?}",
    );
    assert!(
        ts1362[0].1.contains("FooT"),
        "TS1362 must point at `FooT` (the type-only-barrel alias), not at `FooV`. \
         Got message={:?}",
        ts1362[0].1,
    );
}

#[test]
fn import_type_equals_require_value_use_reports_ts1361() {
    let foo = r#"
class Foo {
  value = "";
}
export = Foo;
"#;
    let bar = r#"
import type Foo = require("./foo");

new Foo();
let value: Foo | undefined;
"#;

    let diagnostics = compile_module_files(&[("./foo.ts", foo), ("./bar.ts", bar)], 1);

    let ts1361 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 1361)
        .collect::<Vec<_>>();
    let ts2693 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2693)
        .collect::<Vec<_>>();

    assert_eq!(
        ts1361.len(),
        1,
        "Expected one TS1361 for value use of `import type Foo = require(...)`. \
         Got TS1361={ts1361:?}, all={diagnostics:?}",
    );
    assert!(
        ts2693.is_empty(),
        "A resolved type-only import-equals alias should not fall back to TS2693. \
         Got TS2693={ts2693:?}, all={diagnostics:?}",
    );
}
