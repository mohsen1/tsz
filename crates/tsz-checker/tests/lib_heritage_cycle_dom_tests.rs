//! Regression coverage for lib-interface heritage that is dropped when a base
//! interface is mid-resolution under a real resolution-ordering cycle.
//!
//! See issue #12299: a DOM interface (`Element`/`HTMLElement`) that extends
//! `Node` both directly and through `ChildNode`/`ParentNode` (a diamond) was
//! built without any `Node` members when `Element` happened to be resolved as a
//! nested side effect during `Node`'s own resolution. The relation-input
//! readiness warmup ran while `Node`'s resolution-cache slot still held the
//! in-progress `None` sentinel; the nested `HTMLElement` -> `Element` chain then
//! resolved `extends Node` to `None` and silently dropped it, caching an
//! incomplete type. That produced false `TS2339` on inherited methods
//! (`appendChild`, `cloneNode`, ...) and false `TS2740`/`TS2322` for
//! `Element`-is-not-assignable-to-`Node`.
//!
//! These cases exercise the *real* resolution ordering, so they need the full
//! (compiled) DOM lib rather than the stripped bundle — the stripped interfaces
//! do not reproduce the ordering. The lib files are tracked in-repo under
//! `crates/tsz-website/src/lib`, which `load_compiled_lib_files` probes.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, diagnostic_code_messages, diagnostic_codes, load_compiled_lib_files,
};
use tsz_common::common::{ModuleKind, ScriptTarget};

/// A target-ESNext-ish lib bundle that includes the DOM definitions.
const LIBS: &[&str] = &[
    "lib.es5.d.ts",
    "lib.es2015.d.ts",
    "lib.es2015.core.d.ts",
    "lib.es2015.collection.d.ts",
    "lib.es2015.iterable.d.ts",
    "lib.es2015.generator.d.ts",
    "lib.es2015.promise.d.ts",
    "lib.es2015.proxy.d.ts",
    "lib.es2015.reflect.d.ts",
    "lib.es2015.symbol.d.ts",
    "lib.es2015.symbol.wellknown.d.ts",
    "lib.es2016.array.include.d.ts",
    "lib.es2017.d.ts",
    "lib.es2017.object.d.ts",
    "lib.es2017.string.d.ts",
    "lib.es2018.d.ts",
    "lib.es2019.d.ts",
    "lib.dom.d.ts",
    "lib.dom.iterable.d.ts",
];

const ES5_DOM_LIBS: &[&str] = &["lib.es5.d.ts", "lib.dom.d.ts"];

fn dom_codes(source: &str) -> Vec<u32> {
    dom_codes_with_libs(source, LIBS)
}

fn es5_dom_messages(source: &str) -> Vec<(u32, String)> {
    let libs = load_compiled_lib_files(ES5_DOM_LIBS);
    let diags = check_multi_file_with_libs(
        &[("main.ts", source)],
        "main.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
        &libs,
    );
    diagnostic_code_messages(diags)
}

fn dom_codes_with_libs(source: &str, lib_names: &[&str]) -> Vec<u32> {
    let libs = load_compiled_lib_files(lib_names);
    // Guard: if the compiled DOM lib is not present in any probed root, the
    // ordering under test cannot be exercised. Treat that as a skip rather than
    // a spurious pass/failure.
    assert!(
        libs.iter().any(|l| l.file_name == "lib.dom.d.ts"),
        "compiled lib.dom.d.ts not found in any probed lib root"
    );
    let diags = check_multi_file_with_libs(
        &[("main.ts", source)],
        "main.ts",
        CheckerOptions::default(),
        &libs,
    );
    diagnostic_codes(&diags)
}

#[test]
fn node_resolved_first_then_html_element_methods_no_ts2339() {
    // Force `Node` to be the first DOM interface resolved, so `HTMLElement` /
    // `Element` are pulled in transitively while `Node` is mid-resolution.
    let codes = dom_codes(
        "declare const n: Node;\n\
         declare const h: HTMLElement;\n\
         h.appendChild(h);\n\
         h.cloneNode();\n\
         h.removeChild(h);\n",
    );
    assert!(
        !codes.contains(&2339),
        "HTMLElement must inherit Node methods (appendChild/cloneNode/...): {codes:?}"
    );
}

#[test]
fn node_resolved_first_then_element_assignable_to_node() {
    let codes = dom_codes(
        "declare const n: Node;\n\
         declare const e: Element;\n\
         const nn: Node = e;\n",
    );
    assert!(
        !codes.contains(&2740) && !codes.contains(&2322),
        "Element must be assignable to Node (extends it directly and via ChildNode/ParentNode): {codes:?}"
    );
}

#[test]
fn document_resolved_first_then_html_element_methods_no_ts2339() {
    let codes = dom_codes(
        "declare const d: Document;\n\
         declare const h: HTMLElement;\n\
         h.appendChild(h);\n",
    );
    assert!(
        !codes.contains(&2339),
        "HTMLElement must inherit Node methods regardless of which interface resolves first: {codes:?}"
    );
}

