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

// Known-failing cases tracked by issue #7690 — drop the `#[ignore]` when fixed.

#[ignore = "tsz issue #7690: local `declare function` + cross-file import emits TS2345/TS2349 instead of resolving to lib Node"]
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

#[ignore = "tsz issue #7690: subclass of lib HTMLElement declared in another module loses heritage chain to Element/Node"]
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
