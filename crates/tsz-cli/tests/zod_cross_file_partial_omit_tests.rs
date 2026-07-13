//! Cross-file `Partial<Omit<ParseParams, "data">>` over a circular import must
//! preserve the imported `path` property (zod `ParseParams` witness).
//!
//! These tests drive the real `tsz` binary because the property is reached
//! through a cross-file circular import (`zod-error.ts` <-> `parse-util.ts`)
//! resolved via `Partial<Omit<...>>`. Only the driver runs whole-program
//! checking with deferred lib-interface publication
//! (`mark_non_program_interface_defs_deferred`, the lazy-lib path) and the
//! global symbol-file index; the in-process checker harness checks the entry
//! file only and materializes lib-interface members eagerly, so the circular
//! `ZodErrorMap` arm resolves differently and a spurious `TS2345`/`TS2349`
//! appears in-harness even though tsc 7.0.2 and the real binary are both
//! clean.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_zod_partial_omit_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

const ZOD_ERROR: &str = r#"
import { ZodParsedType } from "./parse-util";

export type ZodIssue = { path: (string | number)[]; parsed: ZodParsedType };
export type ZodErrorMap = (...args: any[]) => { message: string };
"#;

const PARSE_UTIL: &str = r#"
import { ZodErrorMap } from "./zod-error";

export const ZodParsedType = {
    string: "string",
    object: "object",
} as const;
export type ZodParsedType = keyof typeof ZodParsedType;

export type ParseParams = {
    path: (string | number)[];
    errorMap: ZodErrorMap;
    async: boolean;
};

export type ParseParamsNoData = Omit<ParseParams, "data">;
"#;

/// Run `tsz` on a 3-file project (`zod-error.ts`, `parse-util.ts`, plus a
/// caller `types.ts` supplied per test) and return combined stdout+stderr.
fn run_tsz_project(name: &str, types_src: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    std::fs::write(temp.path.join("zod-error.ts"), ZOD_ERROR).expect("write zod-error");
    std::fs::write(temp.path.join("parse-util.ts"), PARSE_UTIL).expect("write parse-util");
    std::fs::write(temp.path.join("types.ts"), types_src).expect("write types");
    let output = Command::new(tsz_bin)
        .args([
            "zod-error.ts",
            "parse-util.ts",
            "types.ts",
            "--strict",
            "--noEmit",
            "--pretty",
            "false",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(text)
}

/// Positive: `Partial<Omit<...>>` preserves the imported `path`, so both reads
/// type-check. tsc 7.0.2 is clean; the real `tsz` binary must be too.
#[test]
fn cross_file_partial_omit_preserves_imported_path_property() {
    let types_src = r#"
import { ParseParamsNoData } from "./parse-util";

type ParsePathComponent = string | number;
declare function pathFromArray(arr: ParsePathComponent[]): unknown;

function createRootContext(params: Partial<ParseParamsNoData>) {
    pathFromArray(params.path || []);
    pathFromArray(params.path ?? []);
}
"#;
    let Some(out) = run_tsz_project("preserve_path", types_src) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        !out.contains("error TS"),
        "lib-backed cross-file `Partial<Omit<...>>` must preserve `path` (tsc 7.0.2 is clean); got:\n{out}"
    );
}

/// Negative control: passing the wrong member (`errorMap`, a function or
/// `undefined`) where a `(string | number)[]` is required must still fail with
/// TS2345, so the positive case is not passing by erasing the property to
/// `any`.
#[test]
fn cross_file_partial_omit_wrong_member_reports_ts2345() {
    let types_src = r#"
import { ParseParamsNoData } from "./parse-util";

declare function pathFromArray(arr: (string | number)[]): unknown;

function createRootContext(params: Partial<ParseParamsNoData>) {
    pathFromArray(params.errorMap);
}
"#;
    let Some(out) = run_tsz_project("wrong_member", types_src) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        out.contains("TS2345"),
        "a `ZodErrorMap | undefined` is never a valid `(string | number)[]`; got:\n{out}"
    );
}
