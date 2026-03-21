//! Tests for the name resolution boundary types and consolidated classifier.
//!
//! These tests verify that:
//! 1. `classify_unresolved_name_suggestion` correctly classifies known globals
//! 2. `is_primitive_type_keyword` correctly identifies primitive type keywords
//! 3. `NameResolutionRequest` / `ResolutionOutcome` can be constructed and queried
//! 4. The boundary classifier produces the same results as the old scattered checks

use tsz_checker::query_boundaries::name_resolution::*;

// =============================================================================
// classify_unresolved_name_suggestion tests
// =============================================================================

#[test]
fn classify_es2015_plus_types() {
    // Promise, Map, Set, Symbol, etc. should suggest changing lib
    for name in &[
        "Promise", "Map", "Set", "WeakMap", "WeakSet", "Proxy", "Reflect",
    ] {
        let class = classify_unresolved_name_suggestion(name);
        assert!(
            matches!(class, UnresolvedNameClass::Es2015PlusType { .. }),
            "Expected Es2015PlusType for '{name}', got {class:?}"
        );
    }
}

#[test]
fn classify_dom_globals() {
    for name in &[
        "console",
        "window",
        "document",
        "HTMLElement",
        "fetch",
        "setTimeout",
        "localStorage",
        "navigator",
        "addEventListener",
    ] {
        let class = classify_unresolved_name_suggestion(name);
        assert!(
            matches!(class, UnresolvedNameClass::DomGlobal),
            "Expected DomGlobal for '{name}', got {class:?}"
        );
    }
}

#[test]
fn classify_node_globals() {
    for name in &[
        "require",
        "exports",
        "module",
        "process",
        "Buffer",
        "__filename",
        "__dirname",
    ] {
        let class = classify_unresolved_name_suggestion(name);
        assert!(
            matches!(class, UnresolvedNameClass::NodeGlobal),
            "Expected NodeGlobal for '{name}', got {class:?}"
        );
    }
}

#[test]
fn classify_jquery_globals() {
    for name in &["$", "jQuery"] {
        let class = classify_unresolved_name_suggestion(name);
        assert!(
            matches!(class, UnresolvedNameClass::JQueryGlobal),
            "Expected JQueryGlobal for '{name}', got {class:?}"
        );
    }
}

#[test]
fn classify_test_runner_globals() {
    for name in &["describe", "suite", "it", "test"] {
        let class = classify_unresolved_name_suggestion(name);
        assert!(
            matches!(class, UnresolvedNameClass::TestRunnerGlobal),
            "Expected TestRunnerGlobal for '{name}', got {class:?}"
        );
    }
}

#[test]
fn classify_bun_global() {
    let class = classify_unresolved_name_suggestion("Bun");
    assert!(
        matches!(class, UnresolvedNameClass::BunGlobal),
        "Expected BunGlobal, got {class:?}"
    );
}

#[test]
fn classify_primitive_type_keywords() {
    for name in &[
        "number",
        "string",
        "boolean",
        "void",
        "undefined",
        "null",
        "any",
        "unknown",
        "never",
        "object",
        "bigint",
    ] {
        let class = classify_unresolved_name_suggestion(name);
        assert!(
            matches!(class, UnresolvedNameClass::PrimitiveTypeKeyword),
            "Expected PrimitiveTypeKeyword for '{name}', got {class:?}"
        );
    }
}

#[test]
fn classify_symbol_not_primitive() {
    // `symbol` is intentionally NOT a primitive type keyword in this classifier.
    // tsc emits TS2552 "Did you mean 'Symbol'?" instead of TS2693.
    let class = classify_unresolved_name_suggestion("symbol");
    assert!(
        matches!(class, UnresolvedNameClass::Unknown),
        "Expected Unknown for 'symbol', got {class:?}"
    );
}

#[test]
fn classify_unknown_names() {
    for name in &["foo", "bar", "MyClass", "myVar", "someFunction"] {
        let class = classify_unresolved_name_suggestion(name);
        assert!(
            matches!(class, UnresolvedNameClass::Unknown),
            "Expected Unknown for '{name}', got {class:?}"
        );
    }
}

// =============================================================================
// is_primitive_type_keyword tests
// =============================================================================

#[test]
fn primitive_type_keyword_positive() {
    for name in &[
        "number",
        "string",
        "boolean",
        "void",
        "undefined",
        "null",
        "any",
        "unknown",
        "never",
        "object",
        "bigint",
    ] {
        assert!(
            is_primitive_type_keyword(name),
            "Expected is_primitive_type_keyword('{name}') to be true"
        );
    }
}

