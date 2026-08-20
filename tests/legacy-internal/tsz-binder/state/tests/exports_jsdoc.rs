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

/// Duplicate `export { X } from "mod"` re-export specifiers must end up sharing
/// a single symbol with multiple declarations so the checker's duplicate-
/// identifier pass can emit TS2300 on each spec node.
///
/// Structural rule: two re-export specs that bind the same *exported* name —
/// independent of source module, original name, or type-only-ness — collide.
#[test]
fn duplicate_reexport_specifiers_share_one_symbol_with_multiple_declarations() {
    let source = r"
export { Foo } from './a';
export type { Foo } from './a';
";
    let (binder, _parser) = parse_and_bind(source);

    let foo_sym_id = binder
        .file_locals
        .get("Foo")
        .expect("expected re-export alias for Foo in file_locals");
    let foo_sym = binder
        .symbols
        .get(foo_sym_id)
        .expect("expected symbol data for Foo");

    assert_ne!(foo_sym.flags & symbol_flags::ALIAS, 0);
    assert_eq!(
        foo_sym.declarations.len(),
        2,
        "expected two spec declarations on the shared re-export symbol, got: {:?}",
        foo_sym.declarations
    );

    // The visible binding keeps the first-bound spec's metadata so downstream
    // import resolution still routes through the original module specifier.
    assert_eq!(foo_sym.import_module(), Some("./a"));
    assert_eq!(foo_sym.import_name(), Some("Foo"));
    assert!(
        !foo_sym.is_type_only,
        "first-bound spec was value-only — the merge must not escalate to type-only"
    );
}

/// Same merging behavior when the duplicate uses a different original name
/// (`Foo as X` + `Bar as X`) — the EXPORTED name is the keying axis, not the
/// source identifier.
#[test]
fn duplicate_reexport_specifiers_with_distinct_originals_share_one_symbol() {
    let source = r"
export { Foo as X } from './a';
export { Bar as X } from './a';
";
    let (binder, _parser) = parse_and_bind(source);

    let x_sym_id = binder
        .file_locals
        .get("X")
        .expect("expected re-export alias for X in file_locals");
    let x_sym = binder
        .symbols
        .get(x_sym_id)
        .expect("expected symbol data for X");

    assert_eq!(
        x_sym.declarations.len(),
        2,
        "expected two spec declarations on the shared re-export symbol, got: {:?}",
        x_sym.declarations
    );
    assert_eq!(x_sym.import_name(), Some("Foo"));
}

/// Aliased re-exports with DISTINCT exported names must remain separate
/// symbols (no merge). Guards the literal repro from issue #11334.
#[test]
fn distinct_exported_names_remain_separate_re_export_symbols() {
    let source = r"
export * from './a';
export { Foo as Bar } from './a';
export type { Foo } from './a';
";
    let (binder, _parser) = parse_and_bind(source);

    let bar = binder.file_locals.get("Bar").expect("expected Bar symbol");
    let foo = binder.file_locals.get("Foo").expect("expected Foo symbol");
    assert_ne!(
        bar, foo,
        "Bar and Foo must remain distinct re-export symbols"
    );

    let bar_sym = binder.symbols.get(bar).expect("Bar symbol data");
    let foo_sym = binder.symbols.get(foo).expect("Foo symbol data");
    assert_eq!(bar_sym.declarations.len(), 1);
    assert_eq!(foo_sym.declarations.len(), 1);
    assert_eq!(bar_sym.import_name(), Some("Foo"));
    assert_eq!(foo_sym.import_name(), Some("Foo"));
    assert!(foo_sym.is_type_only);
    assert!(!bar_sym.is_type_only);
}

