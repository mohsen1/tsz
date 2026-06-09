//! DTS emit: a generic overloaded call's inferred result type must survive a
//! *later* overloaded call that contextually types an inline callback argument.
//!
//! Structural rule: overload resolution is speculative and must be transparent
//! to the caller's cached expression types. When tsc resolves an overloaded
//! generic call whose argument is an inline (contextually typed) callback, it
//! re-collects the argument types under the instantiated signature; that retry
//! must not discard the node-type cache entries computed for *unrelated* earlier
//! expressions. Declaration emit reads those cached initializer-expression types
//! to print inferred `const` types, so dropping them collapses the earlier
//! declaration to `any`.
//!
//! Before the fix, `resolve_overloads` moved the caller's node-type snapshot out
//! of `original_node_types` up front and the generic contextual-retry branch
//! reset the cache to an empty map, so every restore site rebuilt on an empty
//! map. The earlier `Array.prototype.filter` call (whose result type the emitter
//! needs) therefore lost its cached type and emitted `any` as soon as a second
//! `filter` call with an inline callback followed it.
//!
//! These run the full checker pipeline (the unit-level declaration-emit harness
//! uses an empty type cache and cannot resolve `Array.prototype.filter`). Each
//! case varies the binder names, element types and predicate spelling so a
//! regression keyed on a particular identifier or shape rather than the
//! structural caching rule would fail.

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
        path.push(format!("tsz_overload_node_cache_dts_{name}_{nanos}"));
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

fn emit_dts(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let src_path = temp.path.join("repro.ts");
    std::fs::write(&src_path, source).expect("write repro file");

    let output = Command::new(tsz_bin)
        .args([
            "repro.ts",
            "--declaration",
            "--emitDeclarationOnly",
            "--strict",
            "--target",
            "es2015",
            "--lib",
            "es6",
            "--pretty",
            "false",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz declaration emit");

    let dts = std::fs::read_to_string(temp.path.join("repro.d.ts")).unwrap_or_else(|_| {
        panic!(
            "expected repro.d.ts to be emitted.\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    Some(dts)
}

/// Emit declarations for `source` and assert each `(expected_substring, message)`
/// pair is present. Keeps the per-case message so a regression names the exact
/// expectation that failed while removing the repeated emit/skip/assert scaffold.
fn assert_dts_contains(name: &str, source: &str, expectations: &[(&str, &str)]) {
    let Some(dts) = emit_dts(name, source) else {
        println!("skipping: tsz binary not found");
        return;
    };
    for (expected, message) in expectations {
        assert!(dts.contains(expected), "{message}:\n{dts}");
    }
}

// =============================================================================
// The earlier overloaded call's inferred result survives a later overloaded
// call with an inline contextually-typed callback (the fixed behaviour).
// =============================================================================

/// Primary repro: a function-declaration predicate `filter` is followed by an
/// inline predicate `filter`. The earlier result must stay `string[]`.
#[test]
fn function_decl_filter_survives_following_inline_filter() {
    assert_dts_contains(
        "func_decl_then_inline",
        "function isText(x: unknown) { return typeof x === \"string\"; }\n\
         const xs: (string | number)[] = [\"a\", 1];\n\
         export const first = xs.filter(isText);\n\
         export const second = xs.filter((y): y is number => typeof y === \"number\");\n",
        &[
            (
                "export declare const first: string[];",
                "earlier filter result must survive the later inline-callback filter",
            ),
            (
                "export declare const second: number[];",
                "later inline-predicate filter must still resolve correctly",
            ),
        ],
    );
}

/// Adjacent case (renamed binders, different element/predicate types): an arrow
/// predicate `filter` followed by an inline predicate `filter`. Proves the rule
/// is structural, not tied to the `isText`/`string[]` spelling.
#[test]
fn arrow_filter_survives_following_inline_filter() {
    assert_dts_contains(
        "arrow_then_inline",
        "const isCount = (v: unknown) => typeof v === \"number\";\n\
         const items: (string | number)[] = [\"a\", 1];\n\
         export const picked = items.filter(isCount);\n\
         export const rest = items.filter((w): w is string => typeof w === \"string\");\n",
        &[
            (
                "export declare const picked: number[];",
                "earlier arrow-predicate filter result must survive",
            ),
            (
                "export declare const rest: string[];",
                "later inline-predicate filter must still resolve correctly",
            ),
        ],
    );
}

/// An explicit-predicate function declaration as the earlier call also survives,
/// showing the fix is independent of whether the predicate was inferred.
#[test]
fn explicit_predicate_filter_survives_following_inline_filter() {
    assert_dts_contains(
        "explicit_then_inline",
        "function isWord(x: unknown): x is string { return typeof x === \"string\"; }\n\
         const data: (string | number)[] = [\"a\", 1];\n\
         export const words = data.filter(isWord);\n\
         export const nums = data.filter((n): n is number => typeof n === \"number\");\n",
        &[
            (
                "export declare const words: string[];",
                "earlier explicit-predicate filter result must survive",
            ),
            (
                "export declare const nums: number[];",
                "later inline-predicate filter must still resolve correctly",
            ),
        ],
    );
}

/// Breadth: the earliest result must survive *several* following overloaded
/// calls with inline callbacks (filter and map both trigger argument
/// re-collection for their inline callbacks).
#[test]
fn earliest_result_survives_multiple_following_inline_calls() {
    assert_dts_contains(
        "multi_following",
        "function isText(x: unknown) { return typeof x === \"string\"; }\n\
         const xs: (string | number)[] = [\"a\", 1];\n\
         export const head = xs.filter(isText);\n\
         export const tail = xs.filter((y): y is number => typeof y === \"number\");\n\
         export const mapped = xs.map((z) => `${z}`);\n",
        &[
            (
                "export declare const head: string[];",
                "earliest filter result must survive multiple later inline-callback calls",
            ),
            (
                "export declare const tail: number[];",
                "the middle inline-predicate filter must still resolve correctly",
            ),
        ],
    );
}

// =============================================================================
// Unchanged behaviour: a single overloaded call still resolves correctly.
// =============================================================================

/// A lone predicate `filter` (no following overloaded call) still emits the
/// narrowed element array, confirming the fix did not regress the base case.
#[test]
fn single_filter_still_resolves() {
    assert_dts_contains(
        "single",
        "function isText(x: unknown) { return typeof x === \"string\"; }\n\
         const xs: (string | number)[] = [\"a\", 1];\n\
         export const only = xs.filter(isText);\n",
        &[(
            "export declare const only: string[];",
            "a lone predicate filter must still emit the narrowed array",
        )],
    );
}
