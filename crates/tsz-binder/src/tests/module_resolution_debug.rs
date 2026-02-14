use super::*;

#[test]
fn test_flags_to_string() {
    assert_eq!(flags_to_string(symbol_flags::FUNCTION), "FUNCTION");
    assert_eq!(flags_to_string(symbol_flags::CLASS), "CLASS");
    assert_eq!(flags_to_string(symbol_flags::INTERFACE), "INTERFACE");
    assert_eq!(
        flags_to_string(symbol_flags::INTERFACE | symbol_flags::CLASS),
        "CLASS|INTERFACE"
    );
    assert_eq!(flags_to_string(symbol_flags::NONE), "NONE");
}

#[test]
fn test_debugger_records_events() {
    set_debug_enabled(true);

    let mut debugger = ModuleResolutionDebugger::new();
    debugger.set_current_file("test.ts");

    // Record a declaration
    debugger.record_declaration("MyClass", SymbolId(1), symbol_flags::CLASS, 1, false);

    assert_eq!(debugger.declaration_events.len(), 1);
    assert_eq!(debugger.declaration_events[0].name, "MyClass");
    assert!(!debugger.declaration_events[0].is_merge);

    // Record a merge
    debugger.record_merge(
        "MyInterface",
        SymbolId(2),
        symbol_flags::INTERFACE,
        symbol_flags::INTERFACE,
        symbol_flags::INTERFACE,
    );

    assert_eq!(debugger.merge_events.len(), 1);
    assert_eq!(debugger.merge_events[0].name, "MyInterface");

    // Record a lookup
    debugger.record_lookup(
        "MyClass",
        vec!["local".into(), "file".into()],
        Some(SymbolId(1)),
    );

    assert_eq!(debugger.lookup_events.len(), 1);
    assert!(debugger.lookup_events[0].found);

    set_debug_enabled(false);
}

#[test]
fn test_summary_generation() {
    // Directly populate events to avoid race with the global AtomicBool
    // toggle that other parallel tests may flip.
    let mut debugger = ModuleResolutionDebugger::new();
    debugger.set_current_file("test.ts");

    debugger
        .declaration_events
        .push(super::SymbolDeclarationEvent {
            name: "foo".to_string(),
            symbol_id: SymbolId(1),
            flags_description: "FUNCTION".to_string(),
            file_name: "test.ts".to_string(),
            declaration_count: 1,
            is_merge: false,
        });
    debugger.lookup_events.push(super::SymbolLookupEvent {
        name: "bar".to_string(),
        scope_path: vec!["scope1".into()],
        found: false,
        symbol_id: None,
        found_in_file: None,
    });

    let summary = debugger.get_summary();
    assert!(summary.contains("Module Resolution Debug Summary"));
    assert!(summary.contains("Total declarations: 1"));
    assert!(summary.contains("Failed Lookups:"));
}