/// `export { type X, Y } from "./mod"` (per-specifier `type` modifier on a
/// re-export) must mark only `X` as type-only — independent of whether the
/// enclosing declaration is `export {}` or `export type {}`. Before the fix the
/// re-export branch hard-coded `sym.is_type_only = export_type_only`, dropping
/// the per-specifier flag and letting downstream value imports through `X`
/// silently succeed (regression in TS1361/TS1362 attribution).
#[test]
fn per_specifier_type_modifier_on_reexport_marks_alias_type_only() {
    let source = r"
export { type Foo, Bar } from './mod';
export { type Renamed as Aliased } from './mod';
";
    let (binder, _parser) = parse_and_bind(source);
    let lookup = |name: &str| {
        let id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected {name} alias in file_locals"));
        binder
            .symbols
            .get(id)
            .unwrap_or_else(|| panic!("expected symbol data for {name}"))
    };

    let foo_sym = lookup("Foo");
    assert!(
        foo_sym.is_type_only,
        "per-specifier `type Foo` must mark the re-export alias type-only"
    );
    assert_eq!(foo_sym.import_module(), Some("./mod"));
    assert_eq!(foo_sym.import_name(), Some("Foo"));

    let bar_sym = lookup("Bar");
    assert!(
        !bar_sym.is_type_only,
        "sibling value spec `Bar` in the same export clause must stay value-bearing"
    );

    let aliased_sym = lookup("Aliased");
    assert!(
        aliased_sym.is_type_only,
        "renamed per-spec `type Renamed as Aliased` must also be type-only"
    );
    assert_eq!(aliased_sym.import_name(), Some("Renamed"));
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
    assert_eq!(a_symbol.import_module(), Some("./a"));
    assert_eq!(a_symbol.import_name(), Some("A"));
    assert!(!a_symbol.is_type_only);

    let c_sym_id = binder
        .file_locals
        .get("C")
        .expect("expected re-exported symbol C");
    let c_symbol = binder
        .symbols
        .get(c_sym_id)
        .expect("expected symbol data for C");
    assert_eq!(c_symbol.import_module(), Some("./a"));
    assert_eq!(c_symbol.import_name(), Some("B"));
    assert!(!c_symbol.is_type_only);

    let e_sym_id = binder
        .file_locals
        .get("E")
        .expect("expected type-only re-exported symbol E");
    let e_symbol = binder
        .symbols
        .get(e_sym_id)
        .expect("expected symbol data for E");
    assert_eq!(e_symbol.import_module(), Some("./b"));
    assert_eq!(e_symbol.import_name(), Some("D"));
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
    assert_eq!(ns_sym.import_module(), Some("./a"));
    assert_eq!(ns_sym.import_name(), Some("*"));

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
    assert_eq!(renamed_i_sym.import_module(), Some("./a"));
    assert_eq!(renamed_i_sym.import_name(), Some("I"));

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
    assert_eq!(default_sym.import_module(), Some("./a"));
    assert_eq!(default_sym.import_name(), Some("default"));

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
    assert_eq!(ns_sym.import_module(), Some("./types"));
    assert_eq!(ns_sym.import_name(), Some("*"));
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
        assert_eq!(symbol.import_module(), Some("./dep"));
        assert_eq!(symbol.import_name(), Some(import_name));
    }

    assert!(
        !binder.file_locals.has("b\""),
        "quoted names containing commas must not be split as separate imports"
    );
}

#[test]
fn jsdoc_import_tag_records_resolution_mode_attribute() {
    // A JSDoc `@import` carrying a `with { "resolution-mode": ... }` attribute
    // must still bind the alias (it was previously dropped entirely) and record
    // the override so the checker resolves the member through the requested
    // package condition. Vary the binder name so the behavior is structural,
    // not keyed to a specific identifier.
    let source = r#"
/**
 * @import { Esm as ImpAlias } from "pkg" with { "resolution-mode": "import" }
 * @import { Cjs as ReqAlias } from "pkg" with { 'resolution-mode': 'require' }
 * @import { Plain } from "pkg"
 */
class C {}
"#;
    let mut parser = ParserState::new("b.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let lookup = |name: &str| {
        let sym_id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected JSDoc import alias {name}"));
        binder
            .symbols
            .get(sym_id)
            .unwrap_or_else(|| panic!("expected symbol data for {name}"))
    };

    let imp = lookup("ImpAlias");
    assert_ne!(imp.flags & symbol_flags::ALIAS, 0);
    assert!(imp.is_type_only);
    assert_eq!(imp.import_module(), Some("pkg"));
    assert_eq!(imp.import_name(), Some("Esm"));
    assert_eq!(
        imp.import_resolution_mode(),
        Some(tsz_common::ImportResolutionMode::Import)
    );

    let req = lookup("ReqAlias");
    assert_eq!(req.import_name(), Some("Cjs"));
    assert_eq!(
        req.import_resolution_mode(),
        Some(tsz_common::ImportResolutionMode::Require)
    );

    // No attribute clause → no override.
    let plain = lookup("Plain");
    assert_eq!(plain.import_resolution_mode(), None);
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
    assert_eq!(foo_symbol.import_module(), Some("foo.d.ts"));
    assert_eq!(foo_symbol.import_name(), Some("*"));
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
    assert_eq!(export_ns.import_module(), Some("./mod"));
    assert_eq!(export_ns.import_name(), Some("*"));
    assert_ne!(export_ns_id, local_ns_id);
}

