//! Root-file-order independence for lib-global heritage across modules.
//!
//! Structural rule: when `interface X extends <lib global>` (e.g. `Request`,
//! `Error`, `Response`) is declared in one module and consumed through an
//! import in another, tsc resolves the heritage base identically regardless
//! of the order the root files are listed; tsz does the same through the
//! cross-file lookup binder's `program_globals` table (the hoisted
//! program-wide globals carried separately from per-file `file_locals`).
//!
//! Before that table existed, a module first reached as an IMPORT of an
//! earlier-checked root file (instead of as an earlier root file itself)
//! lost its lib heritage: the reconstructed cross-file binder had
//! `lib_symbols_merged == true` but only per-file `file_locals`, so the
//! heritage base name failed to resolve and members inherited from the lib
//! global produced false TS2339s (msw/ofetch/valibot/comlink/kysely corpus
//! family).

use crate::args::CliArgs;
use clap::Parser;

/// Compile `files` (written into one temp dir) with the given root-file
/// order and return the diagnostic codes.
fn compile_in_order(files: &[(&str, &str)], root_order: &[&str]) -> Vec<u32> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).expect("write repro file");
    }

    let mut argv: Vec<&str> = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022,dom,dom.iterable",
    ];
    argv.extend_from_slice(root_order);

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    let result = crate::driver::compile(&args, dir.path()).expect("compile should succeed");
    result.diagnostics.iter().map(|diag| diag.code).collect()
}

/// Assert both root-file orders produce no diagnostics.
fn assert_clean_both_orders(files: &[(&str, &str)]) {
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let forward = compile_in_order(files, &names);
    assert!(
        forward.is_empty(),
        "expected clean check in forward root order {names:?}, got codes: {forward:?}"
    );
    let reversed: Vec<&str> = names.iter().rev().copied().collect();
    let backward = compile_in_order(files, &reversed);
    assert!(
        backward.is_empty(),
        "expected clean check in reversed root order {reversed:?}, got codes: {backward:?}"
    );
}

#[test]
fn imported_interface_extending_request_keeps_lib_heritage_in_both_root_orders() {
    // Consumer listed FIRST is the regression: the declaring module is first
    // reached as an import, through the cross-file lookup binder.
    assert_clean_both_orders(&[
        (
            "reader.ts",
            r#"
import type { TightFetchInput } from './shapes';
export function pullHeaders(input: TightFetchInput): Headers {
    input.describe();
    return input.headers;
}
"#,
        ),
        (
            "shapes.ts",
            r#"
export interface TightFetchInput extends Request {
    describe(): void;
}
"#,
        ),
    ]);
}

#[test]
fn three_file_import_chain_keeps_lib_heritage_in_both_root_orders() {
    assert_clean_both_orders(&[
        (
            "entry.ts",
            r#"
import type { Wrapped } from './middle';
export function follow(w: Wrapped): string {
    w.inner.label();
    return w.inner.url;
}
"#,
        ),
        (
            "middle.ts",
            r#"
export interface Wrapped {
    inner: import('./bottom').TaggedReq;
}
"#,
        ),
        (
            "bottom.ts",
            r#"
export interface TaggedReq extends Request {
    label(): string;
}
"#,
        ),
    ]);
}

#[test]
fn imported_interface_extending_error_keeps_lib_heritage_in_both_root_orders() {
    assert_clean_both_orders(&[
        (
            "render.ts",
            r#"
import type { DomainFault } from './faults';
export function explain(fault: DomainFault): string {
    return fault.hint + fault.message;
}
"#,
        ),
        (
            "faults.ts",
            r#"
export interface DomainFault extends Error {
    hint: string;
}
"#,
        ),
    ]);
}

#[test]
fn imported_interface_extending_response_keeps_lib_heritage_in_both_root_orders() {
    assert_clean_both_orders(&[
        (
            "inspect.ts",
            r#"
import type { ParsedRes } from './payloads';
export function code(res: ParsedRes): number {
    void res.payload;
    return res.status;
}
"#,
        ),
        (
            "payloads.ts",
            r#"
export interface ParsedRes extends Response {
    payload: unknown;
}
"#,
        ),
    ]);
}

#[test]
fn imported_omit_of_lib_global_resolves_in_both_root_orders() {
    assert_clean_both_orders(&[
        (
            "apply.ts",
            r#"
import type { LooseInit } from './aliases';
export function take(init: LooseInit): Headers {
    return init.headers;
}
"#,
        ),
        (
            "aliases.ts",
            r#"
export type LooseInit = Omit<Request, 'method'> & { method?: string };
"#,
        ),
    ]);
}

#[test]
fn imported_interface_without_heritage_stays_clean_in_both_root_orders() {
    assert_clean_both_orders(&[
        (
            "consume.ts",
            r#"
import type { Bare } from './bare';
export function ident(b: Bare): number {
    return b.id;
}
"#,
        ),
        (
            "bare.ts",
            r#"
export interface Bare {
    id: number;
}
"#,
        ),
    ]);
}

#[test]
fn missing_member_on_lib_extending_interface_errors_in_both_root_orders() {
    let files: &[(&str, &str)] = &[
        (
            "misuse.ts",
            r#"
import type { TightFetchInput } from './shapes';
export function broken(input: TightFetchInput): void {
    input.definitelyNotAMember();
}
"#,
        ),
        (
            "shapes.ts",
            r#"
export interface TightFetchInput extends Request {
    describe(): void;
}
"#,
        ),
    ];
    let forward = compile_in_order(files, &["misuse.ts", "shapes.ts"]);
    assert_eq!(
        forward,
        vec![2339],
        "negative control must report exactly TS2339 in forward order"
    );
    let backward = compile_in_order(files, &["shapes.ts", "misuse.ts"]);
    assert_eq!(
        backward,
        vec![2339],
        "negative control must report exactly TS2339 in reversed order"
    );
}