#[test]
fn primitive_type_keyword_negative() {
    for name in &[
        "symbol", "Symbol", "Number", "String", "foo", "class", "function",
    ] {
        assert!(
            !is_primitive_type_keyword(name),
            "Expected is_primitive_type_keyword('{name}') to be false"
        );
    }
}

// =============================================================================
// UnresolvedNameClass::to_suggestion tests
// =============================================================================

#[test]
fn to_suggestion_maps_correctly() {
    assert!(matches!(
        UnresolvedNameClass::Unknown.to_suggestion(),
        NameSuggestion::None
    ));
    assert!(matches!(
        UnresolvedNameClass::PrimitiveTypeKeyword.to_suggestion(),
        NameSuggestion::TypeKeywordAsValue
    ));
    assert!(matches!(
        UnresolvedNameClass::DomGlobal.to_suggestion(),
        NameSuggestion::IncludeDomLib
    ));
    assert!(matches!(
        UnresolvedNameClass::NodeGlobal.to_suggestion(),
        NameSuggestion::InstallNodeTypes
    ));
    assert!(matches!(
        UnresolvedNameClass::JQueryGlobal.to_suggestion(),
        NameSuggestion::InstallJQueryTypes
    ));
    assert!(matches!(
        UnresolvedNameClass::TestRunnerGlobal.to_suggestion(),
        NameSuggestion::InstallTestTypes
    ));
    assert!(matches!(
        UnresolvedNameClass::BunGlobal.to_suggestion(),
        NameSuggestion::InstallBunTypes
    ));
    let es2015 = UnresolvedNameClass::Es2015PlusType {
        suggested_lib: "es2015".to_string(),
    };
    assert!(matches!(
        es2015.to_suggestion(),
        NameSuggestion::ChangeLib { .. }
    ));
}

// =============================================================================
// ResolutionOutcome construction tests
// =============================================================================

#[test]
fn resolution_outcome_found() {
    let outcome = ResolutionOutcome::found(ResolvedName::Intrinsic(tsz_solver::TypeId::UNDEFINED));
    assert!(outcome.is_found());
    assert!(!outcome.is_suppressed());
}

#[test]
fn resolution_outcome_suppressed() {
    let outcome = ResolutionOutcome::suppressed();
    assert!(!outcome.is_found());
    assert!(outcome.is_suppressed());
}

#[test]
fn resolution_outcome_failed_with_suggestion() {
    let outcome = ResolutionOutcome::failed(
        ResolutionFailure::NotFound,
        NameSuggestion::Spelling(vec!["myVariable".to_string()]),
    );
    assert!(!outcome.is_found());
    assert!(!outcome.is_suppressed());
    assert!(matches!(outcome.suggestion, NameSuggestion::Spelling(_)));
}

#[test]
fn resolution_outcome_namespace_as_value() {
    let outcome = ResolutionOutcome::failed(
        ResolutionFailure::NamespaceAsValue {
            namespace_name: "MyNamespace".to_string(),
        },
        NameSuggestion::None,
    );
    assert!(!outcome.is_found());
    assert!(matches!(
        outcome.result,
        Err(ResolutionFailure::NamespaceAsValue { .. })
    ));
}

#[test]
fn resolution_outcome_not_exported() {
    let outcome = ResolutionOutcome::failed(
        ResolutionFailure::NotExported {
            namespace_name: "MyModule".to_string(),
            member_name: "foo".to_string(),
            available_exports: vec!["bar".to_string(), "baz".to_string()],
        },
        NameSuggestion::ExportSpelling {
            suggestion: "bar".to_string(),
        },
    );
    assert!(!outcome.is_found());
    match &outcome.result {
        Err(ResolutionFailure::NotExported {
            namespace_name,
            member_name,
            available_exports,
        }) => {
            assert_eq!(namespace_name, "MyModule");
            assert_eq!(member_name, "foo");
            assert_eq!(available_exports.len(), 2);
        }
        _ => panic!("Expected NotExported failure"),
    }
}

// =============================================================================
// NameResolutionRequest construction tests
// =============================================================================

#[test]
fn name_resolution_request_value() {
    let req = NameResolutionRequest::value("x", tsz_parser::parser::NodeIndex::NONE);
    assert_eq!(req.kind, NameLookupKind::Value);
    assert_eq!(req.name, "x");
}

#[test]
fn name_resolution_request_type_ref() {
    let req = NameResolutionRequest::type_ref("Foo", tsz_parser::parser::NodeIndex::NONE);
    assert_eq!(req.kind, NameLookupKind::Type);
    assert_eq!(req.name, "Foo");
}

#[test]
fn name_resolution_request_namespace() {
    let req = NameResolutionRequest::namespace("MyNs", tsz_parser::parser::NodeIndex::NONE);
    assert_eq!(req.kind, NameLookupKind::Namespace);
    assert_eq!(req.name, "MyNs");
}
