//! CONSTRUCT-LEVEL (smoke) coverage for cross-file `Parameters<typeof F>[N]` /
//! `ReturnType<typeof F>` over a plain imported value (#80). Each test asserts the
//! construct compiles clean cross-file with deliberately fresh binder names
//! (`makeThing` / `assemble` / `build` / `Widget`, never the jotai
//! `useStore`/`useAtomValue` acceptance witnesses) — the anti-hardcoding
//! item-3 "vary binder names" documentation for the #80 fix (the structural
//! `ALIAS`-flag follow + consuming-session typeof registration, which carry no
//! name literals).
//!
//! These are NOT fails-without regression tests. #80's false `TS2345` requires
//! jotai's whole-program cross-arena identity split (its 37-file `Set` /
//! `CSSKeywordValue` "two different types with this name exist, but they are
//! unrelated") — 8 minimal/medium candidates (including a 4-file barrel-cycle
//! `BuildingBlocks`-tuple mirror) were verified clean on BOTH the pre-#80 and the
//! fixed binary, so the bug does not minimize. The load-bearing fails-without /
//! passes-with witness is the jotai canary row: pre-fix 17 diagnostics with
//! `useAtomValue.ts(88,34)` + `(97,28)` `TS2345` present → fixed 15 with both
//! cleared. See #80 and the PR Verification section.
//!
//! They run the full project driver (shared `DefinitionStore`, every file
//! checked) so the per-file `type_env` reset the fix targets is exercised.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

const TS2345: u32 = 2345;
const TS2322: u32 = 2322;

/// Write `files` plus a strict `noEmit` tsconfig into a fresh temp dir and run
/// the project-mode compile. Returns every emitted diagnostic.
fn compile_project(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    let names: Vec<String> = files
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let tsconfig = format!(
        r#"{{ "compilerOptions": {{ "strict": true, "target": "es2015", "noEmit": true }}, "files": [{}] }}"#,
        names.join(", ")
    );
    fs::write(dir.path().join("tsconfig.json"), tsconfig).expect("write tsconfig");
    for (name, source) in files {
        fs::write(dir.path().join(name), source).expect("write source");
    }

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("project args");
    compile(&args, dir.path())
        .expect("compile succeeds")
        .diagnostics
}

fn count_code(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

const MAKE_THING: (&str, &str) = (
    "a.ts",
    "export type Widget = { readonly get: () => number };\n\
     export function makeThing(opts?: { widget?: Widget }): Widget {\n\
       return { get: () => (opts && opts.widget ? opts.widget.get() : 0) };\n\
     }\n",
);

/// `type Opts = Parameters<typeof makeThing>[0]` in a consuming file must resolve
/// to `makeThing`'s own first parameter type — passing an `Opts` value back to
/// `makeThing` is accepted, no false TS2345.
#[test]
fn parameters_typeof_plain_fn_param_cross_file_no_ts2345() {
    let diags = compile_project(&[
        MAKE_THING,
        (
            "b.ts",
            "import { makeThing } from './a';\n\
             type Opts = Parameters<typeof makeThing>[0];\n\
             export function readThing(opts?: Opts): number {\n\
               return makeThing(opts).get();\n\
             }\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2345),
        0,
        "cross-file Parameters<typeof makeThing>[0] must not emit a false TS2345, got: {diags:?}"
    );
}

/// The renamed-import form (`import { makeThing as build }`) must key on the
/// canonical value, not the local alias — same clean result.
#[test]
fn parameters_typeof_renamed_import_cross_file_no_ts2345() {
    let diags = compile_project(&[
        MAKE_THING,
        (
            "b.ts",
            "import { makeThing as build } from './a';\n\
             type Opts = Parameters<typeof build>[0];\n\
             export function readThing(opts?: Opts): number {\n\
               return build(opts).get();\n\
             }\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2345),
        0,
        "renamed-import Parameters<typeof build>[0] must not emit a false TS2345, got: {diags:?}"
    );
}

/// `ReturnType<typeof assemble>` over a cross-file factory returning a structural
/// shape must resolve so the shape relates to itself (the useAtomValue
/// `Store = ReturnType<typeof createStore>` family, minimized + renamed).
#[test]
fn returntype_typeof_plain_fn_cross_file_no_ts2345() {
    let diags = compile_project(&[
        (
            "a.ts",
            "export type Block = { readonly run: () => void };\n\
             export function assemble(): readonly Block[] {\n\
               return [];\n\
             }\n",
        ),
        (
            "b.ts",
            "import { assemble } from './a';\n\
             type Blocks = ReturnType<typeof assemble>;\n\
             export function consume(bs: Readonly<Blocks>): number {\n\
               return bs.length;\n\
             }\n\
             export const out: number = consume(assemble());\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2345),
        0,
        "cross-file ReturnType<typeof assemble> must not emit a false TS2345, got: {diags:?}"
    );
}

/// Negative control: the registered value type is genuinely checked, not blindly
/// accepted — passing a structurally wrong value through a `Parameters<typeof
/// makeThing>[0]` parameter still errors (a `widget: number` where `Widget` is
/// required), so the fix does not mask real mismatches.
#[test]
fn parameters_typeof_wrong_arg_still_errors_cross_file() {
    let diags = compile_project(&[
        MAKE_THING,
        (
            "b.ts",
            "import { makeThing } from './a';\n\
             type Opts = Parameters<typeof makeThing>[0];\n\
             function readThing(opts?: Opts): number {\n\
               return makeThing(opts).get();\n\
             }\n\
             export const r = readThing({ widget: 123 });\n",
        ),
    ]);
    assert!(
        count_code(&diags, TS2322) > 0 || count_code(&diags, TS2345) > 0,
        "a structurally wrong `widget` value must still be rejected, got: {diags:?}"
    );
}
