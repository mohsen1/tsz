use super::*;
use crate::{SymbolArena, symbol_flags};

#[test]
fn test_lib_file_from_source() {
    let content = r"
declare var console: { log(msg: string): void };
declare class Array<T> { length: number; }
";
    let lib = LibFile::from_source("test-lib.d.ts".to_string(), content.to_string());
    assert!(lib.file_locals().has("console"));
    assert!(lib.file_locals().has("Array"));
}

#[test]
fn test_merge_lib_symbols() {
    let mut arena = SymbolArena::new();
    let object_id = arena.alloc(symbol_flags::VALUE, "Object".to_string());
    let function_id = arena.alloc(symbol_flags::VALUE, "Function".to_string());
    let console_id = arena.alloc(symbol_flags::VALUE, "console".to_string());

    let mut lib_file_locals = SymbolTable::new();
    lib_file_locals.set("Object".to_string(), object_id);
    lib_file_locals.set("Function".to_string(), function_id);
    lib_file_locals.set("console".to_string(), console_id);

    let lib_binder =
        BinderState::from_bound_state(arena, lib_file_locals, rustc_hash::FxHashMap::default());
    let lib = Arc::new(LibFile::new(
        "lib.d.ts".to_string(),
        Arc::new(NodeArena::new()),
        Arc::new(lib_binder),
    ));

    let mut user_arena = SymbolArena::new();
    let user_object_id = user_arena.alloc(symbol_flags::VALUE, "Object".to_string());
    let mut user_file_locals = SymbolTable::new();
    user_file_locals.set("Object".to_string(), user_object_id);
    let mut user_binder = BinderState::from_bound_state(
        user_arena,
        user_file_locals,
        rustc_hash::FxHashMap::default(),
    );

    user_binder.merge_lib_symbols(&[lib]);

    assert_eq!(user_binder.file_locals.get("Object"), Some(user_object_id));
    assert!(user_binder.file_locals.has("Function"));
    assert!(user_binder.file_locals.has("console"));

    assert_ne!(user_binder.file_locals.get("Function"), Some(function_id));
    assert_ne!(user_binder.file_locals.get("console"), Some(console_id));
}

#[test]
fn test_is_es2015_plus_type() {
    assert!(is_es2015_plus_type("Promise"));
    assert!(is_es2015_plus_type("Map"));
    assert!(is_es2015_plus_type("Set"));
    assert!(is_es2015_plus_type("Symbol"));
    assert!(is_es2015_plus_type("BigInt"));
    assert!(!is_es2015_plus_type("Object"));
    assert!(!is_es2015_plus_type("Array"));
    assert!(!is_es2015_plus_type("Function"));
    assert!(!is_es2015_plus_type("Date"));
    assert!(!is_es2015_plus_type("RegExp"));
}
