#[test]
fn test_property_access_class_component_missing_required_prop_emits_ts2322_not_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P, S> {{
    props: P;
    state: S;
    constructor(props?: P, context?: any);
    render(): JSX.Element;
}}
interface ComponentClass<P> {{
    new (props?: P, context?: any): Component<P, any>;
}}
declare namespace TestMod {{
    interface TestClass extends ComponentClass<{{ reqd: string }}> {{}}
    var Test: TestClass;
}}
const T = TestMod.Test;
let p1 = <T />;
let p2 = <TestMod.Test />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Missing required property-access class-component props should emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Missing required property-access class-component props should not fall back to TS2741, got: {diags:?}"
    );
}

// =============================================================================
// Cross-file: import React = require('react') with ambient module
// =============================================================================

fn load_cross_file_jsx_lib_files() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let search_roots: Vec<&Path> = {
        let mut roots = vec![manifest_dir];
        let mut parent = manifest_dir.parent();
        while let Some(dir) = parent {
            roots.push(dir);
            parent = dir.parent();
        }
        roots
    };
    let candidates = [(
        "lib.es5.d.ts",
        [
            "scripts/node_modules/typescript/lib/lib.es5.d.ts",
            "scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts",
            "scripts/emit/node_modules/typescript/lib/lib.es5.d.ts",
            "crates/tsz-core/src/lib-assets-stripped/es5.d.ts",
            "crates/tsz-core/src/lib-assets/es5.d.ts",
            "../tsz-core/src/lib-assets-stripped/es5.d.ts",
            "../tsz-core/src/lib-assets/es5.d.ts",
            "TypeScript/node_modules/typescript/lib/lib.es5.d.ts",
            "TypeScript/src/lib/es5.d.ts",
        ],
    )];

    let mut lib_files = Vec::new();
    for (file_name, suffixes) in candidates {
        let maybe_path = search_roots
            .iter()
            .flat_map(|root| suffixes.iter().map(move |suffix| root.join(suffix)))
            .find(|path| path.exists());
        if let Some(path) = maybe_path
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            lib_files.push(Arc::new(LibFile::from_source(
                file_name.to_string(),
                content,
            )));
        }
    }
    lib_files
}

/// Helper to compile a multi-file JSX project and return diagnostics for the main file.
fn cross_file_jsx_diagnostics(lib_source: &str, main_source: &str) -> Vec<(u32, String)> {
    cross_file_jsx_diagnostics_with_mode_and_default_libs(
        lib_source,
        main_source,
        JsxMode::Preserve,
        false,
    )
}

fn cross_file_jsx_diagnostics_with_mode(
    lib_source: &str,
    main_source: &str,
    jsx_mode: JsxMode,
) -> Vec<(u32, String)> {
    cross_file_jsx_diagnostics_with_mode_and_default_libs(lib_source, main_source, jsx_mode, false)
}

fn cross_file_jsx_diagnostics_with_mode_and_default_libs(
    lib_source: &str,
    main_source: &str,
    jsx_mode: JsxMode,
    include_default_libs: bool,
) -> Vec<(u32, String)> {
    let default_lib_files = if include_default_libs {
        load_cross_file_jsx_lib_files()
    } else {
        Vec::new()
    };

    // Parse and bind lib file (react.d.ts equivalent)
    let mut parser_lib = ParserState::new("react.d.ts".to_string(), lib_source.to_string());
    let root_lib = parser_lib.parse_source_file();
    let mut binder_lib = tsz_binder::BinderState::new();
    binder_lib.bind_source_file(parser_lib.get_arena(), root_lib);
    let arena_lib = Arc::new(parser_lib.get_arena().clone());
    let binder_lib = Arc::new(binder_lib);

    // Parse and bind main file
    let mut parser_main = ParserState::new("file.tsx".to_string(), main_source.to_string());
    let root_main = parser_main.parse_source_file();
    let mut binder_main = tsz_binder::BinderState::new();
    let mut raw_lib_contexts: Vec<_> = default_lib_files
        .iter()
        .map(|lib| tsz_binder::state::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    raw_lib_contexts.push(tsz_binder::state::LibContext {
        arena: Arc::clone(&arena_lib),
        binder: Arc::clone(&binder_lib),
    });
    binder_main.merge_lib_contexts_into_binder(&raw_lib_contexts);
    binder_main.bind_source_file(parser_main.get_arena(), root_main);

    let arena_main = Arc::new(parser_main.get_arena().clone());
    let binder_main = Arc::new(binder_main);

    let mut all_arenas_vec = vec![Arc::clone(&arena_main), Arc::clone(&arena_lib)];
    let mut all_binders_vec = vec![Arc::clone(&binder_main), Arc::clone(&binder_lib)];
    for lib in &default_lib_files {
        all_arenas_vec.push(Arc::clone(&lib.arena));
        all_binders_vec.push(Arc::clone(&lib.binder));
    }
    let all_arenas = Arc::new(all_arenas_vec);
    let all_binders = Arc::new(all_binders_vec);

    let options = CheckerOptions {
        jsx_mode,
        ..CheckerOptions::default()
    };

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        "file.tsx".to_string(),
        options,
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    let mut checker_lib_contexts: Vec<_> = default_lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker_lib_contexts.push(tsz_checker::context::LibContext {
        arena: Arc::clone(&arena_lib),
        binder: Arc::clone(&binder_lib),
    });
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker
        .ctx
        .set_actual_lib_file_count(default_lib_files.len() + 1);

    checker.check_source_file(root_main);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn load_typescript_fixture(rel_path: &str) -> Option<String> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../").join(rel_path),
        manifest_dir.join("../../../").join(rel_path),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return std::fs::read_to_string(candidate).ok();
        }
    }

    None
}

