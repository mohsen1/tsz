//! Tests that structural/declaration/import nodes reaching expression-type
//! dispatch do not return `TypeId::ERROR`, preventing poisoning of downstream
//! relation checks.
//!
//! Structural rule: when `dispatch_type_computation` is called on a node that
//! is not a value-producing expression, the result must not be `TypeId::ERROR`.
//! Two categories apply:
//!
//! - **Statement/declaration nodes** (`IMPORT_DECLARATION`, `EXPORT_DECLARATION`,
//!   `BLOCK`, etc.) produce no value → return `TypeId::VOID`.
//! - **Binding nodes** (`IMPORT_SPECIFIER`, `EXPORT_SPECIFIER`,
//!   `NAMESPACE_IMPORT`, `NAMESPACE_EXPORT`) refer to named bindings that
//!   carry real types. `VOID` would be incorrect because `void` participates
//!   in assignability and triggers spurious diagnostics; `ANY` is returned as
//!   a conservative permissive placeholder until proper per-node type
//!   resolution is in place.
//!
//! An `ERROR` return propagates through assignability checks and produces
//! "uncoded diagnostics" — diagnostics without a standard TS error code.
//!
//! Adjacent-case coverage (per `CLAUDE.md` §25/§26):
//! - import declaration node family: `import { X } from '...'`
//! - export declaration node family: `export { X }`
//! - parameter declaration: `(x: T) => void`
//! - import keyword token in structural position
//! - multiple name choices to prove no hardcoded name dependency
//!
//! The multi-file tests exercise the cross-file traversal path, which is
//! the primary way these structural nodes reach expression dispatch in
//! real-world projects.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file, check_source_codes};

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file(files, entry, opts())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// ── Import declaration / import specifier family ─────────────────────────────

/// Cross-file import specifier traversal must not poison the entry-file's
/// assignability checks with ERROR types from the import specifier node.
/// Name: `Widget` / `render` (varies from the Kysely repro spelling).
#[test]
fn import_specifier_traversal_does_not_poison_assignability_widget() {
    let lib = r#"
export type Widget = { id: number; label: string };
export function render(w: Widget): string { return w.label; }
"#;
    let entry = r#"
import { Widget, render } from './lib';
const w: Widget = { id: 1, label: 'hello' };
const s: string = render(w);
"#;
    let result = codes(&[("lib.ts", lib), ("entry.ts", entry)], "entry.ts");
    assert!(
        result.is_empty(),
        "valid import-specifier cross-file usage must produce no diagnostics; got {result:?}"
    );
}

/// Import-specifier traversal with a renamed identifier to prove the rule
/// is structural, not name-dependent. Name: `Gadget` / `display`.
#[test]
fn import_specifier_traversal_does_not_poison_assignability_gadget() {
    let lib = r#"
export type Gadget = { code: string };
export function display(g: Gadget): void {}
"#;
    let entry = r#"
import { Gadget, display } from './lib';
const g: Gadget = { code: 'abc' };
display(g);
"#;
    let result = codes(&[("lib.ts", lib), ("entry.ts", entry)], "entry.ts");
    assert!(
        result.is_empty(),
        "import specifier with renamed types must produce no diagnostics; got {result:?}"
    );
}

/// An import declaration whose specifiers introduce a function type.
/// Verifies no false TS2345 from structural-node ERROR contamination.
/// Uses explicit annotations to avoid inference-limit interference.
#[test]
fn import_declaration_with_function_type_no_false_ts2345() {
    let lib = r#"
export type Transformer<A, B> = (input: A) => B;
export function applyAll<T, U>(items: T[], fn: Transformer<T, U>): U[] {
    return items.map(fn);
}
"#;
    let entry = r#"
import { Transformer, applyAll } from './lib';
const nums: number[] = [1, 2, 3];
const fn1: Transformer<number, string> = (n: number) => n.toString();
const strs: string[] = applyAll(nums, fn1);
"#;
    let result = codes(&[("lib.ts", lib), ("entry.ts", entry)], "entry.ts");
    assert!(
        result.is_empty(),
        "import of generic function type must produce no diagnostics; got {result:?}"
    );
}

// ── Parameter declaration family ─────────────────────────────────────────────

/// Callback parameters in a generic type must not cause ERROR contamination.
/// Shape: `(param: T) => R` — the PARAMETER node for `param` must yield VOID.
#[test]
fn parameter_node_in_callback_type_no_false_diagnostics_param_name_x() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const result: string[] = map([1, 2, 3], (x) => x.toString());
"#;
    let result = check_source_codes(source);
    assert!(
        result.is_empty(),
        "callback with parameter `x` must produce no diagnostics; got {result:?}"
    );
}