#[test]
fn document_body_append_child_no_ts2339() {
    // The exact shape from the `importMeta.ts` conformance fixture that issue
    // #12299 identified as keeping that fixture in the accepted-regression list.
    let codes = dom_codes(
        "const image = document.createElement('img');\n\
         document.body.appendChild(image);\n",
    );
    assert!(
        !codes.contains(&2339),
        "document.body.appendChild must resolve (HTMLElement inherits Node.appendChild): {codes:?}"
    );
}

#[test]
fn es5_dom_document_body_append_child_no_ts2339() {
    // The `importMeta.ts` conformance fixture requests exactly `es5,dom`; keep
    // that narrower lib-set from dropping inherited `Node.appendChild`.
    let messages = es5_dom_messages(
        "(async () => {\n\
         const response = await fetch(new URL(\"../hamsters.jpg\", import.meta.url).toString());\n\
         const blob = await response.blob();\n\
         const size = import.meta.scriptElement.dataset.size || 300;\n\
         const image = new Image();\n\
         image.src = URL.createObjectURL(blob);\n\
         image.width = image.height = size;\n\
         document.body.appendChild(image);\n\
         })();\n",
    );
    assert!(
        !messages.iter().any(|(code, message)| {
            *code == 2339 && message.contains("appendChild") && message.contains("HTMLElement")
        }),
        "document.body.appendChild must resolve with only es5,dom libs: {messages:?}"
    );
}

#[test]
fn es5_dom_import_meta_fixture_append_child_no_extra_ts2339() {
    let libs = load_compiled_lib_files(ES5_DOM_LIBS);
    let diagnostics = check_multi_file_with_libs(
        &[
            (
                "example.ts",
                r#"
(async () => {
  const response = await fetch(new URL("../hamsters.jpg", import.meta.url).toString());
  const blob = await response.blob();

  const size = import.meta.scriptElement.dataset.size || 300;

  const image = new Image();
  image.src = URL.createObjectURL(blob);
  image.width = image.height = size;

  document.body.appendChild(image);
})();
"#,
            ),
            (
                "moduleLookingFile01.ts",
                r#"
export let x = import.meta;
export let y = import.metal;
export let z = import.import.import.malkovich;
"#,
            ),
            (
                "scriptLookingFile01.ts",
                r#"
let globalA = import.meta;
let globalB = import.metal;
let globalC = import.import.import.malkovich;
"#,
            ),
            (
                "assignmentTargets.ts",
                r#"
export const foo: ImportMeta = import.meta.blah = import.meta.blue = import.meta;
import.meta = foo;
"#,
            ),
            (
                "augmentations.ts",
                r#"
declare global {
  interface ImportMeta {
    wellKnownProperty: { a: number, b: string, c: boolean };
  }
}

const { a, b, c } = import.meta.wellKnownProperty;
"#,
            ),
        ],
        "example.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let messages = diagnostic_code_messages(diagnostics);
    assert!(
        !messages.iter().any(|(code, message)| {
            *code == 2339 && message.contains("appendChild") && message.contains("HTMLElement")
        }),
        "importMeta fixture should not add appendChild/HTMLElement TS2339: {messages:?}"
    );
}

#[test]
fn es5_dom_import_meta_global_augmentation_keeps_image_assignable_to_node() {
    let libs = load_compiled_lib_files(ES5_DOM_LIBS);
    let diagnostics = check_multi_file_with_libs(
        &[
            (
                "example.ts",
                r#"
(async () => {
  const response = await fetch(new URL("../hamsters.jpg", import.meta.url).toString());
  const blob = await response.blob();
  const size = import.meta.scriptElement.dataset.size || 300;
  const image = new Image();
  image.src = URL.createObjectURL(blob);
  image.width = image.height = size;
  document.body.appendChild(image);
})();
"#,
            ),
            (
                "augmentations.ts",
                r#"
declare global {
  interface ImportMeta {
    wellKnownProperty: { a: number, b: string, c: boolean };
  }
}
const { a, b, c } = import.meta.wellKnownProperty;
"#,
            ),
        ],
        "example.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let messages = diagnostic_code_messages(diagnostics);
    assert!(
        !messages.iter().any(|(code, message)| {
            (*code == 2339 && message.contains("appendChild"))
                || (*code == 2345
                    && message.contains("HTMLImageElement")
                    && message.contains("Node"))
        }),
        "global augmentation must not drop DOM heritage for chained appendChild: {messages:?}"
    );
}

#[test]
fn element_inherits_node_methods_no_ts2339() {
    let codes = dom_codes(
        "declare const el: Element;\n\
         el.appendChild(el);\n\
         el.cloneNode();\n",
    );
    assert!(
        !codes.contains(&2339),
        "Element must inherit Node methods through heritage: {codes:?}"
    );
}

#[test]
fn node_own_members_still_resolve() {
    // Guard: the base interface itself keeps resolving cleanly.
    let codes = dom_codes(
        "declare const n: Node;\n\
         n.appendChild(n);\n\
         n.nodeName;\n",
    );
    assert!(
        !codes.contains(&2339),
        "Node's own members must resolve: {codes:?}"
    );
}
