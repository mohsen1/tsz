//! Cross-file lib-interface identity.
//!
//! When a lib interface (`Element`, `HTMLElement`, ...) appears in another
//! module's exported declaration, the imported binding's type must remain
//! the same lib interface — with the same heritage chain — as the one the
//! consuming file resolves. Breaking that identity makes `Element` → `Node`
//! and similar lib-heritage assignments fail.

use std::sync::{Arc, OnceLock};

use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn dom_libs() -> &'static Vec<Arc<LibFile>> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(|| {
        tsz_checker::test_utils::load_lib_files(&[
            "es5.d.ts",
            "es2015.iterable.d.ts",
            "es2015.symbol.d.ts",
            "es2015.symbol.wellknown.d.ts",
            "dom.d.ts",
        ])
    })
}

fn compile_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_multi_file_with_libs(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        dom_libs(),
    )
    .into_iter()
    // TS2318 ("Cannot find global type") is noise from the stripped lib bundle
    // and unrelated to assignability identity.
    .filter(|d| d.code != 2318)
    .map(|d| d.code)
    .collect()
}

#[test]
fn imported_element_is_assignable_to_node() {
    let exporter = "export const blogPost: Element;";
    let consumer = r#"
import { blogPost } from "./component";
const n: Node = blogPost;
// Accessing an Element-only property pins the import to lib `Element`; if the
// import were silently `any` or `unknown` this would emit TS2339.
const tag: string = blogPost.tagName;
"#;

    let codes = compile_codes(
        &[("component.d.ts", exporter), ("main.ts", consumer)],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn imported_element_can_be_passed_as_node_argument_when_function_is_imported_too() {
    let exporter = r#"
export const blogPost: Element;
export declare function takeNode(child: Node): Node;
"#;
    let consumer = r#"
import { blogPost, takeNode } from "./component";
takeNode(blogPost);
"#;

    let codes = compile_codes(
        &[("component.d.ts", exporter), ("main.ts", consumer)],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn locally_declared_element_can_be_passed_as_node_argument() {
    let consumer = r#"
declare const blogPost: Element;
declare function takeNode(child: Node): Node;
takeNode(blogPost);
"#;

    let codes = compile_codes(&[("main.ts", consumer)], "main.ts");
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn imported_html_element_is_assignable_to_element_and_node() {
    let exporter = "export const html: HTMLElement;";
    let consumer = r#"
import { html } from "./component";
const e: Element = html;
const n: Node = html;
"#;

    let codes = compile_codes(
        &[("component.d.ts", exporter), ("main.ts", consumer)],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn imported_div_element_is_assignable_to_html_element_and_node() {
    let exporter = "export const div: HTMLDivElement;";
    let consumer = r#"
import { div } from "./component";
const he: HTMLElement = div;
const e: Element = div;
const n: Node = div;
"#;

    let codes = compile_codes(
        &[("component.d.ts", exporter), ("main.ts", consumer)],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn imported_element_through_namespace_reexport_is_assignable_to_node() {
    let leaf = "export const blogPost: Element;";
    let reexporter = r#"export * as mod from "./component";"#;
    let consumer = r#"
import { mod } from "./reexport";
const n: Node = mod.blogPost;
"#;

    let codes = compile_codes(
        &[
            ("component.d.ts", leaf),
            ("reexport.d.ts", reexporter),
            ("main.ts", consumer),
        ],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn imported_element_keeps_lib_heritage_when_renamed_on_import() {
    let exporter = "export const blogPost: Element;";
    let consumer = r#"
import { blogPost as bp } from "./component";
const n: Node = bp;
"#;

    let codes = compile_codes(
        &[("component.d.ts", exporter), ("main.ts", consumer)],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn local_declare_function_after_import_is_callable_with_cross_file_element() {
    let exporter = "export const blogPost: Element;";
    let consumer = r#"
import { blogPost } from "./component";
declare function takeNode(child: Node): Node;
takeNode(blogPost);
"#;

    let codes = compile_codes(
        &[("component.d.ts", exporter), ("main.ts", consumer)],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

// Regression coverage for the broader cross-arena `SymbolId` collision rule
// that surfaced through issue #8476: a local `function` declaration whose raw
// `SymbolId` happens to collide with a value symbol from another project file
// must still resolve to its own function type — not the colliding symbol's
// type — when called. The fix lives in checker cross-arena delegation.
//
// All three tests below use plain identifiers and primitive types so the
// rule is provable without DOM lib heritage, and the matrix varies the
// declaration shape, the call shape, and the source-symbol name to prove the
// fix is structural rather than keyed on `f` / `x` / `Element`.

#[test]
fn local_declare_function_after_value_import_is_callable() {
    // The reduced repro: no lib types, no `declare function` heritage —
    // a value import followed by a local declared function. Before the
    // fix, calling `f()` reported TS2349 because cross-arena delegation
    // computed the imported symbol's type under the local function's
    // raw `SymbolId`.
    let exporter = "export const x: string;";
    let consumer = r#"
import { x } from "./other";
declare function f(): void;
f();
"#;

    let codes = compile_codes(&[("other.ts", exporter), ("main.ts", consumer)], "main.ts");
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn local_function_implementation_after_value_import_is_callable_with_renamed_names() {
    // Renamed identifiers prove the rule is structural rather than keyed
    // on `f` / `x`; a real function body (not `declare`) covers the
    // non-ambient path.
    let exporter = "export const greeting: number;";
    let consumer = r#"
import { greeting } from "./other";
function makeGreeting(): string { return "ok"; }
const imported: number = greeting;
const result: string = makeGreeting();
"#;

    let codes = compile_codes(&[("other.ts", exporter), ("main.ts", consumer)], "main.ts");
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn multiple_local_functions_after_value_import_each_keep_callable_type() {
    // Multiple local functions after a named value import: every function
    // declaration must keep its own callable type — not collapse to a
    // single colliding symbol's type — when raw `SymbolId`s collide with
    // names exported from another project file.
    let exporter = "export const greeting: number; export const farewell: number;";
    let consumer = r#"
import { greeting, farewell } from "./other";
declare function first(): number;
declare function second(): number;
const importedGreeting: number = greeting;
const importedFarewell: number = farewell;
const a: number = first();
const b: number = second();
"#;

    let codes = compile_codes(&[("other.ts", exporter), ("main.ts", consumer)], "main.ts");
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

// Regression coverage for issue #8476: a class declared in another file that
// extends a lib type retains its full heritage chain across the import edge.
// Before the named-import-alias fix in `get_cross_file_symbol`/
// `get_symbol_globally`, the foreign-file's symbol at the colliding raw
// `SymbolId` would replace the local alias, and the imported binding's type
// rendered as `typeof instance` instead of `MyElement` — dropping the
// `Element`/`Node` properties.

#[test]
fn imported_element_class_extension_unifies_with_lib_html_element() {
    let leaf = r#"
export class MyElement extends HTMLElement {
    connectedCallback(): void;
}
export const instance: MyElement;
"#;
    let consumer = r#"
import { instance } from "./component";
const he: HTMLElement = instance;
const e: Element = instance;
const n: Node = instance;
"#;

    let codes = compile_codes(
        &[("component.d.ts", leaf), ("main.ts", consumer)],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn imported_class_extension_unifies_with_lib_html_element_with_renamed_names() {
    // Anti-§25 hardcoding: the structural rule must hold when the exported
    // class/instance and the importing alias are renamed.
    let leaf = r#"
export class CustomWidget extends HTMLDivElement {
    extra(): void;
}
export const widget: CustomWidget;
"#;
    let consumer = r#"
import { widget as w } from "./component";
const div: HTMLDivElement = w;
const he: HTMLElement = w;
const e: Element = w;
const n: Node = w;
"#;

    let codes = compile_codes(
        &[("component.d.ts", leaf), ("main.ts", consumer)],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}

#[test]
fn imported_class_extension_heritage_preserved_through_namespace_reexport() {
    // Adjacent shape: the class flows through a `export *` re-export rather
    // than a direct named import. The reexport edge must not lose the
    // heritage chain either.
    let leaf = r#"
export class Anchor extends HTMLAnchorElement {
    label(): void;
}
export const anchor: Anchor;
"#;
    let reexporter = r#"export * from "./component";"#;
    let consumer = r#"
import { anchor } from "./reexport";
const a: HTMLAnchorElement = anchor;
const he: HTMLElement = anchor;
const e: Element = anchor;
const n: Node = anchor;
"#;

    let codes = compile_codes(
        &[
            ("component.d.ts", leaf),
            ("reexport.d.ts", reexporter),
            ("main.ts", consumer),
        ],
        "main.ts",
    );
    assert!(codes.is_empty(), "Diagnostics: {codes:?}");
}
