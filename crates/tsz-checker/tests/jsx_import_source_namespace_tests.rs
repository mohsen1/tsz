use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::checker_options::JsxMode;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_named_files(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .expect("entry file should exist");
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn jsx_import_source_namespace_overrides_global_jsx_intrinsic_elements() {
    let diagnostics = compile_named_files(
        &[
            (
                "/node_modules/react/index.d.ts",
                r#"
export = React;
export as namespace React;

declare namespace React {}

declare global {
  namespace JSX {
    interface Element {}
    interface ElementClass {}
    interface ElementAttributesProperty {}
    interface ElementChildrenAttribute {}
    type LibraryManagedAttributes<C, P> = {};
    interface IntrinsicAttributes {}
    interface IntrinsicClassAttributes<T> {}
    interface IntrinsicElements {
      div: {};
    }
  }
}
"#,
            ),
            (
                "/node_modules/@emotion/react/jsx-runtime/index.d.ts",
                r#"
export { EmotionJSX as JSX } from "./jsx-namespace";
"#,
            ),
            (
                "/node_modules/@emotion/react/jsx-runtime/jsx-namespace.d.ts",
                r#"
import "react";

type WithConditionalCSSProp<P> = "className" extends keyof P
  ? (P extends { className?: string } ? P & { css?: string } : P)
  : P;

type ReactJSXElement = JSX.Element;
type ReactJSXElementClass = JSX.ElementClass;
type ReactJSXElementAttributesProperty = JSX.ElementAttributesProperty;
type ReactJSXElementChildrenAttribute = JSX.ElementChildrenAttribute;
type ReactJSXLibraryManagedAttributes<C, P> = JSX.LibraryManagedAttributes<C, P>;
type ReactJSXIntrinsicAttributes = JSX.IntrinsicAttributes;
type ReactJSXIntrinsicClassAttributes<T> = JSX.IntrinsicClassAttributes<T>;
type ReactJSXIntrinsicElements = JSX.IntrinsicElements;

export namespace EmotionJSX {
  export interface Element extends ReactJSXElement {}
  export interface ElementClass extends ReactJSXElementClass {}
  export interface ElementAttributesProperty
    extends ReactJSXElementAttributesProperty {}
  export interface ElementChildrenAttribute extends ReactJSXElementChildrenAttribute {}

  export type LibraryManagedAttributes<C, P> = WithConditionalCSSProp<P> &
    ReactJSXLibraryManagedAttributes<C, P>;

  export interface IntrinsicAttributes extends ReactJSXIntrinsicAttributes {}
  export interface IntrinsicClassAttributes<T>
    extends ReactJSXIntrinsicClassAttributes<T> {}

  export type IntrinsicElements = {
    [K in keyof ReactJSXIntrinsicElements]: ReactJSXIntrinsicElements[K] & {
      css?: string;
    };
  };
}
"#,
            ),
            (
                "/index.tsx",
                r#"
export const Comp = () => <div css="color: hotpink;"></div>;
export const Bad = () => <div nope={1}></div>;
"#,
            ),
        ],
        "/index.tsx",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            jsx_mode: JsxMode::ReactJsx,
            jsx_import_source: "@emotion/react".to_string(),
            no_lib: true,
            strict: true,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2456),
        "Expected JSX runtime bridge aliases to avoid false circularity. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2322.len(),
        1,
        "Expected only the invalid intrinsic attribute to fail. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2322.iter().any(|(_, message)| message.contains("nope")),
        "Expected excess intrinsic attributes to still be checked. Actual diagnostics: {diagnostics:#?}"
    );
}
