//! Regression coverage for the cross-arena delegation hot path
//! (`delegate_cross_arena_symbol_resolution`).
//!
//! The delegation entry point resolves the requested symbol once and reuses that
//! identity across its read-only guard blocks (TYPE_ALIAS / CLASS / FUNCTION /
//! NAMESPACE / INTERFACE). These tests pin that the consolidated single lowering
//! still drives every guard branch correctly: a clean program importing each
//! symbol kind across files must type-check with no spurious cross-file
//! diagnostics (`TS2304` cannot-find-name, `TS2307` cannot-find-module,
//! `TS2305` no-exported-member, `TS2339` property-missing, `TS18046` unknown).
//!
//! Binder names are varied between cases so nothing keys off a specific
//! identifier (anti-hardcoding contract).

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_all_multi_file_with_global_index;

/// Codes that would indicate the cross-arena delegation path failed to resolve
/// an imported symbol (mis-routed arena, dropped declaration, or `UNKNOWN`).
const CROSS_FILE_RESOLUTION_FAILURE_CODES: [u32; 5] = [2304, 2307, 2305, 2339, 18046];

fn delegation_failure_codes(files: &[(&str, &str)]) -> Vec<u32> {
    check_all_multi_file_with_global_index(files, CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .filter(|code| CROSS_FILE_RESOLUTION_FAILURE_CODES.contains(code))
        .collect()
}

#[test]
fn cross_file_interface_alias_class_function_all_resolve_through_delegation() {
    // One target module declares every symbol kind the delegation guards branch
    // on; the entry module imports and uses each in a well-typed position.
    let files = [
        (
            "lib_alpha.ts",
            r#"
                export interface OptionBag {
                    enabled: boolean;
                    retries: number;
                }
                export type Mode = "fast" | "slow";
                export class Worker {
                    run(): number { return 1; }
                }
                export function build(): number { return 0; }
            "#,
        ),
        (
            "entry_alpha.ts",
            r#"
                import { OptionBag, Mode, Worker, build } from "./lib_alpha";
                const opts: OptionBag = { enabled: true, retries: 3 };
                const flag: boolean = opts.enabled;
                const mode: Mode = "fast";
                const w: Worker = new Worker();
                const n: number = w.run();
                const m: number = build();
            "#,
        ),
    ];
    assert_eq!(
        delegation_failure_codes(&files),
        Vec::<u32>::new(),
        "clean cross-file program must resolve every imported symbol kind",
    );
}

#[test]
fn cross_file_interface_alias_class_function_resolve_with_renamed_binders() {
    // Same shape, different identifiers — the consolidated single fetch is keyed
    // by SymbolId, never by name, so the renamed program must behave identically.
    let files = [
        (
            "lib_beta.ts",
            r#"
                export interface SettingsRecord {
                    verbose: boolean;
                    attempts: number;
                }
                export type Speed = "high" | "low";
                export class Runner {
                    execute(): string { return ""; }
                }
                export function assemble(): string { return ""; }
            "#,
        ),
        (
            "entry_beta.ts",
            r#"
                import { SettingsRecord, Speed, Runner, assemble } from "./lib_beta";
                const cfg: SettingsRecord = { verbose: false, attempts: 1 };
                const v: boolean = cfg.verbose;
                const s: Speed = "low";
                const r: Runner = new Runner();
                const out: string = r.execute();
                const a: string = assemble();
            "#,
        ),
    ];
    assert_eq!(
        delegation_failure_codes(&files),
        Vec::<u32>::new(),
        "renamed cross-file program must resolve identically (no name-keyed behavior)",
    );
}
