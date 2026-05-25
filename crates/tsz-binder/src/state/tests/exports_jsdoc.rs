use super::*;

#[test]
fn test_namespace_exports_exclude_non_exported_members() {
    let source = r"
namespace M {
    export class A {}
    class B {}
}
";
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let m_sym_id = binder
        .file_locals
        .get("M")
        .expect("expected namespace symbol for M");
    let symbol = binder
        .symbols
        .get(m_sym_id)
        .expect("expected namespace symbol data");
    let exports = symbol.exports.as_ref().expect("expected exports table");

    assert!(exports.has("A"), "expected A to be exported");
    assert!(!exports.has("B"), "expected B to be non-exported");
}

#[test]
fn export_namespace_wrapper_marks_inner_module_as_publicly_exported() {
    let source = r"
namespace M {
    export namespace foo {
        export var y = 1;
    }
    namespace foo {
        export var z = 1;
    }
}
";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let source_file = arena
        .get_source_file_at(root)
        .expect("expected source file");
    let outer_ns_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected outer namespace");
    let outer_ns = arena
        .get_module_at(outer_ns_idx)
        .expect("expected outer namespace declaration");
    let body = arena
        .get_module_block_at(outer_ns.body)
        .expect("expected outer namespace body");
    let statements = body.statements.as_ref().expect("expected inner statements");

    let exported_stmt_idx = statements.nodes[0];
    let exported_stmt = arena
        .get_export_decl_at(exported_stmt_idx)
        .expect("expected export declaration wrapper");
    let exported_foo_idx = exported_stmt.export_clause;
    let plain_foo_idx = statements.nodes[1];

    assert_eq!(
        binder
            .module_declaration_exports_publicly
            .get(&exported_foo_idx.0),
        Some(&true),
        "export namespace foo should be recorded as publicly exported"
    );
    assert_eq!(
        binder
            .module_declaration_exports_publicly
            .get(&plain_foo_idx.0),
        Some(&false),
        "plain namespace foo should remain non-exported"
    );
}

#[test]
fn records_import_metadata_for_exported_reexports() {
    let source = r"
export { A, B as C } from './a';
export type { D as E } from './b';
";
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let a_sym_id = binder
        .file_locals
        .get("A")
        .expect("expected re-exported symbol A");
    let a_symbol = binder
        .symbols
        .get(a_sym_id)
        .expect("expected symbol data for A");
    assert_eq!(a_symbol.import_module.as_deref(), Some("./a"));
    assert_eq!(a_symbol.import_name.as_deref(), Some("A"));
    assert!(!a_symbol.is_type_only);

    let c_sym_id = binder
        .file_locals
        .get("C")
        .expect("expected re-exported symbol C");
    let c_symbol = binder
        .symbols
        .get(c_sym_id)
        .expect("expected symbol data for C");
    assert_eq!(c_symbol.import_module.as_deref(), Some("./a"));
    assert_eq!(c_symbol.import_name.as_deref(), Some("B"));
    assert!(!c_symbol.is_type_only);

    let e_sym_id = binder
        .file_locals
        .get("E")
        .expect("expected type-only re-exported symbol E");
    let e_symbol = binder
        .symbols
        .get(e_sym_id)
        .expect("expected symbol data for E");
    assert_eq!(e_symbol.import_module.as_deref(), Some("./b"));
    assert_eq!(e_symbol.import_name.as_deref(), Some("D"));
    assert!(e_symbol.is_type_only);
}

