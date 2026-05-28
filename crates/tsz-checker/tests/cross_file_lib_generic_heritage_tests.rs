//! Cross-file resolution of inferred library-generic interfaces must preserve
//! inherited (`extends`) members.
//!
//! Structural rule: when a class declared in one file exposes a member whose
//! type is *inferred* to a library generic with a heritage chain (e.g.
//! `ArrayIterator<T> extends IteratorObject<...> extends Iterator<...>`), and
//! that class is consumed from another file, the flattened member set must
//! still include the inherited members (`next`, `return`, ...). Otherwise the
//! consuming file reports spurious `TS2345` / `TS2488` because the iterator
//! protocol appears incomplete.
//!
//! This regresses the family behind issue #10443
//! (`jsDeclarationsTypedefAndLatebound`): an unrelated JSDoc `@typedef` next to
//! a class with a late-bound `[Symbol.iterator]` member must not change whether
//! the class is recognized as iterable across files. The bug is not specific to
//! the late-bound spelling or the typedef — it is general cross-file lib-generic
//! heritage resolution — so the matrix varies the member kind, the type-param
//! spelling, the typedef name, JS vs TS, and includes a negative case.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::{
    check_multi_file_libs_via_contexts, check_multi_file_with_libs, load_default_lib_files,
};
use tsz_common::diagnostics::Diagnostic;

fn js_options() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    }
}

fn ts_options() -> CheckerOptions {
    CheckerOptions {
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    }
}

