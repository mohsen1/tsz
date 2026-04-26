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

    let lib_binder = BinderState::from_bound_state(
        arena,
        lib_file_locals,
        Arc::new(rustc_hash::FxHashMap::default()),
    );
    let lib = Arc::new(LibFile::new(
        "lib.d.ts".to_string(),
        Arc::new(NodeArena::new()),
        Arc::new(lib_binder),
        tsz_parser::NodeIndex(0),
    ));

    let mut user_arena = SymbolArena::new();
    let user_object_id = user_arena.alloc(symbol_flags::VALUE, "Object".to_string());
    let mut user_file_locals = SymbolTable::new();
    user_file_locals.set("Object".to_string(), user_object_id);
    let mut user_binder = BinderState::from_bound_state(
        user_arena,
        user_file_locals,
        Arc::new(rustc_hash::FxHashMap::default()),
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

// ---------------------------------------------------------------------------
// LibLoader cache behavior
// ---------------------------------------------------------------------------

/// Newly constructed loader starts empty and has no entries.
#[test]
fn lib_loader_new_has_empty_cache() {
    let loader = LibLoader::new(PathBuf::from("/nonexistent/lib/dir"));
    assert_eq!(loader.cache_size(), 0);
}

/// `load_lib` returns `None` when the directory does not exist on disk.
#[test]
fn lib_loader_load_lib_returns_none_for_missing_dir() {
    let mut loader = LibLoader::new(PathBuf::from("/path/that/does/not/exist/anywhere"));
    assert!(loader.load_lib("es5").is_none());
    assert_eq!(loader.cache_size(), 0);
}

/// `load_lib` returns `None` for a missing lib name even when the dir exists.
#[test]
fn lib_loader_load_lib_returns_none_for_missing_file() {
    let tmp = std::env::temp_dir().join("tsz_lib_loader_missing_file_test");
    std::fs::create_dir_all(&tmp).expect("mkdir");
    let mut loader = LibLoader::new(tmp.clone());
    assert!(loader.load_lib("nonexistent_lib_name_xyz").is_none());
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Files are read from disk and cached on first access; subsequent
/// accesses return the cached content without rereading the file.
#[test]
fn lib_loader_load_lib_caches_first_read() {
    let tmp = std::env::temp_dir().join("tsz_lib_loader_cache_first");
    std::fs::create_dir_all(&tmp).expect("mkdir");
    let path = tmp.join("lib.es5.d.ts");
    std::fs::write(&path, "declare var Object: any;\n").expect("write");

    let mut loader = LibLoader::new(tmp.clone());
    assert_eq!(loader.cache_size(), 0);

    let first = loader.load_lib("es5").map(str::to_owned);
    assert_eq!(first.as_deref(), Some("declare var Object: any;\n"));
    assert_eq!(loader.cache_size(), 1);

    // Mutate the file on disk; the second call must still return the cached
    // value, proving that the loader is not rereading every call.
    std::fs::write(&path, "declare var Other: any;\n").expect("write2");
    let second = loader.load_lib("es5").map(str::to_owned);
    assert_eq!(second.as_deref(), Some("declare var Object: any;\n"));
    assert_eq!(loader.cache_size(), 1);

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Lib names are normalised to lowercase before lookup, so `ES5` and `es5`
/// share a single cache entry.
#[test]
fn lib_loader_load_lib_normalises_lib_name() {
    let tmp = std::env::temp_dir().join("tsz_lib_loader_normalise");
    std::fs::create_dir_all(&tmp).expect("mkdir");
    std::fs::write(tmp.join("lib.es5.d.ts"), "// es5").expect("write");

    let mut loader = LibLoader::new(tmp.clone());
    assert_eq!(loader.load_lib("ES5"), Some("// es5"));
    assert_eq!(loader.load_lib("es5"), Some("// es5"));
    assert_eq!(loader.load_lib("  ES5  "), Some("// es5"));
    assert_eq!(loader.cache_size(), 1);

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Files matching the secondary `<name>.d.ts` candidate are also resolved.
#[test]
fn lib_loader_load_lib_accepts_short_filename() {
    let tmp = std::env::temp_dir().join("tsz_lib_loader_short_name");
    std::fs::create_dir_all(&tmp).expect("mkdir");
    // Note the file does NOT have the `lib.` prefix.
    std::fs::write(tmp.join("dom.d.ts"), "// dom").expect("write");

    let mut loader = LibLoader::new(tmp.clone());
    assert_eq!(loader.load_lib("dom"), Some("// dom"));
    assert_eq!(loader.cache_size(), 1);

    let _ = std::fs::remove_dir_all(&tmp);
}

/// `clear_cache` empties the cache; subsequent loads will read from disk again.
#[test]
fn lib_loader_clear_cache_resets_size() {
    let tmp = std::env::temp_dir().join("tsz_lib_loader_clear_cache");
    std::fs::create_dir_all(&tmp).expect("mkdir");
    std::fs::write(tmp.join("lib.es5.d.ts"), "// content").expect("write");

    let mut loader = LibLoader::new(tmp.clone());
    let _ = loader.load_lib("es5");
    assert_eq!(loader.cache_size(), 1);

    loader.clear_cache();
    assert_eq!(loader.cache_size(), 0);

    // Reload still works after the cache has been cleared.
    assert_eq!(loader.load_lib("es5"), Some("// content"));
    assert_eq!(loader.cache_size(), 1);

    let _ = std::fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------
// is_es2015_plus_type — additional cases
// ---------------------------------------------------------------------------

/// `PromiseLike` is intentionally excluded from the ES2015+ list so that
/// noLib tests fall back to the regular TS2304 path. Locks down the special
/// case so future refactors don't accidentally re-include it.
#[test]
fn is_es2015_plus_type_excludes_promise_like() {
    // PromiseLike is in ES2015_PLUS_TYPES but explicitly returns false.
    assert!(!is_es2015_plus_type("PromiseLike"));

    // Sibling Promise-family entries still report true.
    assert!(is_es2015_plus_type("Promise"));
    assert!(is_es2015_plus_type("PromiseConstructor"));
    assert!(is_es2015_plus_type("PromiseConstructorLike"));
    assert!(is_es2015_plus_type("PromiseSettledResult"));
}

/// Iterator/generator family types are ES2015+.
#[test]
fn is_es2015_plus_type_iterator_family() {
    assert!(is_es2015_plus_type("Iterator"));
    assert!(is_es2015_plus_type("IterableIterator"));
    assert!(is_es2015_plus_type("IteratorResult"));
    assert!(is_es2015_plus_type("Generator"));
    assert!(is_es2015_plus_type("GeneratorFunction"));
    assert!(is_es2015_plus_type("AsyncIterator"));
    assert!(is_es2015_plus_type("AsyncIterable"));
}

/// Empty and unrelated names should not match the table.
#[test]
fn is_es2015_plus_type_negative_cases() {
    assert!(!is_es2015_plus_type(""));
    assert!(!is_es2015_plus_type("Foo"));
    assert!(!is_es2015_plus_type("MyPromise"));
    assert!(!is_es2015_plus_type("promise")); // case-sensitive
    // Pre-ES2015 typed arrays defined in lib.es5 are NOT in the list.
    assert!(!is_es2015_plus_type("Int8Array"));
    assert!(!is_es2015_plus_type("Uint8Array"));
    assert!(!is_es2015_plus_type("DataView"));
}

// ---------------------------------------------------------------------------
// get_suggested_lib_for_type
// ---------------------------------------------------------------------------

/// Default ES2015+ types map to the `es2015` suggestion.
#[test]
fn get_suggested_lib_default_es2015() {
    assert_eq!(get_suggested_lib_for_type("Promise"), "es2015");
    assert_eq!(get_suggested_lib_for_type("Map"), "es2015");
    assert_eq!(get_suggested_lib_for_type("Set"), "es2015");
    assert_eq!(get_suggested_lib_for_type("Symbol"), "es2015");
    assert_eq!(get_suggested_lib_for_type("Iterator"), "es2015");
    // Names not in the table also fall through to the default arm.
    assert_eq!(get_suggested_lib_for_type("UnknownType"), "es2015");
    assert_eq!(get_suggested_lib_for_type(""), "es2015");
}

/// `SharedArrayBuffer` family is es2017.
#[test]
fn get_suggested_lib_shared_array_buffer_is_es2017() {
    assert_eq!(get_suggested_lib_for_type("SharedArrayBuffer"), "es2017");
    assert_eq!(
        get_suggested_lib_for_type("SharedArrayBufferConstructor"),
        "es2017"
    );
    assert_eq!(get_suggested_lib_for_type("Atomics"), "es2017");
}

/// `AsyncGenerator` family is es2018.
#[test]
fn get_suggested_lib_async_generator_is_es2018() {
    assert_eq!(get_suggested_lib_for_type("AsyncGenerator"), "es2018");
    assert_eq!(
        get_suggested_lib_for_type("AsyncGeneratorFunction"),
        "es2018"
    );
    assert_eq!(
        get_suggested_lib_for_type("AsyncGeneratorFunctionConstructor"),
        "es2018"
    );
}

/// `BigInt` family is es2020.
#[test]
fn get_suggested_lib_bigint_is_es2020() {
    assert_eq!(get_suggested_lib_for_type("BigInt"), "es2020");
    assert_eq!(get_suggested_lib_for_type("BigIntConstructor"), "es2020");
    assert_eq!(get_suggested_lib_for_type("BigInt64Array"), "es2020");
    assert_eq!(
        get_suggested_lib_for_type("BigInt64ArrayConstructor"),
        "es2020"
    );
    assert_eq!(get_suggested_lib_for_type("BigUint64Array"), "es2020");
    assert_eq!(
        get_suggested_lib_for_type("BigUint64ArrayConstructor"),
        "es2020"
    );
}

/// `FinalizationRegistry` / `WeakRef` / `AggregateError` / `ErrorOptions` are es2021.
#[test]
fn get_suggested_lib_finalization_family_is_es2021() {
    assert_eq!(get_suggested_lib_for_type("FinalizationRegistry"), "es2021");
    assert_eq!(
        get_suggested_lib_for_type("FinalizationRegistryConstructor"),
        "es2021"
    );
    assert_eq!(get_suggested_lib_for_type("WeakRef"), "es2021");
    assert_eq!(get_suggested_lib_for_type("WeakRefConstructor"), "es2021");
    assert_eq!(get_suggested_lib_for_type("AggregateError"), "es2021");
    assert_eq!(
        get_suggested_lib_for_type("AggregateErrorConstructor"),
        "es2021"
    );
    assert_eq!(get_suggested_lib_for_type("ErrorOptions"), "es2021");
}

/// Disposable / `AsyncDisposable` suggest `esnext`.
#[test]
fn get_suggested_lib_disposable_is_esnext() {
    assert_eq!(get_suggested_lib_for_type("Disposable"), "esnext");
    assert_eq!(get_suggested_lib_for_type("AsyncDisposable"), "esnext");
}

// ---------------------------------------------------------------------------
// Diagnostic constructors
// ---------------------------------------------------------------------------

/// `emit_error_global_type_missing` produces a TS2318 error diagnostic with the
/// type name embedded in the message.
#[test]
fn emit_error_global_type_missing_has_correct_shape() {
    let diag = emit_error_global_type_missing("Promise", "main.ts".to_string(), 42, 7);

    assert_eq!(diag.code, CANNOT_FIND_GLOBAL_TYPE);
    assert_eq!(diag.code, 2318);
    assert_eq!(diag.file, "main.ts");
    assert_eq!(diag.start, 42);
    assert_eq!(diag.length, 7);
    assert!(diag.message_text.contains("Promise"));
    assert!(diag.message_text.contains("global type"));
    assert_eq!(
        diag.category,
        tsz_common::diagnostics::DiagnosticCategory::Error
    );
    assert!(diag.related_information.is_empty());
}

/// `emit_error_lib_target_mismatch` produces a TS2583 error diagnostic that
/// includes both the type name and the recommended `lib` option upgrade.
#[test]
fn emit_error_lib_target_mismatch_has_correct_shape() {
    let diag = emit_error_lib_target_mismatch("Map", "lib.ts".to_string(), 0, 3);

    assert_eq!(diag.code, MISSING_ES2015_LIB_SUPPORT);
    assert_eq!(diag.code, 2583);
    assert_eq!(diag.file, "lib.ts");
    assert_eq!(diag.start, 0);
    assert_eq!(diag.length, 3);
    assert!(diag.message_text.contains("Map"));
    assert!(diag.message_text.contains("'lib' compiler option"));
    assert!(diag.message_text.contains("es2015"));
    assert_eq!(
        diag.category,
        tsz_common::diagnostics::DiagnosticCategory::Error
    );
}

/// Error-code constants must remain stable; downstream code relies on the
/// exact values for consumer-side filtering.
#[test]
fn diagnostic_code_constants_are_stable() {
    assert_eq!(CANNOT_FIND_GLOBAL_TYPE, 2318);
    assert_eq!(MISSING_ES2015_LIB_SUPPORT, 2583);
}

// ---------------------------------------------------------------------------
// LibFile basic accessors
// ---------------------------------------------------------------------------

/// `LibFile::file_locals` exposes the bound symbol table from the underlying
/// binder; sanity-check that it round-trips an entry built ourselves.
#[test]
fn lib_file_file_locals_returns_binder_locals() {
    let mut arena = SymbolArena::new();
    let id = arena.alloc(symbol_flags::VALUE, "MyGlobal".to_string());

    let mut locals = SymbolTable::new();
    locals.set("MyGlobal".to_string(), id);

    let binder =
        BinderState::from_bound_state(arena, locals, Arc::new(rustc_hash::FxHashMap::default()));
    let lib = LibFile::new(
        "synthetic.d.ts".to_string(),
        Arc::new(NodeArena::new()),
        Arc::new(binder),
        tsz_parser::NodeIndex(0),
    );

    assert!(lib.file_locals().has("MyGlobal"));
    assert_eq!(lib.file_locals().get("MyGlobal"), Some(id));
    assert!(!lib.file_locals().has("Other"));
    assert_eq!(lib.file_name, "synthetic.d.ts");
}

/// An empty source produces a `LibFile` whose locals contain no user-declared
/// names. Locks down that `from_source` does not accidentally inject globals.
#[test]
fn lib_file_from_source_empty_input() {
    let lib = LibFile::from_source("empty.d.ts".to_string(), String::new());
    assert_eq!(lib.file_name, "empty.d.ts");
    assert!(!lib.file_locals().has("console"));
    assert!(!lib.file_locals().has("Array"));
}