#[test]
fn jsdoc_import_tag_binds_alias_symbols_in_js_files() {
    let source = r#"
/**
 * @import * as NS from "./a"
 * @import { I as RenamedI } from "./a"
 * @import DefaultThing from "./a"
 */
class C {}
"#;
    let mut parser = ParserState::new("b.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let ns_sym_id = binder
        .file_locals
        .get("NS")
        .expect("expected JSDoc namespace import alias");
    let ns_sym = binder
        .symbols
        .get(ns_sym_id)
        .expect("expected symbol data for NS");
    assert_ne!(ns_sym.flags & symbol_flags::ALIAS, 0);
    assert!(ns_sym.is_type_only);
    assert_eq!(ns_sym.import_module.as_deref(), Some("./a"));
    assert_eq!(ns_sym.import_name.as_deref(), Some("*"));

    let renamed_i_sym_id = binder
        .file_locals
        .get("RenamedI")
        .expect("expected JSDoc named import alias");
    let renamed_i_sym = binder
        .symbols
        .get(renamed_i_sym_id)
        .expect("expected symbol data for RenamedI");
    assert_ne!(renamed_i_sym.flags & symbol_flags::ALIAS, 0);
    assert!(renamed_i_sym.is_type_only);
    assert_eq!(renamed_i_sym.import_module.as_deref(), Some("./a"));
    assert_eq!(renamed_i_sym.import_name.as_deref(), Some("I"));

    let default_sym_id = binder
        .file_locals
        .get("DefaultThing")
        .expect("expected JSDoc default import alias");
    let default_sym = binder
        .symbols
        .get(default_sym_id)
        .expect("expected symbol data for DefaultThing");
    assert_ne!(default_sym.flags & symbol_flags::ALIAS, 0);
    assert!(default_sym.is_type_only);
    assert_eq!(default_sym.import_module.as_deref(), Some("./a"));
    assert_eq!(default_sym.import_name.as_deref(), Some("default"));

    assert!(
        binder.file_import_sources.iter().any(|spec| spec == "./a"),
        "expected JSDoc imports to register import source for dependency tracking"
    );
}

#[test]
fn jsdoc_import_tag_binds_namespace_alias_split_across_continuation_lines() {
    // Multi-line `@import` continuation: tsc accepts JSDoc imports whose
    // clause is broken across the `@import`, namespace, and `from` lines.
    // Without continuation merging the binder sees an empty rest on the
    // `@import` line and silently fails to register `types`, which then
    // surfaces as a TS2304 on every later `types.A` reference.
    let source = r#"
/**
 * @import
 * * as types
 * from "./types"
 */
class C {}
"#;
    let mut parser = ParserState::new("b.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let ns_sym_id = binder
        .file_locals
        .get("types")
        .expect("expected multi-line JSDoc namespace import alias to bind");
    let ns_sym = binder
        .symbols
        .get(ns_sym_id)
        .expect("expected symbol data for `types`");
    assert_ne!(ns_sym.flags & symbol_flags::ALIAS, 0);
    assert!(ns_sym.is_type_only);
    assert_eq!(ns_sym.import_module.as_deref(), Some("./types"));
    assert_eq!(ns_sym.import_name.as_deref(), Some("*"));
}

#[test]
fn jsdoc_import_tag_binds_string_literal_export_names() {
    let source = r#"
/**
 * @import { "a,b" as CommaName, "as" as AsName, "from" as FromName } from "./dep"
 */
class C {}
"#;
    let mut parser = ParserState::new("b.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    for (local_name, import_name) in [("CommaName", "a,b"), ("AsName", "as"), ("FromName", "from")]
    {
        let sym_id = binder
            .file_locals
            .get(local_name)
            .unwrap_or_else(|| panic!("expected JSDoc import alias {local_name}"));
        let symbol = binder
            .symbols
            .get(sym_id)
            .unwrap_or_else(|| panic!("expected symbol data for {local_name}"));
        assert_ne!(symbol.flags & symbol_flags::ALIAS, 0);
        assert!(symbol.is_type_only);
        assert_eq!(symbol.import_module.as_deref(), Some("./dep"));
        assert_eq!(symbol.import_name.as_deref(), Some(import_name));
    }

    assert!(
        !binder.file_locals.has("b\""),
        "quoted names containing commas must not be split as separate imports"
    );
}

#[test]
fn export_as_namespace_records_current_file_namespace_metadata() {
    let source = r"
export var x: number;
export interface Thing { n: typeof x }
export as namespace Foo;
";
    let mut parser = ParserState::new("foo.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let foo_sym_id = binder
        .file_locals
        .get("Foo")
        .expect("expected UMD namespace alias symbol");
    let foo_symbol = binder
        .symbols
        .get(foo_sym_id)
        .expect("expected symbol data for Foo");

    assert_ne!(foo_symbol.flags & symbol_flags::ALIAS, 0);
    assert!(foo_symbol.is_umd_export);
    assert_eq!(foo_symbol.import_module.as_deref(), Some("foo.d.ts"));
    assert_eq!(foo_symbol.import_name.as_deref(), Some("*"));
}

#[test]
fn namespace_reexport_does_not_create_local_binding() {
    let source = r"
export * as ns from './mod';
ns.a;
let ns = { a: 1 };
";
    let (binder, _parser) = parse_and_bind(source);

    let local_ns_id = binder
        .file_locals
        .get("ns")
        .expect("expected local let ns in file_locals");
    let local_ns = binder
        .symbols
        .get(local_ns_id)
        .expect("expected symbol data for local ns");
    assert_ne!(local_ns.flags & symbol_flags::BLOCK_SCOPED_VARIABLE, 0);
    assert_eq!(local_ns.flags & symbol_flags::ALIAS, 0);

    let export_ns_id = binder
        .module_exports
        .get("test.ts")
        .and_then(|exports| exports.get("ns"))
        .expect("expected namespace re-export in module_exports");
    let export_ns = binder
        .symbols
        .get(export_ns_id)
        .expect("expected symbol data for exported ns");

    assert_ne!(export_ns.flags & symbol_flags::ALIAS, 0);
    assert_eq!(export_ns.import_module.as_deref(), Some("./mod"));
    assert_eq!(export_ns.import_name.as_deref(), Some("*"));
    assert_ne!(export_ns_id, local_ns_id);
}

#[test]
fn type_alias_after_namespace_reexport_keeps_alias_partner() {
    let source = r"
export * as Foo from './mod';
export type Foo = { x: number };
";
    let (binder, _parser) = parse_and_bind(source);

    let foo_sym_id = binder
        .file_locals
        .get("Foo")
        .expect("expected exported type alias in file_locals");
    let foo_symbol = binder
        .symbols
        .get(foo_sym_id)
        .expect("expected symbol data for Foo");
    assert_ne!(foo_symbol.flags & symbol_flags::TYPE_ALIAS, 0);

    let alias_id = *binder
        .alias_partners
        .get(&foo_sym_id)
        .expect("expected namespace export alias partner for Foo");
    let alias_symbol = binder
        .symbols
        .get(alias_id)
        .expect("expected alias partner symbol data");
    assert_ne!(alias_symbol.flags & symbol_flags::ALIAS, 0);
    assert_eq!(alias_symbol.import_module.as_deref(), Some("./mod"));
    assert_eq!(alias_symbol.import_name.as_deref(), Some("*"));

    let export_foo_id = binder
        .module_exports
        .get("test.ts")
        .and_then(|exports| exports.get("Foo"))
        .expect("expected Foo export in module_exports");
    assert_eq!(export_foo_id, foo_sym_id);
}

#[test]
fn resolves_wildcard_type_only_reexports_with_provenance() {
    let mut binder = BinderState::new();

    let a_sym = binder.symbols.alloc(symbol_flags::CLASS, "A".to_string());
    let b_sym = binder.symbols.alloc(symbol_flags::CLASS, "B".to_string());

    let mut a_exports = SymbolTable::new();
    a_exports.set("A".to_string(), a_sym);
    a_exports.set("B".to_string(), b_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./a".to_string(), a_exports);

    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./b".to_string())
        .or_default()
        .push("./a".to_string());
    Arc::make_mut(&mut binder.wildcard_reexports_type_only)
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), true));

    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./c".to_string())
        .or_default()
        .push("./b".to_string());
    Arc::make_mut(&mut binder.wildcard_reexports_type_only)
        .entry("./c".to_string())
        .or_default()
        .push(("./b".to_string(), false));

    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./d".to_string())
        .or_default()
        .push("./a".to_string());
    Arc::make_mut(&mut binder.wildcard_reexports_type_only)
        .entry("./d".to_string())
        .or_default()
        .push(("./a".to_string(), false));

    let (resolved_a, is_type_only_a) = binder
        .resolve_import_with_reexports_type_only("./c", "A")
        .expect("expected type-only wildcard chain from './c' -> './b' -> './a'");
    assert_eq!(resolved_a, a_sym);
    assert!(is_type_only_a);

    let (resolved_b, is_type_only_b) = binder
        .resolve_import_with_reexports_type_only("./c", "B")
        .expect("expected type-only wildcard chain from './c' -> './b' -> './a'");
    assert_eq!(resolved_b, b_sym);
    assert!(is_type_only_b);

    let (resolved_a_value, is_type_only_value) = binder
        .resolve_import_with_reexports_type_only("./d", "A")
        .expect("expected value wildcard chain from './d' -> './a'");
    assert_eq!(resolved_a_value, a_sym);
    assert!(!is_type_only_value);
}

#[test]
fn global_augmentation_namespace_appears_in_file_locals() {
    // `declare global { namespace JSX { ... } }` inside a module declaration
    // should make the JSX namespace visible at the file level (in file_locals),
    // since `global` escapes the module scope.
    let source = r#"
declare module "react" {
    global {
        namespace JSX {
            interface IntrinsicElements {
                div: any;
                span: any;
            }
        }
    }
}
"#;
    let mut parser = ParserState::new("react.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // JSX namespace should be in file_locals because it's inside `declare global`
    let jsx_sym_id = binder
        .file_locals
        .get("JSX")
        .expect("expected JSX namespace in file_locals from global augmentation");
    let jsx_symbol = binder
        .symbols
        .get(jsx_sym_id)
        .expect("expected symbol data for JSX");

    // JSX should be a namespace/module
    assert!(
        jsx_symbol.flags & symbol_flags::NAMESPACE_MODULE != 0,
        "JSX should have NAMESPACE_MODULE flag"
    );

    // JSX should have IntrinsicElements in its exports
    let exports = jsx_symbol
        .exports
        .as_ref()
        .expect("expected JSX to have exports");
    assert!(
        exports.has("IntrinsicElements"),
        "expected IntrinsicElements in JSX exports"
    );

    // JSX should also be tracked as a global augmentation
    assert!(
        binder.global_augmentations.contains_key("JSX"),
        "expected JSX in global_augmentations"
    );
}

#[test]
fn ambient_module_export_import_populates_module_exports() {
    let source = r#"
declare module "a" {
    export type T = number;
}
declare module "b" {
    export import a = require("a");
    export const x: a.T;
}
"#;
    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let b_sym_id = binder
        .file_locals
        .get("b")
        .expect("expected ambient module symbol for b");
    let b_symbol = binder
        .symbols
        .get(b_sym_id)
        .expect("expected symbol data for module b");
    let exports = b_symbol
        .exports
        .as_ref()
        .expect("expected exports table for module b");
    let a_sym_id = exports
        .get("a")
        .expect("expected export-import alias a in module b exports");
    let a_symbol = binder
        .symbols
        .get(a_sym_id)
        .expect("expected symbol data for alias a");

    assert_ne!(a_symbol.flags & symbol_flags::ALIAS, 0);
    assert_eq!(a_symbol.import_module.as_deref(), Some("a"));

    let module_exports = binder
        .module_exports
        .get("b")
        .expect("expected cached module exports for module b");
    assert!(
        module_exports.has("a"),
        "expected export-import alias a in cached module exports"
    );
}

#[test]
fn export_equals_default_property_does_not_create_default_module_export() {
    let source = r#"
var x = {
    default: 42,
    answer: 1
};

export = x;
"#;
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let module_exports = binder
        .module_exports
        .get("test.ts")
        .expect("expected cached module exports for file");
    assert!(
        module_exports.has("export="),
        "expected explicit export= target to stay cached"
    );
    assert!(
        !module_exports.has("default"),
        "default-valued export= members must not masquerade as real default exports"
    );
}

#[test]
fn export_equals_class_static_default_not_in_file_locals() {
    // A class with `static default: "foo"` exported via `export = Point`
    // must NOT put the `default` static member into file_locals as `"default"`.
    // Otherwise default-import resolution picks up the static member instead
    // of the class constructor.
    let source = r#"
declare class Point {
    x: number;
    y: number;
    constructor(x: number, y: number);
    static default: "foo";
}
export = Point;
"#;
    let mut parser = ParserState::new("point.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // file_locals should have "export=" but NOT "default"
    assert!(
        binder.file_locals.has("export="),
        "expected export= in file_locals"
    );
    assert!(
        !binder.file_locals.has("default"),
        "static member named 'default' must not leak into file_locals from export= target"
    );

    // module_exports should also not have "default"
    let module_exports = binder
        .module_exports
        .get("point.d.ts")
        .expect("expected cached module exports for file");
    assert!(
        module_exports.has("export="),
        "expected export= in module_exports"
    );
    assert!(
        !module_exports.has("default"),
        "static member named 'default' must not appear in module_exports"
    );
}

#[test]
fn export_equals_class_instance_members_not_in_module_exports() {
    // For `class D { bar: string; } export = D;`, the class's INSTANCE members
    // (`bar`, accessible only via `new D().bar`) must NOT appear in
    // `module_exports`. Otherwise an `import x = require()` of this module
    // synthesizes a phantom `{ bar }` namespace surface and computes
    // `typeof D & { bar }` as the import type instead of tsc's plain
    // `typeof D`. That in turn flips diagnostics for assignment failures
    // from TS2741 ("Property X is missing") to TS2322 ("Type … is not
    // assignable"), as seen in the `typeofAmbientExternalModules`
    // conformance test. tsc only exposes static members and namespace
    // augmentations (which live in `.exports`) at the module level.
    let source = r#"
class D { bar: string; }
export = D;
"#;
    let mut parser = ParserState::new("class_export_equals.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let module_exports = binder
        .module_exports
        .get("class_export_equals.ts")
        .expect("expected cached module exports for file");
    assert!(
        module_exports.has("export="),
        "expected explicit export= target to be cached"
    );
    assert!(
        !module_exports.has("bar"),
        "instance member `bar` from `class D` must not leak into module exports \
         via export = D; tsc treats `import x = require()` as `typeof D`, not \
         `typeof D & {{ bar }}`",
    );
}

#[test]
fn export_equals_qualified_namespace_target_populates_cached_members() {
    let source = r#"
declare module "nestNamespaceModule" {
    namespace a1.a2 {
        class d {}
    }
    namespace a1.a2.n3 {
        class c {}
    }
    export = a1.a2;
}
"#;
    let mut parser = ParserState::new("ambient.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let exports = binder
        .module_exports
        .get("nestNamespaceModule")
        .expect("expected cached module exports for ambient module");
    assert!(exports.has("export="), "expected export= entry");
    assert!(exports.has("d"), "expected export= target member d");
    assert!(exports.has("n3"), "expected export= target member n3");
}

#[test]
fn export_equals_import_equals_qualified_target_populates_cached_members() {
    let source = r#"
declare module "renameModule" {
    namespace a.b {
        class c {}
    }
    import d = a.b;
    export = d;
}
"#;
    let mut parser = ParserState::new("ambient2.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let exports = binder
        .module_exports
        .get("renameModule")
        .expect("expected cached module exports for ambient module");
    assert!(exports.has("export="), "expected export= entry");
    assert!(exports.has("c"), "expected export= target member c");
}

#[test]
fn iife_no_flow_start_node() {
    // For a non-async, non-generator IIFE, the binder should NOT create a
    // FlowStart node for the function body. This means the IIFE body runs
    // in the outer flow context.
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
(function() {
    x = 1;
})();
";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Count START nodes. There should be exactly 1 (the file-level start),
    // NOT 2 (file + IIFE body).
    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 1,
        "IIFE body should not create a FlowStart node"
    );
}

#[test]
fn non_iife_function_gets_flow_start_node() {
    // A regular (non-IIFE) function expression SHOULD get a FlowStart node.
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
let f = function() {
    x = 1;
};
";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Count START nodes. Should be 2: one for the file, one for the function body.
    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 2,
        "non-IIFE function should create a FlowStart node"
    );
}

#[test]
fn async_iife_gets_flow_start_node() {
    // An async IIFE should still get a FlowStart node (not treated as inline).
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
(async function() {
    x = 1;
})();
";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 2,
        "async IIFE should still create a FlowStart node"
    );
}

#[test]
fn generator_iife_gets_flow_start_node() {
    // A generator IIFE should still get a FlowStart node (not treated as inline).
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
(function*() {
    x = 1;
})();
";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 2,
        "generator IIFE should still create a FlowStart node"
    );
}

#[test]
fn arrow_iife_no_flow_start_node() {
    // Arrow function IIFE should also be treated as inline (no FlowStart).
    use crate::flow::flow_flags;

    let source = r"
let x: number | undefined;
(() => {
    x = 1;
})();
";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let start_count = (0..binder.flow_nodes.len())
        .filter(|&i| {
            binder
                .flow_nodes
                .get(crate::flow::FlowNodeId(i as u32))
                .is_some_and(|n| n.has_any_flags(flow_flags::START))
        })
        .count();
    assert_eq!(
        start_count, 1,
        "arrow IIFE should not create a FlowStart node"
    );
}