/// Same rule, different parameter name to prove no hardcoded `x` dependency.
#[test]
fn parameter_node_in_callback_type_no_false_diagnostics_param_name_item() {
    let source = r#"
declare function transform<T, U>(arr: T[], fn: (item: T) => U): U[];
const result: string[] = transform([1, 2, 3], (item) => item.toString());
"#;
    let result = check_source_codes(source);
    assert!(
        result.is_empty(),
        "callback with parameter `item` must produce no diagnostics; got {result:?}"
    );
}

/// Constructor parameter declarations must not reach dispatch and return ERROR.
/// Shape: class with typed constructor parameter.
#[test]
fn constructor_parameter_declaration_no_false_diagnostics() {
    let source = r#"
class Service {
    constructor(private name: string, private port: number) {}
    describe(): string { return `${this.name}:${this.port}`; }
}
const svc: Service = new Service('api', 8080);
const desc: string = svc.describe();
"#;
    let result = check_source_codes(source);
    assert!(
        result.is_empty(),
        "constructor parameters must produce no diagnostics; got {result:?}"
    );
}

/// Multi-file: parameter node in an exported function signature does not
/// contaminate the importer's type checks.
#[test]
fn cross_file_parameter_node_does_not_contaminate_importer() {
    let lib = r#"
export function clamp(value: number, min: number, max: number): number {
    return Math.min(Math.max(value, min), max);
}
"#;
    let entry = r#"
import { clamp } from './lib';
const c: number = clamp(5, 0, 10);
"#;
    let result = codes(&[("lib.ts", lib), ("entry.ts", entry)], "entry.ts");
    assert!(
        result.is_empty(),
        "cross-file function with parameters must produce no diagnostics; got {result:?}"
    );
}

// ── Export declaration family ─────────────────────────────────────────────────

/// Export specifiers re-exporting imported symbols must not poison the
/// consumer's type checks with `ERROR` from `EXPORT_SPECIFIER` nodes.
#[test]
fn export_specifier_traversal_does_not_poison_consumer() {
    let base = r#"
export type Config = { host: string; port: number };
"#;
    let re_export = r#"
export { Config } from './base';
"#;
    let entry = r#"
import { Config } from './re_export';
const cfg: Config = { host: 'localhost', port: 3000 };
"#;
    let result = codes(
        &[
            ("base.ts", base),
            ("re_export.ts", re_export),
            ("entry.ts", entry),
        ],
        "entry.ts",
    );
    assert!(
        result.is_empty(),
        "re-export specifier traversal must produce no diagnostics; got {result:?}"
    );
}

// ── Import keyword in structural position ────────────────────────────────────

/// `import("…").T` type-import exercises the `ImportKeyword` path.
/// VOID must be returned for the keyword token; no ERROR cascades.
#[test]
fn import_keyword_in_type_position_does_not_produce_error_type() {
    let source = r#"
type Loaded = import('./some-module').default;
"#;
    let result = check_source_codes(source);
    // TS2307 (cannot find module) is expected; no other codes should appear.
    let unexpected: Vec<u32> = result
        .into_iter()
        .filter(|&c| c != 2307 && c != 2792)
        .collect();
    assert!(
        unexpected.is_empty(),
        "import type expression must not produce unexpected diagnostics; got {unexpected:?}"
    );
}

// ── Absence of ERROR-cascade diagnostics across all families ─────────────────

/// Verifies that none of the specific TS error codes that are characteristic
/// of ERROR-type propagation appear spuriously for clean import-specifier code.
/// ERROR contamination typically surfaces as TS2322, TS2345, or TS2339.
#[test]
fn no_error_cascade_codes_from_structural_nodes_multi_file() {
    let lib = r#"
export interface Repository<T> {
    findById(id: number): T | undefined;
    save(entity: T): void;
}
export type User = { id: number; email: string };
export function createUserRepo(): Repository<User> {
    return {
        findById: (_id) => undefined,
        save: (_u) => {},
    };
}
"#;
    let entry = r#"
import { Repository, User, createUserRepo } from './lib';
const repo: Repository<User> = createUserRepo();
const user: User | undefined = repo.findById(42);
"#;
    let result = codes(&[("lib.ts", lib), ("entry.ts", entry)], "entry.ts");
    let cascade_codes = [2322u32, 2345, 2339];
    let cascades: Vec<u32> = result
        .into_iter()
        .filter(|c| cascade_codes.contains(c))
        .collect();
    assert!(
        cascades.is_empty(),
        "structural node traversal must not produce cascade diagnostics; got {cascades:?}"
    );
}
