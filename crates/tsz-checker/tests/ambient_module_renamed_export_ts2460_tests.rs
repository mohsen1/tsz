//! TS2460 parity for `declare module "X" { ... export { Orig as Renamed } }`
//! (regression for #9775).
//!
//! Structural rule (one sentence):
//!
//! > When `import { N } from "M"` resolves to a module whose body — either a
//! > file module or a `declare module "M" { ... }` ambient block — declares
//! > `N` locally but only exports it under another name via
//! > `export { N as Other }` (without a `from` clause), tsc emits TS2460
//! > "Module 'M' declares 'N' locally, but it is exported as 'Other'"; this
//! > change makes tsz emit the same diagnostic in both surfaces.
//!
//! Until #9775 was fixed, tsz only detected the file-module form; the
//! ambient-module form silently bound the import to the local declaration
//! and emitted no diagnostic.
//!
//! Every test below varies at least one user-chosen name (interface/class
//! name, alias name, module spec) so the fix is structural rather than
//! shape-fingerprinted.
//!
//! ## Test-harness note
//!
//! `check_multi_file` resolves ambient-module specifiers through
//! `build_module_resolution_maps`, which registers the same-directory bare
//! alias (`./<stem>` ↔ `<stem>`) so the importer's `resolve_import_target`
//! lookup succeeds. The tests below therefore name the declaring file with a
//! stem equal to the ambient module specifier (e.g. `rn.d.ts` for
//! `declare module "rn"`). This is a harness limitation only — the CLI
//! resolves ambient modules through `global_module_binder_index` regardless
//! of file stem (verified manually against the live `tsz` binary on the
//! repro from #9775).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

const TS2460: u32 = 2460;
const TS2305: u32 = 2305;

fn ambient_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..Default::default()
    }
}

fn diagnostics(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check_multi_file(files, entry, ambient_options())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

// ───────────────────────── 1. reported repro ──────────────────────────────

/// Reported case: importing the original name from a `declare module "X"`
/// that renames it on export must emit TS2460.
#[test]
fn ambient_module_interface_renamed_export_imported_by_original_name_emits_ts2460() {
    let diags = diagnostics(
        &[
            (
                "rn.d.ts",
                r#"
declare module "rn" {
  interface Orig { v: number; }
  export { Orig as Renamed };
}
"#,
            ),
            (
                "rnuse.ts",
                r#"
import { Orig } from "rn";
const r: Orig = { v: 1 };
"#,
            ),
        ],
        "rnuse.ts",
    );
    assert!(
        diags.iter().any(|(c, m)| *c == TS2460
            && m.contains("\"rn\"")
            && m.contains("'Orig'")
            && m.contains("'Renamed'")),
        "expected TS2460 about 'rn' / 'Orig' as 'Renamed'; got {diags:#?}"
    );
}

// ───────────────────── 2. value (const + function) ─────────────────────────

/// Same rule applies to a renamed `const` export.
#[test]
fn ambient_module_const_renamed_export_imported_by_original_name_emits_ts2460() {
    let diags = diagnostics(
        &[
            (
                "vmod.d.ts",
                r#"
declare module "vmod" {
  const value: number;
  export { value as v };
}
"#,
            ),
            (
                "vuse.ts",
                r#"
import { value } from "vmod";
const x: number = value;
"#,
            ),
        ],
        "vuse.ts",
    );
    assert!(
        diags.iter().any(|(c, m)| *c == TS2460
            && m.contains("\"vmod\"")
            && m.contains("'value'")
            && m.contains("'v'")),
        "expected TS2460 about 'vmod' / 'value' as 'v'; got {diags:#?}"
    );
}

/// Same rule applies to a renamed `function` export.
#[test]
fn ambient_module_function_renamed_export_imported_by_original_name_emits_ts2460() {
    let diags = diagnostics(
        &[
            (
                "fmod.d.ts",
                r#"
declare module "fmod" {
  function originalFn(x: number): number;
  export { originalFn as fn };
}
"#,
            ),
            (
                "fuse.ts",
                r#"
import { originalFn } from "fmod";
"#,
            ),
        ],
        "fuse.ts",
    );
    assert!(
        diags.iter().any(|(c, m)| *c == TS2460
            && m.contains("\"fmod\"")
            && m.contains("'originalFn'")
            && m.contains("'fn'")),
        "expected TS2460 about 'fmod' / 'originalFn' as 'fn'; got {diags:#?}"
    );
}

// ───────────────────── 3. renamed identifiers (anti-hardcoding) ────────────

/// Different identifier spellings — the rule is structural, not name-based.
#[test]
fn ambient_module_renamed_export_with_different_identifier_names_still_emits_ts2460() {
    let diags = diagnostics(
        &[
            (
                "pkg.d.ts",
                r#"
declare module "pkg" {
  interface Mover { go(): void; }
  export { Mover as Car };
}
"#,
            ),
            (
                "consumer.ts",
                r#"
import { Mover } from "pkg";
"#,
            ),
        ],
        "consumer.ts",
    );
    assert!(
        diags.iter().any(|(c, m)| *c == TS2460
            && m.contains("\"pkg\"")
            && m.contains("'Mover'")
            && m.contains("'Car'")),
        "expected TS2460 about 'pkg' / 'Mover' as 'Car'; got {diags:#?}"
    );
}

// ───────────────────── 4. file-module control still works ──────────────────

/// File-module path must continue to emit TS2460 unchanged. (#6059/#6180
/// regression guard plus this PR's no-regression invariant.)
#[test]
fn file_module_renamed_export_still_emits_ts2460() {
    let diags = diagnostics(
        &[
            (
                "fm1.ts",
                r#"
interface Orig { v: number; }
export { Orig as Renamed };
"#,
            ),
            (
                "fm2.ts",
                r#"
import { Orig } from "./fm1";
const r: Orig = { v: 1 };
"#,
            ),
        ],
        "fm2.ts",
    );
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == TS2460 && m.contains("'Orig'") && m.contains("'Renamed'")),
        "file-module TS2460 must keep firing; got {diags:#?}"
    );
}