#[test]
fn renamed_default_export_updates_synthetic_default_slot() {
    let source = r#"
export default interface zzz {
    x: string;
}
import zzz from "./b";
export { zzz as default };
"#;
    let (binder, _parser) = parse_and_bind(source);

    let default_id = binder
        .file_locals
        .get("default")
        .expect("expected synthetic default export slot");
    let default_symbol = binder
        .symbols
        .get(default_id)
        .expect("expected default symbol data");

    assert_ne!(default_symbol.flags & symbol_flags::ALIAS, 0);
    assert!(!default_symbol.is_type_only);
    assert_eq!(default_symbol.import_module(), Some("./b"));
    assert_eq!(default_symbol.import_name(), Some("default"));
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
    assert_eq!(alias_symbol.import_module(), Some("./mod"));
    assert_eq!(alias_symbol.import_name(), Some("*"));

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
        .push(("./a".to_string(), true));

    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./c".to_string())
        .or_default()
        .push(("./b".to_string(), false));

    Arc::make_mut(&mut binder.wildcard_reexports)
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

/// When two wildcard sources export the same name, one as a pure `TYPE_ALIAS`
/// and one as a value, the value symbol must be returned.
#[test]
fn value_export_preferred_over_type_alias_in_wildcard_sources() {
    let mut binder = BinderState::new();

    let type_alias_sym = binder
        .symbols
        .alloc(symbol_flags::TYPE_ALIAS, "Config".to_string());
    let mut types_exports = SymbolTable::new();
    types_exports.set("Config".to_string(), type_alias_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./types".to_string(), types_exports);

    let value_sym = binder
        .symbols
        .alloc(symbol_flags::VARIABLE, "Config".to_string());
    let mut values_exports = SymbolTable::new();
    values_exports.set("Config".to_string(), value_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./values".to_string(), values_exports);

    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./barrel".to_string())
        .or_default()
        .push(("./types".to_string(), false));
    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./barrel".to_string())
        .or_default()
        .push(("./values".to_string(), false));

    let (resolved, is_type_only) = binder
        .resolve_import_with_reexports_type_only("./barrel", "Config")
        .expect("expected Config to be resolved from barrel");

    assert_eq!(
        resolved, value_sym,
        "should resolve to value symbol, not type alias, when both are exported"
    );
    assert!(!is_type_only, "value export must not be marked type-only");
}

/// A value-bearing declaration reached through `export type *` is type-only for
/// that path, so it must not shadow a later value wildcard source.
#[test]
fn value_export_preferred_over_type_only_wildcard_value_source() {
    let mut binder = BinderState::new();

    let type_only_value_sym = binder
        .symbols
        .alloc(symbol_flags::CLASS, "Config".to_string());
    let mut classes_exports = SymbolTable::new();
    classes_exports.set("Config".to_string(), type_only_value_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./classes".to_string(), classes_exports);

    let value_sym = binder
        .symbols
        .alloc(symbol_flags::VARIABLE, "Config".to_string());
    let mut values_exports = SymbolTable::new();
    values_exports.set("Config".to_string(), value_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./values".to_string(), values_exports);

    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./barrel".to_string())
        .or_default()
        .push(("./classes".to_string(), true));
    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./barrel".to_string())
        .or_default()
        .push(("./values".to_string(), false));

    let (resolved, is_type_only) = binder
        .resolve_import_with_reexports_type_only("./barrel", "Config")
        .expect("expected Config to be resolved from barrel");

    assert_eq!(
        resolved, value_sym,
        "value context should skip the type-only wildcard path and use the later value export"
    );
    assert!(!is_type_only, "value export must not be marked type-only");
}

/// When only a `TYPE_ALIAS` is exported, the type-alias symbol is still the
/// resolution result for type-position access.
#[test]
fn type_alias_returned_when_no_value_in_wildcard_sources() {
    let mut binder = BinderState::new();

    let type_alias_sym = binder
        .symbols
        .alloc(symbol_flags::TYPE_ALIAS, "Config".to_string());
    let mut types_exports = SymbolTable::new();
    types_exports.set("Config".to_string(), type_alias_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./types".to_string(), types_exports);

    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./barrel".to_string())
        .or_default()
        .push(("./types".to_string(), false));

    let (resolved, _) = binder
        .resolve_import_with_reexports_type_only("./barrel", "Config")
        .expect("expected Config to be resolved from barrel");

    assert_eq!(
        resolved, type_alias_sym,
        "should resolve to type alias when no value export exists"
    );
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
    assert_eq!(a_symbol.import_module(), Some("a"));

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

// ----- namespace + `export * as N` coexistence (issue #11331) ---------------

/// Asserts that after binding `source`, `module_exports["test.ts"][ns_name]`
/// is a MODULE symbol and `alias_partners[MODULE]` points to an ALIAS with
/// `import_module = "./mod"` and `import_name = "*"`.
fn assert_ns_module_alias_partner(source: &str, ns_name: &str) {
    let (binder, _parser) = parse_and_bind(source);

    let ns_exported_id = binder
        .module_exports
        .get("test.ts")
        .and_then(|e| e.get(ns_name))
        .unwrap_or_else(|| panic!("expected {ns_name} in module_exports"));
    let ns_sym = binder
        .symbols
        .get(ns_exported_id)
        .unwrap_or_else(|| panic!("expected symbol data for exported {ns_name}"));
    assert_ne!(
        ns_sym.flags & symbol_flags::MODULE,
        0,
        "exported {ns_name} should be the namespace MODULE symbol"
    );

    let alias_id = binder
        .alias_partners
        .get(&ns_exported_id)
        .copied()
        .unwrap_or_else(|| panic!("expected alias_partners entry for {ns_name} MODULE"));
    let alias_sym = binder
        .symbols
        .get(alias_id)
        .unwrap_or_else(|| panic!("expected alias partner symbol data for {ns_name}"));
    assert_ne!(alias_sym.flags & symbol_flags::ALIAS, 0);
    assert_eq!(alias_sym.import_module(), Some("./mod"));
    assert_eq!(alias_sym.import_name(), Some("*"));
}

/// `export * as N from './mod'` followed by `namespace N { ... }`:
/// `module_exports` should hold the MODULE symbol and `alias_partners` should link
/// MODULE → ALIAS so the checker can resolve members from both the local
/// namespace and the re-exported source module.
#[test]
fn namespace_reexport_alias_before_local_namespace_links_alias_partners() {
    assert_ns_module_alias_partner(
        r"
export * as Ns from './mod';
export namespace Ns { export const x = 1; }
",
        "Ns",
    );
}

/// `namespace N { ... }` followed by `export * as N from './mod'` (reversed
/// source order): same invariants must hold — the fix must be order-insensitive.
#[test]
fn namespace_reexport_local_namespace_before_alias_links_alias_partners() {
    assert_ns_module_alias_partner(
        r"
export namespace Ns { export const x = 1; }
export * as Ns from './mod';
",
        "Ns",
    );
}

/// Verify the fix is name-agnostic: use `K` and `M` as namespace names.
#[test]
fn namespace_reexport_alias_partner_is_name_agnostic() {
    for (source, name) in [
        (
            r"
export * as K from './mod';
export namespace K { export const y = 2; }
",
            "K",
        ),
        (
            r"
export namespace M { export const z = 3; }
export * as M from './mod';
",
            "M",
        ),
    ] {
        assert_ns_module_alias_partner(source, name);
    }
}

/// A plain `export * as N from './mod'` without a companion `namespace N`
/// must NOT create a spurious `alias_partners` entry — only the ALIAS should
/// be in `module_exports`, no MODULE to pair it with.
#[test]
fn namespace_reexport_without_local_namespace_has_no_alias_partner() {
    let source = r"
export * as Ns from './mod';
";
    let (binder, _parser) = parse_and_bind(source);

    let ns_id = binder
        .module_exports
        .get("test.ts")
        .and_then(|e| e.get("Ns"))
        .expect("expected Ns in module_exports");
    let ns_sym = binder
        .symbols
        .get(ns_id)
        .expect("expected symbol data for Ns");
    // Should be ALIAS, not MODULE
    assert_ne!(ns_sym.flags & symbol_flags::ALIAS, 0);
    assert_eq!(ns_sym.import_module(), Some("./mod"));
    // No alias_partners entry for a lone ALIAS
    assert!(
        !binder.alias_partners.contains_key(&ns_id),
        "lone export-* alias should not have an alias_partners entry"
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

/// Build a binder with a named re-export chain `entry -> mid -> leaf` whose
/// `export_name` ultimately resolves to a single leaf symbol. The chain is
/// constructed structurally (no source text) so the names of the modules and
/// the export can be varied by the caller — the resolution rule must not depend
/// on any specific spelling.
fn build_named_reexport_chain(
    leaf_module: &str,
    mid_module: &str,
    entry_module: &str,
    export_name: &str,
) -> (BinderState, SymbolId) {
    let mut binder = BinderState::new();
    let leaf_sym = binder
        .symbols
        .alloc(symbol_flags::CLASS, export_name.to_string());

    let mut leaf_exports = SymbolTable::new();
    leaf_exports.set(export_name.to_string(), leaf_sym);
    Arc::make_mut(&mut binder.module_exports).insert(leaf_module.to_string(), leaf_exports);

    // mid re-exports `export { export_name } from leaf`
    Arc::make_mut(&mut binder.reexports)
        .entry(mid_module.to_string())
        .or_default()
        .insert(export_name.to_string(), (leaf_module.to_string(), None));
    // entry re-exports `export { export_name } from mid`
    Arc::make_mut(&mut binder.reexports)
        .entry(entry_module.to_string())
        .or_default()
        .insert(export_name.to_string(), (mid_module.to_string(), None));

    (binder, leaf_sym)
}

#[test]
fn type_only_reexport_resolution_is_cached_and_stable() {
    let (binder, leaf_sym) = build_named_reexport_chain("./leaf", "./mid", "./entry", "Widget");

    // Cold lookup walks the chain and resolves to the leaf symbol.
    let first = binder.resolve_import_with_reexports_type_only("./entry", "Widget");
    assert_eq!(first, Some((leaf_sym, false)));

    // The result is now memoized for this (module, export) pair.
    assert_eq!(
        binder
            .resolved_export_type_only_cache
            .read()
            .expect("cache lock")
            .get("./entry")
            .and_then(|module_exports| module_exports.get("Widget"))
            .copied(),
        Some(Some((leaf_sym, false))),
        "type-only resolution should be memoized per (module, export)"
    );

    // Warm lookup returns the identical result.
    let second = binder.resolve_import_with_reexports_type_only("./entry", "Widget");
    assert_eq!(second, first);
}

#[test]
fn type_only_reexport_cache_is_name_agnostic() {
    // Same structural chain, completely different spellings. The cached result
    // must still be the single leaf symbol — proving the rule is structural, not
    // keyed on any particular identifier.
    let (binder, leaf_sym) =
        build_named_reexport_chain("./pkg-z/lib", "./pkg-y/index", "./pkg-x/index", "Zeta");

    let resolved = binder.resolve_import_with_reexports_type_only("./pkg-x/index", "Zeta");
    assert_eq!(resolved, Some((leaf_sym, false)));
    // Repeat lookup served from cache is identical.
    assert_eq!(
        binder.resolve_import_with_reexports_type_only("./pkg-x/index", "Zeta"),
        resolved
    );
}

#[test]
fn type_only_reexport_cache_missing_export_is_cached_as_none() {
    let (binder, _leaf) = build_named_reexport_chain("./leaf", "./mid", "./entry", "Widget");

    // An export that does not exist anywhere in the chain resolves to None and
    // the negative result is memoized (so the chain is not re-walked).
    assert_eq!(
        binder.resolve_import_with_reexports_type_only("./entry", "DoesNotExist"),
        None
    );
    assert_eq!(
        binder
            .resolved_export_type_only_cache
            .read()
            .expect("cache lock")
            .get("./entry")
            .and_then(|module_exports| module_exports.get("DoesNotExist"))
            .copied(),
        Some(None),
        "negative type-only results must also be memoized"
    );
}

#[test]
fn clear_resolution_caches_drops_type_only_cache() {
    let (mut binder, leaf_sym) = build_named_reexport_chain("./leaf", "./mid", "./entry", "Widget");

    assert_eq!(
        binder.resolve_import_with_reexports_type_only("./entry", "Widget"),
        Some((leaf_sym, false))
    );
    assert!(
        !binder
            .resolved_export_type_only_cache
            .read()
            .expect("cache lock")
            .is_empty(),
        "cache should be populated after a lookup"
    );

    binder.clear_resolution_caches();

    assert!(
        binder
            .resolved_export_type_only_cache
            .read()
            .expect("cache lock")
            .is_empty(),
        "clear_resolution_caches must drop the type-only re-export cache so a \
         symbol-id remap cannot serve a stale resolution"
    );

    // Resolution still works after the cache is cleared.
    assert_eq!(
        binder.resolve_import_with_reexports_type_only("./entry", "Widget"),
        Some((leaf_sym, false))
    );
}

#[test]
fn type_alias_chain_through_value_wildcard_is_type_only() {
    // `export type * from './a'` in ./b (type-only wildcard)
    // `export * from './b'` in ./c (value wildcard — but source is type-only)
    // Symbols reaching ./c from ./a through ./b should remain type-only because
    // the type-only flag is transitively preserved when traversing chains.
    let mut binder = BinderState::new();

    let x_sym = binder.symbols.alloc(symbol_flags::CLASS, "X".to_string());
    let mut a_exports = SymbolTable::new();
    a_exports.set("X".to_string(), x_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./a".to_string(), a_exports);

    // ./b re-exports ./a via `export type *` (type-only)
    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), true));

    // ./c re-exports ./b via `export *` (value wildcard, not type-only)
    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./c".to_string())
        .or_default()
        .push(("./b".to_string(), false));

    let (resolved, is_type_only) = binder
        .resolve_import_with_reexports_type_only("./c", "X")
        .expect("expected X to resolve through ./b -> ./a");
    assert_eq!(resolved, x_sym);
    assert!(
        is_type_only,
        "X should be type-only because ./b uses `export type *`"
    );
}

#[test]
fn explicit_type_only_wildcard_marks_exports_type_only() {
    // `export type * from './a'` should mark all its exports as type-only,
    // regardless of the name chosen for symbols or the order of declarations.
    let mut binder = BinderState::new();

    let foo_sym = binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Foo".to_string());
    let bar_sym = binder
        .symbols
        .alloc(symbol_flags::TYPE_ALIAS, "Bar".to_string());
    let mut a_exports = SymbolTable::new();
    a_exports.set("Foo".to_string(), foo_sym);
    a_exports.set("Bar".to_string(), bar_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./a".to_string(), a_exports);

    // ./b uses `export type * from './a'` — is_type_only = true
    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), true));

    let (resolved_foo, foo_type_only) = binder
        .resolve_import_with_reexports_type_only("./b", "Foo")
        .expect("Foo should resolve via type-only wildcard");
    assert_eq!(resolved_foo, foo_sym);
    assert!(foo_type_only, "Foo should be type-only via `export type *`");

    let (resolved_bar, bar_type_only) = binder
        .resolve_import_with_reexports_type_only("./b", "Bar")
        .expect("Bar should resolve via type-only wildcard");
    assert_eq!(resolved_bar, bar_sym);
    assert!(bar_type_only, "Bar should be type-only via `export type *`");
}

#[test]
fn value_wildcard_overrides_type_only_wildcard_for_same_source() {
    // When a source module appears in both `export type *` and `export *`,
    // the merge reducer produces a single de-duplicated entry with
    // is_type_only=false (value re-export wins). This test verifies that
    // a binder with that already-reduced state resolves symbols as values.
    let mut binder = BinderState::new();

    let val_sym = binder.symbols.alloc(symbol_flags::CLASS, "Val".to_string());
    let mut a_exports = SymbolTable::new();
    a_exports.set("Val".to_string(), val_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./a".to_string(), a_exports);

    // After merging `export type * from './a'` and `export * from './a'`,
    // the reducer leaves a single entry with is_type_only=false.
    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), false)); // value wins after deduplication

    let (resolved, is_type_only) = binder
        .resolve_import_with_reexports_type_only("./b", "Val")
        .expect("Val should resolve via value wildcard");
    assert_eq!(resolved, val_sym);
    assert!(
        !is_type_only,
        "Val should not be type-only because the value `export *` won deduplication"
    );
}
