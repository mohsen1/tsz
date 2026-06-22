//! Regression tests for re-exported symbol-keyed computed members (#14127,
//! #14130).
//!
//! Structural rule: when a `const s = Symbol()` (an inferred `unique symbol`
//! binding) is re-exported through an `import { s } from "./a"; export { s }`
//! chain and consumed in another file as a computed member key `[s]`, the
//! member-key resolution must follow the full cross-file import/re-export chain
//! to the declaring `const`. tsz previously resolved only a single alias hop
//! against the current file's binder, so the key was derived from an
//! intermediate alias copy (or failed), the symbol-named member was dropped,
//! and indexing the type by `typeof s` produced a false `TS2536`.
//!
//! The fix is name-agnostic (binder names are varied) and survives multiple
//! re-export hops.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_multi_file_with_global_index;
use tsz_common::ModuleKind;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2020,
        module: ModuleKind::ESNext,
        no_lib: true,
        ..Default::default()
    }
}

/// `no_lib` checking cannot resolve global types (`Function`, `Object`, ...),
/// so filter the unavoidable `TS2318` noise and assert on the real diagnostics.
fn sig(diags: &[Diagnostic]) -> Vec<String> {
    let mut v: Vec<String> = diags
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| format!("TS{}@{}", d.code, d.start))
        .collect();
    v.sort();
    v
}

/// Direct import: the baseline that already worked. Pins the expected
/// (clean) result so the re-export cases are compared against real parity.
#[test]
fn direct_import_symbol_keyed_member_is_clean() {
    let files = [
        (
            "symbols.ts",
            "interface SymbolConstructor { (): symbol; for(key: string): symbol; }\n\
declare var Symbol: SymbolConstructor;\n\
export const matcher = Symbol();\n",
        ),
        (
            "pattern.ts",
            "import { matcher } from './symbols';\n\
interface Matcher { [matcher](): number; }\n\
export type R = Matcher[typeof matcher];\n",
        ),
    ];
    let diags = check_multi_file_with_global_index(&files, "pattern.ts", opts());
    assert!(sig(&diags).is_empty(), "direct: {:?}", sig(&diags));
}

/// The witnessed ts-pattern shape: `import { matcher } from "./symbols";
/// export { matcher };` re-export of an inferred `Symbol()` const, consumed as
/// a symbol-keyed interface member and indexed by `typeof matcher`.
#[test]
fn reexported_symbol_keyed_member_indexed_access_no_ts2536() {
    let files = [
        (
            "symbols.ts",
            "interface SymbolConstructor { (): symbol; for(key: string): symbol; }\n\
declare var Symbol: SymbolConstructor;\n\
export const matcher = Symbol();\n",
        ),
        (
            "patterns.ts",
            "import { matcher } from './symbols';\nexport { matcher };\n",
        ),
        (
            "pattern.ts",
            "import { matcher } from './patterns';\n\
interface Matcher { [matcher](): number; }\n\
export type R = Matcher[typeof matcher];\n",
        ),
    ];
    let diags = check_multi_file_with_global_index(&files, "pattern.ts", opts());
    assert!(sig(&diags).is_empty(), "re-export: {:?}", sig(&diags));
}

/// Name-agnostic: the same re-export shape with different binder/identifier
/// names (`tag`/`Tagged`) — proves the fix is not keyed on any spelling.
#[test]
fn reexported_symbol_keyed_member_is_name_agnostic() {
    let files = [
        (
            "sym.ts",
            "interface SymbolConstructor { (): symbol; for(key: string): symbol; }\n\
declare var Symbol: SymbolConstructor;\n\
export const tag = Symbol();\n",
        ),
        (
            "barrel.ts",
            "import { tag } from './sym';\nexport { tag };\n",
        ),
        (
            "use.ts",
            "import { tag } from './barrel';\n\
interface Tagged { [tag](): string; }\n\
export type R = Tagged[typeof tag];\n",
        ),
    ];
    let diags = check_multi_file_with_global_index(&files, "use.ts", opts());
    assert!(sig(&diags).is_empty(), "name-agnostic: {:?}", sig(&diags));
}

/// The `unique symbol`-annotated re-export form (no inferred-factory upgrade)
/// must remain clean through the same re-export chain — a guard that the fix
/// does not regress the annotation path.
#[test]
fn reexported_annotated_unique_symbol_member_is_clean() {
    let files = [
        (
            "symbols.ts",
            "export declare const matcher: unique symbol;\n",
        ),
        (
            "patterns.ts",
            "import { matcher } from './symbols';\nexport { matcher };\n",
        ),
        (
            "pattern.ts",
            "import { matcher } from './patterns';\n\
interface Matcher { [matcher](): number; }\n\
export type R = Matcher[typeof matcher];\n",
        ),
    ];
    let diags = check_multi_file_with_global_index(&files, "pattern.ts", opts());
    assert!(
        sig(&diags).is_empty(),
        "annotated re-export: {:?}",
        sig(&diags)
    );
}