// ───────────────────── 5. importing the renamed alias is OK ────────────────

/// Importing the *renamed* alias from an ambient module must remain clean
/// (no TS2460 false positive on the success path).
#[test]
fn ambient_module_importing_the_renamed_alias_has_no_ts2460() {
    let diags = diagnostics(
        &[
            (
                "rn.d.ts",
                r#"
declare module "rn" {
  interface Orig { v: number; }
  export { Orig as Renamed };
}
"#,
            ),
            (
                "ok.ts",
                r#"
import { Renamed } from "rn";
const r: Renamed = { v: 1 };
"#,
            ),
        ],
        "ok.ts",
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == TS2460),
        "renamed-alias import must not emit TS2460; got {diags:#?}"
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == TS2305),
        "renamed-alias import must not emit TS2305; got {diags:#?}"
    );
}

// ───────────────────── 6. ambient + file modules in same project ───────────

/// When the import resolves to a file module that exports `Orig` directly, an
/// unrelated ambient module elsewhere in the project that *renames* `Orig`
/// under a different specifier must not leak into the file-module diagnostic.
#[test]
fn ambient_module_with_same_specifier_in_another_file_does_not_leak_into_file_module() {
    let diags = diagnostics(
        &[
            // file-module `./fm1` exports `Orig` directly.
            (
                "fm1.ts",
                r#"
export interface Orig { v: number; }
"#,
            ),
            // Ambient module with a DIFFERENT specifier that happens to also
            // define a renamed `Orig` — must not contaminate the file-module
            // diagnostic.
            (
                "amb.d.ts",
                r#"
declare module "amb" {
  interface Orig { v: number; }
  export { Orig as Renamed };
}
"#,
            ),
            (
                "use.ts",
                r#"
import { Orig } from "./fm1";
const r: Orig = { v: 1 };
"#,
            ),
        ],
        "use.ts",
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == TS2460),
        "file-module import must not be flagged by an unrelated ambient module's rename; got {diags:#?}"
    );
}

// ───────────────────── 7. multiple ambient modules in one file ─────────────

/// Two ambient modules in the same `.d.ts` — only the one matching the
/// import specifier should be consulted for renames.
const TWO_AMBIENT_SOURCE: &str = r#"
declare module "alpha" {
  interface Shared { a: number; }
  export { Shared as A };
}
declare module "beta" {
  interface Shared { a: number; }
  // beta does NOT rename Shared — it is directly exported.
}
"#;

#[test]
fn alpha_ambient_module_emits_ts2460_for_its_renamed_interface() {
    let diags = diagnostics(
        &[
            ("alpha.d.ts", TWO_AMBIENT_SOURCE),
            (
                "use_alpha.ts",
                r#"
import { Shared } from "alpha";
"#,
            ),
        ],
        "use_alpha.ts",
    );
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == TS2460 && m.contains("\"alpha\"") && m.contains("'A'")),
        "alpha import must emit TS2460 about its own rename; got {diags:#?}"
    );
}

#[test]
fn beta_ambient_module_does_not_get_flagged_by_alphas_rename() {
    let diags = diagnostics(
        &[
            ("beta.d.ts", TWO_AMBIENT_SOURCE),
            (
                "use_beta.ts",
                r#"
import { Shared } from "beta";
"#,
            ),
        ],
        "use_beta.ts",
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == TS2460),
        "beta import must not be flagged by alpha's rename; got {diags:#?}"
    );
}

// ───────────────────── 8. direct + renamed export — no TS2460 ──────────────

/// If the original name is ALSO exported directly (`export { Orig }`),
/// TS2460 must NOT fire even though a `Orig as Renamed` rename exists,
/// because the direct export makes the original name a valid public name.
#[test]
fn ambient_module_direct_and_renamed_export_does_not_emit_ts2460() {
    let diags = diagnostics(
        &[
            (
                "both.d.ts",
                r#"
declare module "both" {
  interface Orig { v: number; }
  export { Orig };
  export { Orig as Renamed };
}
"#,
            ),
            (
                "use.ts",
                r#"
import { Orig } from "both";
const r: Orig = { v: 1 };
"#,
            ),
        ],
        "use.ts",
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == TS2460),
        "direct + renamed exports must keep TS2460 silenced; got {diags:#?}"
    );
}
