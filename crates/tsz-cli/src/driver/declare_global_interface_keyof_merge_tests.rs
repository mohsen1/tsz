//! Project-mode coverage: interface declarations spread across multiple
//! `declare global { ... }` blocks — in one file or across files — must be
//! folded into the global interface BEFORE type-level operations (`keyof X`,
//! indexed access `X[K]`, assignability) observe it, not only value-position
//! member access.
//!
//! Each `declare global` block binds a SEPARATE interface symbol (the binder
//! restores the boundary scope between blocks so they cannot shadow lib
//! globals), so a bare type reference resolved only one partial symbol and left
//! `keyof X` / `X["k"]` blind to the other blocks' members — a false
//! `TS2339`/`TS2322` and a `keyof` that dropped keys. `tsc` merges all blocks.
//!
//! These run the full project driver (shared `DefinitionStore`, cross-file
//! global resolution) because the cross-file global merge only arises under the
//! project pipeline. The matrix varies the binder names (anti-hardcoding) and
//! keeps negative cases so the fold does not widen `keyof` to `string` or values
//! to `any`.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

const TS2322: u32 = 2322;
const TS2339: u32 = 2339;
const TS2536: u32 = 2536;

/// Write `files` plus a strict `noEmit` tsconfig into a fresh temp dir and run
/// the project-mode compile. Returns every emitted diagnostic.
fn compile_project(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    let names: Vec<String> = files
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let tsconfig = format!(
        r#"{{ "compilerOptions": {{ "strict": true, "target": "esnext", "module": "esnext", "moduleResolution": "bundler", "skipLibCheck": true, "noEmit": true }}, "files": [{}] }}"#,
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

/// Two `declare global { interface Registry }` blocks in one file: `keyof` and
/// indexed access must see both members; only a genuinely-absent key errors.
#[test]
fn two_global_blocks_in_one_file_merge_type_level() {
    let diags = compile_project(&[(
        "main.ts",
        r#"
declare global { interface Registry { a: number } }
declare global { interface Registry { b: string } }
type Va = Registry["a"];
type Vb = Registry["b"];
const va: Va = 1;
const vb: Vb = "x";
type K = keyof Registry;
const k1: K = "a";
const k2: K = "b";
const kbad: K = "c";
export {};
"#,
    )]);

    assert_eq!(
        count_code(&diags, TS2339),
        0,
        "indexed access of a merged-global member must resolve; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, TS2536),
        0,
        "merged-global indexed access must not raise TS2536; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, TS2322),
        1,
        "only the absent key `\"c\"` should be rejected; got {diags:#?}"
    );
}

/// Cross-file `declare global` blocks: the registry interface is declared empty
/// in one module, augmented in another, and `keyof` / indexed access consumed in
/// a third. The merge must be order-independent.
#[test]
fn cross_file_global_blocks_merge_type_level() {
    let files = [
        (
            "registry.ts",
            r#"
declare global { interface Registry {} }
export type Keys = keyof Registry;
export {};
"#,
        ),
        (
            "augment.ts",
            r#"
declare global { interface Registry { foo: number } }
export {};
"#,
        ),
        (
            "use.ts",
            r#"
import type { Keys } from "./registry";
const ok: Keys[] = ["foo"];
const bad: Keys[] = ["nope"];
type Vfoo = Registry["foo"];
const vf: Vfoo = 1;
type Vbad = Registry["nope"];
export {};
"#,
        ),
    ];

    let diags = compile_project(&files);

    // `"foo"` is a real merged key: the `bad` array element (`"nope"`) is the
    // single TS2322, and `Registry["nope"]` is the single TS2339.
    assert_eq!(
        count_code(&diags, TS2322),
        1,
        "only the absent key `\"nope\"` should be rejected; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, TS2339),
        1,
        "only `Registry[\"nope\"]` should miss; `Registry[\"foo\"]` must resolve; got {diags:#?}"
    );
}

/// Anti-hardcoding: the rule is structural, not name-driven. Rename every binder
/// and the merge still holds across two blocks.
#[test]
fn merged_global_keyof_rule_is_binder_name_independent() {
    let diags = compile_project(&[(
        "main.ts",
        r#"
declare global { interface Slots { widget: number } }
declare global { interface Slots { gadget: string } }
type Tags = keyof Slots;
const t1: Tags = "widget";
const t2: Tags = "gadget";
const tbad: Tags = "absent";
export {};
"#,
    )]);

    assert_eq!(
        count_code(&diags, TS2322),
        1,
        "renamed-binder global registry should merge both keys, rejecting only \
         the absent one; got {diags:#?}"
    );
}