fn check(files: &[(&str, &str)], entry: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    // Use the CLI-faithful helper: lib symbols come from lib contexts, not from
    // each file's merged `file_locals`. This is the setup under which the
    // cross-file heritage drop reproduces; the merged-`file_locals` helper
    // masks it.
    check_multi_file_libs_via_contexts(files, entry, options, &libs)
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

/// Codes emitted when a value is wrongly judged non-iterable / its iterator
/// protocol incomplete.
const ITERABILITY_FALSE_POSITIVES: &[u32] = &[2345, 2488, 2504, 2549, 2769];

fn assert_no_iterability_false_positives(diags: &[Diagnostic], context: &str) {
    let offending: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| ITERABILITY_FALSE_POSITIVES.contains(&d.code))
        .collect();
    assert!(
        offending.is_empty(),
        "{context}: expected no iterability false positives, got {:#?}",
        offending
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// The reported repro family: JS class with an unrelated `@typedef` and a
/// late-bound `[Symbol.iterator]` member returning an inferred lib iterator,
/// consumed cross-file. Must not be judged non-iterable.
#[test]
fn jsdoc_typedef_latebound_iterator_iterable_across_files() {
    let diags = check(
        &[
            (
                "lazyset.js",
                r#"
/** @typedef {Object} SomeObject */
class LazySet {
    /** @param {Iterable<any> | LazySet} iterable */
    addAll(iterable) {}
    [Symbol.iterator]() { return [][Symbol.iterator](); }
}
module.exports = LazySet;
"#,
            ),
            (
                "index.js",
                r#"
const LazySet = require("./lazyset");
/** @param {Iterable<any>} x */
function takeIterable(x) {}
const s = new LazySet();
takeIterable(s);
s.addAll(s);
for (const y of s) {}
"#,
            ),
        ],
        "index.js",
        js_options(),
    );
    assert_no_iterability_false_positives(&diags, "jsdoc typedef + latebound iterator");
}

/// Same shape with the `@typedef` removed: proves the typedef is not the
/// trigger and the fix is about cross-file lib-generic heritage, not the
/// typedef interaction.
#[test]
fn latebound_iterator_iterable_across_files_without_typedef() {
    let diags = check(
        &[
            (
                "lazyset.js",
                r#"
class LazySet {
    [Symbol.iterator]() { return [][Symbol.iterator](); }
}
module.exports = LazySet;
"#,
            ),
            (
                "index.js",
                r#"
const LazySet = require("./lazyset");
/** @param {Iterable<any>} x */
function takeIterable(x) {}
const s = new LazySet();
takeIterable(s);
for (const y of s) {}
"#,
            ),
        ],
        "index.js",
        js_options(),
    );
    assert_no_iterability_false_positives(&diags, "latebound iterator without typedef");
}

/// A regular (not late-bound) method whose return type is the same inferred lib
/// generic. Proves the rule is about heritage preservation, not the
/// `[Symbol.iterator]` spelling.
#[test]
fn regular_method_inferred_lib_iterator_across_files() {
    let diags = check(
        &[
            (
                "make.js",
                r#"
class Maker {
    iter() { return [][Symbol.iterator](); }
}
module.exports = Maker;
"#,
            ),
            (
                "use.js",
                r#"
const Maker = require("./make");
/** @param {Iterator<any>} x */
function takeIterator(x) {}
const m = new Maker();
takeIterator(m.iter());
"#,
            ),
        ],
        "use.js",
        js_options(),
    );
    assert_no_iterability_false_positives(&diags, "regular method inferred lib iterator");
}

/// Renamed `@typedef` (and a different iteration variable shape): proves the
/// fix does not key on any particular name spelling.
#[test]
fn renamed_typedef_latebound_iterator_iterable_across_files() {
    let diags = check(
        &[
            (
                "coll.js",
                r#"
/** @typedef {Object} CompletelyDifferentName */
class Coll {
    [Symbol.iterator]() { return [][Symbol.iterator](); }
}
module.exports = Coll;
"#,
            ),
            (
                "main.js",
                r#"
const Coll = require("./coll");
/** @param {Iterable<any>} x */
function takeIterable(x) {}
const c = new Coll();
takeIterable(c);
for (const y of c) {}
"#,
            ),
        ],
        "main.js",
        js_options(),
    );
    assert_no_iterability_false_positives(&diags, "renamed typedef latebound iterator");
}

/// TypeScript (not JS) class whose `[Symbol.iterator]` and a regular method
/// both return an inferred lib iterator, consumed cross-file. Proves the rule
/// holds through `.ts` sources, not only JSDoc/`.js`.
#[test]
fn typescript_cross_file_inferred_lib_iterator() {
    let diags = check(
        &[
            (
                "coll.ts",
                r#"
export class Coll {
    iter() { return [1, 2, 3].values(); }
    [Symbol.iterator]() { return [1, 2, 3].values(); }
}
"#,
            ),
            (
                "consumer.ts",
                r#"
import { Coll } from "./coll";
const c = new Coll();
function takeIterator(x: Iterator<number>) {}
function takeIterable(x: Iterable<number>) {}
takeIterator(c.iter());
takeIterable(c);
for (const v of c) {}
"#,
            ),
        ],
        "consumer.ts",
        ts_options(),
    );
    assert_no_iterability_false_positives(&diags, "ts cross-file inferred lib iterator");
}

/// Negative case: a cross-file class with no `[Symbol.iterator]` member must
/// still be reported as non-iterable. The heritage-resolution fix must not
/// blanket-suppress the genuine `TS2488`.
#[test]
fn cross_file_non_iterable_class_still_reports_ts2488() {
    let diags = check(
        &[
            (
                "plain.js",
                r#"
class Plain {
    value() { return 1; }
}
module.exports = Plain;
"#,
            ),
            (
                "useplain.js",
                r#"
const Plain = require("./plain");
const p = new Plain();
for (const y of p) {}
"#,
            ),
        ],
        "useplain.js",
        js_options(),
    );
    assert!(
        codes(&diags).contains(&2488),
        "expected TS2488 for a class with no [Symbol.iterator], got {:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Reproduces the exact production state: a binder with merged lib symbols
/// (`lib_symbols_merged == true`) whose `file_locals` nonetheless does not
/// contain a particular library interface, because that interface is declared
/// in an *external-module* lib file (only its `declare global` members are
/// merged into the global scope). This is how the real TypeScript libs declare
/// some iterator/helper interfaces, and it is the state under which the bug
/// manifested in the CLI but which the plain `load_default_lib_files()` assets
/// (all-global) do not exhibit.
///
/// `Boxed<T> extends BoxBase<T>` is module-scoped, so it is absent from the
/// consumer file's merged `file_locals`. A class in one file returns it
/// (inferred via the `Array.intoBoxed` global augmentation); the consumer in
/// another file accesses the inherited `BoxBase.unwrap`. Without robust symbol
/// resolution in lib heritage merging, the inherited member is dropped and the
/// access reports a spurious `TS2339`.
fn custom_external_module_lib() -> Arc<LibFile> {
    Arc::new(LibFile::from_source(
        "es2099.boxed.d.ts".to_string(),
        r#"export {};
interface BoxBase<T> {
    unwrap(): T;
}
interface Boxed<T> extends BoxBase<T> {
    readonly boxed: true;
}
declare global {
    interface Array<T> {
        intoBoxed(): Boxed<T>;
    }
}
"#
        .to_string(),
    ))
}

#[test]
fn cross_file_inferred_external_module_lib_generic_keeps_inherited_member() {
    let mut libs = load_default_lib_files();
    libs.push(custom_external_module_lib());

    let diags = check_multi_file_with_libs(
        &[
            (
                "producer.ts",
                r#"
export class Producer {
    produce() { return [1, 2, 3].intoBoxed(); }
}
"#,
            ),
            (
                "consumer.ts",
                r#"
import { Producer } from "./producer";
const p = new Producer();
const b = p.produce();
const n: number = b.unwrap();
"#,
            ),
        ],
        "consumer.ts",
        ts_options(),
        &libs,
    );

    let offending: Vec<(u32, String)> = diags
        .iter()
        .filter(|d| d.code == 2339)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    assert!(
        offending.is_empty(),
        "inherited `BoxBase.unwrap` must survive cross-file resolution of the \
         external-module lib generic `Boxed<T>`; got {offending:#?}"
    );
}
