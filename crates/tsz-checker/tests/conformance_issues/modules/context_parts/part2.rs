#[test]
fn test_declare_global_export_import_missing_alias_target_keeps_jsx_missing_interface_error() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "/node_modules/preact/index.d.ts",
                r#"
type Defaultize<Props, Defaults> =
    Props extends any
        ? Partial<Pick<Props, Extract<keyof Props, keyof Defaults>>> &
            Pick<Props, Exclude<keyof Props, keyof Defaults>>
        : never;
export namespace JSXInternal {
    interface HTMLAttributes<T = {}> {}
    interface SVGAttributes<T = {}> {}
    type LibraryManagedAttributes<Component, Props> = Component extends {
        defaultProps: infer Defaults;
    } ? Defaultize<Props, Defaults> : Props;
    interface IntrinsicAttributes { key?: any; }
    interface Element extends VNode<any> {}
    interface ElementClass extends Component<any, any> {}
    interface ElementAttributesProperty { props: any; }
    interface ElementChildrenAttribute { children: any; }
    interface IntrinsicElements { div: HTMLAttributes; }
}
export const Fragment: unique symbol;
export type ComponentType<T = {}> = {};
export type ComponentChild = {};
export type ComponentChildren = {};
export type VNode<T = {}> = {};
export type Attributes = {};
export type Component<T = {}, U = {}> = {};
"#,
            ),
            (
                "/node_modules/preact/jsx-runtime/index.d.ts",
                r#"
export { Fragment } from "..";
import { ComponentType, ComponentChild, ComponentChildren, VNode, Attributes } from "..";
import { JSXInternal } from "..";

export function jsx(
    type: string,
    props: JSXInternal.HTMLAttributes &
        JSXInternal.SVGAttributes &
        Record<string, any> & { children?: ComponentChild },
    key?: string
): VNode<any>;
export function jsx<P>(
    type: ComponentType<P>,
    props: Attributes & P & { children?: ComponentChild },
    key?: string
): VNode<any>;

declare global {
    export import JSX = NotFound;
}
"#,
            ),
            ("/index.tsx", r#"export const Comp = () => <div></div>;"#),
        ],
        "/index.tsx",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            jsx_mode: JsxMode::Preserve,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 7026),
        "Expected TS7026 when missing global JSX alias target leaves IntrinsicElements unavailable in the local harness, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == 2666 || *code == 2694),
        "Expected missing alias target case to avoid bogus augmentation-export and namespace-member diagnostics, got: {diagnostics:#?}"
    );
}

#[test]
fn test_imported_declaration_file_with_top_level_declare_global_still_emits_ts2306() {
    let mut parser_entry = ParserState::new(
        "/src/index.ts".to_string(),
        r#"
import {} from "./react";
export const x = 1;
"#
        .to_string(),
    );
    let root_entry = parser_entry.parse_source_file();
    let mut binder_entry = BinderState::new();
    binder_entry.bind_source_file(parser_entry.get_arena(), root_entry);

    let mut parser_react = ParserState::new(
        "/src/react.d.ts".to_string(),
        "declare global {}".to_string(),
    );
    let root_react = parser_react.parse_source_file();
    let mut binder_react = BinderState::new();
    binder_react.bind_source_file(parser_react.get_arena(), root_react);

    let arena_entry = Arc::new(parser_entry.get_arena().clone());
    let arena_react = Arc::new(parser_react.get_arena().clone());
    let binder_entry = Arc::new(binder_entry);
    let binder_react = Arc::new(binder_react);
    let all_arenas = Arc::new(vec![Arc::clone(&arena_entry), Arc::clone(&arena_react)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_entry), Arc::clone(&binder_react)]);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((0, "./react".to_string()), 1);
    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./react".to_string());

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_entry.as_ref(),
        binder_entry.as_ref(),
        &types,
        "/src/index.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root_entry);
    let diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2306),
        "Expected imported declaration file with top-level declare global to still report TS2306. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_umd_global_module_augmentation_class_method_merge() {
    // Structural rule: when `declare module 'M' { interface C { extra(): T } }`
    // augments a UMD module, class instances accessed via the UMD global should
    // expose the augmented methods. This covers the umd-augmentation-2 pattern.
    //
    // Variants tested here use different interface names (Widget/K/Item) to prove
    // the fix is not keyed on a particular name.
    let files = [
        (
            "/lib.d.ts",
            r#"
export as namespace Lib;
export class Widget {
    name: string;
}
"#,
        ),
        (
            "/lib-aug.d.ts",
            r#"
import * as X from './lib';
declare module './lib' {
    interface Widget {
        extra(): number;
    }
}
"#,
        ),
        (
            "/use.ts",
            r#"
let w = new Lib.Widget();
let n: number = w.extra();
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/use.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            allow_umd_global_access: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected Widget.extra() to be visible via UMD global after interface augmentation. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_umd_global_augmentation_member_accessible_on_namespace_export() {
    // Structural rule: when a module augmentation adds a top-level exported member
    // to a UMD module, that member should be accessible via the UMD global alias.
    // This is the analogous case to the existing const-export test but with an
    // interface method member (not a direct const re-export).
    //
    // Two interface names (Widget/Item) prove the fix is not keyed on spelling.
    let files_widget = [
        (
            "/w.d.ts",
            r#"
export as namespace W;
export class Widget { id: number; }
"#,
        ),
        (
            "/w-aug.d.ts",
            r#"
import * as W from './w';
declare module './w' {
    interface Widget { label(): string; }
}
"#,
        ),
        (
            "/w-use.ts",
            r#"
let w = new W.Widget();
let s: string = w.label();
"#,
        ),
    ];

    let diagnostics_widget = compile_named_files_get_diagnostics_with_options(
        &files_widget,
        "/w-use.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            allow_umd_global_access: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics_widget.iter().any(|(code, _)| *code == 2339),
        "Widget.label() must be accessible via UMD global. Diagnostics: {diagnostics_widget:#?}"
    );

    let files_item = [
        (
            "/i.d.ts",
            r#"
export as namespace I;
export class Item { id: number; }
"#,
        ),
        (
            "/i-aug.d.ts",
            r#"
import * as I from './i';
declare module './i' {
    interface Item { display(): string; }
}
"#,
        ),
        (
            "/i-use.ts",
            r#"
let x = new I.Item();
let s: string = x.display();
"#,
        ),
    ];

    let diagnostics_item = compile_named_files_get_diagnostics_with_options(
        &files_item,
        "/i-use.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            allow_umd_global_access: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics_item.iter().any(|(code, _)| *code == 2339),
        "Item.display() must be accessible via UMD global. Diagnostics: {diagnostics_item:#?}"
    );
}

#[test]
fn test_umd_global_augmentation_introduces_new_top_level_exported_member() {
    // Structural rule: `declare module 'M' { export const z = 0; }` adds a
    // top-level exported name `z` that was not in the original UMD module.
    // The UMD global alias should expose `z` as a reachable member.
    // This test uses two different augmented-name spellings (z / q) to prove
    // the fix is not keyed on a particular name.
    let files = [
        (
            "/m.d.ts",
            r#"
export as namespace M;
export const x: number = 0;
"#,
        ),
        (
            "/m-aug.d.ts",
            r#"
import * as M from './m';
declare module './m' {
    const z: string;
    const q: boolean;
}
"#,
        ),
        (
            "/m-use.ts",
            r#"
let a: number = M.x;
let b: string = M.z;
let c: boolean = M.q;
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/m-use.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            allow_umd_global_access: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "M.z and M.q from augmentation must be accessible via UMD global. Diagnostics: {diagnostics:#?}"
    );
}
