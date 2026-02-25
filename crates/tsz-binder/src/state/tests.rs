use super::BinderState;
use crate::{SymbolTable, symbol_flags};
use tsz_parser::parser::ParserState;

#[test]
fn test_namespace_exports_exclude_non_exported_members() {
    let source = r"
namespace M {
    export class A {}
    class B {}
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
fn records_import_metadata_for_exported_reexports() {
    let source = r"
export { A, B as C } from './a';
export type { D as E } from './b';
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
fn resolves_wildcard_type_only_reexports_with_provenance() {
    let mut binder = BinderState::new();

    let a_sym = binder.symbols.alloc(symbol_flags::CLASS, "A".to_string());
    let b_sym = binder.symbols.alloc(symbol_flags::CLASS, "B".to_string());

    let mut a_exports = SymbolTable::new();
    a_exports.set("A".to_string(), a_sym);
    a_exports.set("B".to_string(), b_sym);
    binder.module_exports.insert("./a".to_string(), a_exports);

    binder
        .wildcard_reexports
        .entry("./b".to_string())
        .or_default()
        .push("./a".to_string());
    binder
        .wildcard_reexports_type_only
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), true));

    binder
        .wildcard_reexports
        .entry("./c".to_string())
        .or_default()
        .push("./b".to_string());
    binder
        .wildcard_reexports_type_only
        .entry("./c".to_string())
        .or_default()
        .push(("./b".to_string(), false));

    binder
        .wildcard_reexports
        .entry("./d".to_string())
        .or_default()
        .push("./a".to_string());
    binder
        .wildcard_reexports_type_only
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