#[test]
fn test_cross_file_import_require_export_equals() {
    // Simulate: declare module "react" { export = __React; }
    // with: import React = require('react')
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace __React {
    class Component<P, S = {}> {
        props: P & { children?: any };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
    }
}
declare module "react" {
    export = __React;
}
"#;

    let main_source = r#"
import React = require('react');

interface Prop {
    x: false;
}
class Poisoned extends React.Component<Prop, {}> {
    render() {
        return <div>Hello</div>;
    }
}

let p = <Poisoned x />;
"#;

    let diags = cross_file_jsx_diagnostics_with_mode_and_default_libs(
        lib_source,
        main_source,
        JsxMode::Preserve,
        true,
    );
    // The export= resolution should work — no TS2307 "Cannot find module"
    assert!(
        !has_code(&diags, 2307),
        "Should not emit TS2307 for resolvable ambient module, got: {diags:?}"
    );
}

#[test]
#[ignore] // TODO: needs default lib types (Array, Object, etc.) to avoid TS2318 flood
fn test_cross_file_react_class_generic_props_emit_errors() {
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace __React {
    interface Attributes {
        key?: string | number;
    }
    interface ClassAttributes<T> extends Attributes {
        ref?: (instance: T) => any;
    }
    interface ReactNode {
        readonly __tsz_react_node: true;
    }
    class Component<P, S = {}> {
        props: P & { children?: ReactNode };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
    }
}
declare module "react" {
    export = __React;
}
"#;

    let main_source = r#"
import React = require('react');

interface Prop {
    a: number,
    b: string
}

declare class MyComp<P extends Prop> extends React.Component<P, {}> {
    internalProp: P;
    render() {
        return <div>Hello</div>;
    }
}

let x1 = <MyComp />;
let x2 = <MyComp a="hi" />;
"#;

    let diags = cross_file_jsx_diagnostics(lib_source, main_source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for class component prop mismatch, got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2739 for missing class component props, got: {diags:?}"
    );
}

#[test]
fn test_cross_file_react_component_override_emits_ts2416() {
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace __React {
    class Component<P, S = {}> {
        props: P & { children?: any };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
    }
}
declare module "react" {
    export = __React;
}
"#;

    let main_source = r#"
import React = require('react');

class B1<T extends { x: string }> extends React.Component<T, {}> {
    render() {
        return <div>hi</div>;
    }
}
class B<U> extends React.Component<U, {}> {
    props: U;
    render() {
        return <B1 {...this.props} x="hi" />;
    }
}
"#;

    let diags = cross_file_jsx_diagnostics(lib_source, main_source);
    assert!(
        !has_code(&diags, 2307),
        "Should resolve the ambient React module, got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE
        ),
        "Expected TS2416 for incompatible inherited props override across the React module boundary, got: {diags:?}"
    );
}

// =============================================================================
// TS2698: JSX spread type validation
// =============================================================================

#[test]
fn test_ts2698_spread_null_emits_error() {
    // Spreading `null` in JSX should emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = null;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading null, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_spread_undefined_emits_error() {
    // Spreading `undefined` in JSX should emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = undefined;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading undefined, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_spread_never_emits_error() {
    // Spreading `never` in JSX should emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = {} as never;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading never, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_not_emitted_for_object_spread() {
    // Spreading a valid object in JSX should NOT emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = { x: 1 };
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Should NOT emit TS2698 for object spread, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_not_emitted_for_any_spread() {
    // Spreading `any` in JSX should NOT emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
declare var a: any;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Should NOT emit TS2698 for any spread, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_works_with_intrinsic_any_props() {
    // TS2698 should fire even when IntrinsicElements has [key: string]: any
    // (i.e., when skip_prop_checks would be true). The spread type validation
    // is independent of the props type.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const b = null;
const c = undefined;
const d = <div { ...b } />;
const e = <div { ...c } />;
"#;
    let diags = jsx_diagnostics(source);
    let ts2698_count = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES)
        .count();
    assert!(
        ts2698_count >= 2,
        "Expected at least 2 TS2698 errors (for null and undefined spreads), got {ts2698_count}: {diags:?}"
    );
}

// =============================================================================
// Intrinsic element return type: JSX.Element
// =============================================================================

