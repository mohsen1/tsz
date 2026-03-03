//! Tests for diagnostic generation.

use super::*;
use crate::TypeInterner;
use crate::diagnostics::builders::{
    DiagnosticBuilder, DiagnosticCollector, SourceLocation, SpannedDiagnosticBuilder,
};
use crate::diagnostics::format::TypeFormatter;
#[test]
fn test_format_intrinsic_types() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    assert_eq!(formatter.format(TypeId::STRING), "string");
    assert_eq!(formatter.format(TypeId::NUMBER), "number");
    assert_eq!(formatter.format(TypeId::BOOLEAN), "boolean");
    assert_eq!(formatter.format(TypeId::NEVER), "never");
    assert_eq!(formatter.format(TypeId::UNKNOWN), "unknown");
    assert_eq!(formatter.format(TypeId::ANY), "any");
    assert_eq!(formatter.format(TypeId::VOID), "void");
    assert_eq!(formatter.format(TypeId::NULL), "null");
    assert_eq!(formatter.format(TypeId::UNDEFINED), "undefined");
}

#[test]
fn test_format_literal_types() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let hello = interner.literal_string("hello");
    assert_eq!(formatter.format(hello), "\"hello\"");

    let num = interner.literal_number(42.0);
    assert_eq!(formatter.format(num), "42");

    let true_lit = interner.literal_boolean(true);
    assert_eq!(formatter.format(true_lit), "true");

    let false_lit = interner.literal_boolean(false);
    assert_eq!(formatter.format(false_lit), "false");
}

#[test]
fn test_format_object_type() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::opt(interner.intern_string("y"), TypeId::STRING),
    ]);

    let formatted = formatter.format(obj);
    assert!(formatted.contains("x: number"));
    // Optional properties display with `| undefined` to match tsc output
    assert!(formatted.contains("y?: string | undefined"));
}

#[test]
fn test_format_object_type_preserves_property_insertion_order() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    assert_eq!(formatter.format(obj), "{ name: string; age: number; }");
}

#[test]
fn test_format_object_type_numeric_keys_sorted_first() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    // Simulate the tuple-like object: { 0: string; 1: number; length: 2; }
    // Properties are stored sorted by Atom ID internally, which may not be
    // numeric order. The formatter should sort: numeric keys first in numeric
    // order, then string keys alphabetically.
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("1"), TypeId::NUMBER),
        PropertyInfo::new(
            interner.intern_string("length"),
            interner.literal_number(2.0),
        ),
    ]);

    assert_eq!(
        formatter.format(obj),
        "{ 0: string; 1: number; length: 2; }"
    );
}

#[test]
fn test_format_object_type_same_decl_order_uses_numeric_tiebreak() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    // When properties have the same declaration_order (e.g., from type
    // evaluation merging), numeric keys should be sorted numerically.
    // This simulates the case where all numeric keys get decl_order=1.
    let mut prop1 = PropertyInfo::new(interner.intern_string("1"), TypeId::NUMBER);
    let mut prop0 = PropertyInfo::new(interner.intern_string("0"), TypeId::STRING);
    let mut prop_len = PropertyInfo::new(
        interner.intern_string("length"),
        interner.literal_number(2.0),
    );
    prop1.declaration_order = 1;
    prop0.declaration_order = 1;
    prop_len.declaration_order = 2;

    // Build object shape directly to control declaration_order
    let shape = crate::types::ObjectShape {
        properties: vec![prop1, prop0, prop_len],
        ..Default::default()
    };
    let obj = interner.object_with_index(shape);

    assert_eq!(
        formatter.format(obj),
        "{ 0: string; 1: number; length: 2; }"
    );
}

#[test]
fn test_format_union_type() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    // Union ordering may vary due to sorting
    let formatted = formatter.format(union);
    assert!(formatted.contains("string"));
    assert!(formatted.contains("number"));
    assert!(formatted.contains(" | "));
}

#[test]
fn test_format_array_type() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let arr = interner.array(TypeId::STRING);
    assert_eq!(formatter.format(arr), "string[]");
}

#[test]
fn test_format_array_of_union_type_parenthesized() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let arr = interner.array(elem);

    let formatted = formatter.format(arr);
    assert!(formatted.starts_with('(') && formatted.ends_with(")[]"));
    assert!(formatted.contains("string"));
    assert!(formatted.contains("number"));
    assert!(formatted.contains(" | "));
}

#[test]
fn test_format_function_type() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let formatted = formatter.format(func);
    assert!(formatted.contains("x: string"));
    assert!(formatted.contains("=> number"));
}

#[test]
fn test_format_function_type_with_this() {
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let formatted = formatter.format(func);
    assert!(formatted.contains("this: string"));
    assert!(formatted.contains("x: number"));
}

#[test]
fn test_type_not_assignable_diagnostic() {
    let interner = TypeInterner::new();
    let mut builder = DiagnosticBuilder::new(&interner);

    let diag = builder.type_not_assignable(TypeId::STRING, TypeId::NUMBER);
    assert_eq!(diag.code, codes::TYPE_NOT_ASSIGNABLE); // This uses the alias from codes module
    assert!(diag.message.contains("string"));
    assert!(diag.message.contains("number"));
    assert!(diag.message.contains("not assignable"));
}

#[test]
fn test_union_member_mismatch_diagnostic_includes_related_members() {
    let interner = TypeInterner::new();
    let union_members = vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::BIGINT,
    ];
    let union = interner.union(union_members.clone());

    let reason = SubtypeFailureReason::NoUnionMemberMatches {
        source_type: TypeId::NULL,
        target_union_members: union_members,
    };

    if let SubtypeFailureReason::NoUnionMemberMatches {
        target_union_members,
        ..
    } = &reason
    {
        assert_eq!(target_union_members.len(), 4);
    } else {
        panic!("Expected NoUnionMemberMatches");
    }

    let pending = reason
        .to_diagnostic(TypeId::NULL, union)
        .with_span(SourceSpan::new("test.ts", 0, 1));
    assert_eq!(pending.related.len(), 3);

    let mut formatter = TypeFormatter::new(&interner);
    let diag = formatter.render(&pending);
    assert_eq!(diag.related.len(), 3);
    assert!(diag.message.contains("null"));

    let related_messages: Vec<&str> = diag
        .related
        .iter()
        .map(|info| info.message.as_str())
        .collect();
    assert!(related_messages.iter().any(|msg| msg.contains("string")));
    assert!(related_messages.iter().any(|msg| msg.contains("number")));
    assert!(related_messages.iter().any(|msg| msg.contains("boolean")));
    assert!(!related_messages.iter().any(|msg| msg.contains("bigint")));
}

#[test]
fn test_property_missing_diagnostic() {
    let interner = TypeInterner::new();
    let mut builder = DiagnosticBuilder::new(&interner);

    let obj1 = interner.object(vec![]);
    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let diag = builder.property_missing("x", obj1, obj2);
    assert_eq!(diag.code, codes::PROPERTY_MISSING);
    assert!(diag.message.contains("'x'"));
    assert!(diag.message.contains("missing"));
}

#[test]
fn test_diagnostic_with_span() {
    let diag =
        TypeDiagnostic::error("Test error", 2322).with_span(SourceSpan::new("test.ts", 10, 5));

    assert!(diag.span.is_some());
    let span = diag.span.unwrap();
    assert_eq!(span.start, 10);
    assert_eq!(span.length, 5);
    assert_eq!(span.file.as_ref(), "test.ts");
}

#[test]
fn test_diagnostic_with_related() {
    let diag = TypeDiagnostic::error("Test error", 2322)
        .with_related(SourceSpan::new("other.ts", 20, 3), "See declaration here");

    assert_eq!(diag.related.len(), 1);
    assert_eq!(diag.related[0].message, "See declaration here");
}

// =============================================================================
// Source Location Tracking Tests
// =============================================================================

#[test]
fn test_source_location_new() {
    let loc = SourceLocation::new("test.ts", 10, 25);
    assert_eq!(loc.file.as_ref(), "test.ts");
    assert_eq!(loc.start, 10);
    assert_eq!(loc.end, 25);
}

#[test]
fn test_source_location_length() {
    let loc = SourceLocation::new("test.ts", 10, 25);
    assert_eq!(loc.length(), 15);
}

#[test]
fn test_source_location_to_span() {
    let loc = SourceLocation::new("test.ts", 10, 25);
    let span = loc.to_span();
    assert_eq!(span.file.as_ref(), "test.ts");
    assert_eq!(span.start, 10);
    assert_eq!(span.length, 15);
}

#[test]
fn test_spanned_diagnostic_builder() {
    let interner = TypeInterner::new();
    let mut builder = SpannedDiagnosticBuilder::new(&interner, "test.ts");

    let diag = builder.type_not_assignable(TypeId::STRING, TypeId::NUMBER, 10, 5);

    assert!(diag.span.is_some());
    let span = diag.span.as_ref().unwrap();
    assert_eq!(span.file.as_ref(), "test.ts");
    assert_eq!(span.start, 10);
    assert_eq!(span.length, 5);
    assert!(diag.message.contains("string"));
    assert!(diag.message.contains("number"));
}

#[test]
fn test_spanned_diagnostic_builder_cannot_find_name() {
    let interner = TypeInterner::new();
    let mut builder = SpannedDiagnosticBuilder::new(&interner, "test.ts");

    let diag = builder.cannot_find_name("myVariable", 20, 10);

    assert!(diag.span.is_some());
    let span = diag.span.as_ref().unwrap();
    assert_eq!(span.start, 20);
    assert_eq!(span.length, 10);
    assert!(diag.message.contains("myVariable"));
    assert_eq!(diag.code, codes::CANNOT_FIND_NAME);
}

#[test]
fn test_spanned_diagnostic_builder_argument_count() {
    let interner = TypeInterner::new();
    let mut builder = SpannedDiagnosticBuilder::new(&interner, "test.ts");

    let diag = builder.argument_count_mismatch(3, 3, 1, 50, 15);

    assert!(diag.span.is_some());
    let span = diag.span.as_ref().unwrap();
    assert_eq!(span.start, 50);
    assert_eq!(span.length, 15);
    assert!(diag.message.contains("3"));
    assert!(diag.message.contains("1"));
    assert_eq!(diag.code, codes::ARG_COUNT_MISMATCH);
}

#[test]
fn test_diagnostic_collector() {
    let interner = TypeInterner::new();
    let mut collector = DiagnosticCollector::new(&interner, "test.ts");

    let loc = SourceLocation::new("test.ts", 10, 25);
    collector.type_not_assignable(TypeId::STRING, TypeId::NUMBER, &loc);

    let diagnostics = collector.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].span.is_some());
    assert!(diagnostics[0].message.contains("string"));
}

#[test]
fn test_diagnostic_collector_multiple_errors() {
    let interner = TypeInterner::new();
    let mut collector = DiagnosticCollector::new(&interner, "test.ts");

    let loc1 = SourceLocation::new("test.ts", 10, 20);
    let loc2 = SourceLocation::new("test.ts", 30, 45);

    collector.type_not_assignable(TypeId::STRING, TypeId::NUMBER, &loc1);
    collector.cannot_find_name("foo", &loc2);

    assert_eq!(collector.diagnostics().len(), 2);
}

#[test]
fn test_diagnostic_to_checker_diagnostic() {
    let diag =
        TypeDiagnostic::error("Test error", 2322).with_span(SourceSpan::new("test.ts", 10, 5));

    let checker_diag = diag.to_checker_diagnostic("default.ts");

    assert_eq!(checker_diag.file, "test.ts");
    assert_eq!(checker_diag.start, 10);
    assert_eq!(checker_diag.length, 5);
    assert_eq!(checker_diag.message_text, "Test error");
    assert_eq!(checker_diag.code, 2322);
}

#[test]
fn test_diagnostic_to_checker_diagnostic_no_span() {
    let diag = TypeDiagnostic::error("Test error", 2322);

    let checker_diag = diag.to_checker_diagnostic("default.ts");

    // Should use default file when no span
    assert_eq!(checker_diag.file, "default.ts");
    assert_eq!(checker_diag.start, 0);
    assert_eq!(checker_diag.length, 0);
}

#[test]
fn test_diagnostic_to_checker_diagnostic_with_related() {
    let diag = TypeDiagnostic::error("Test error", 2322)
        .with_span(SourceSpan::new("test.ts", 10, 5))
        .with_related(SourceSpan::new("other.ts", 20, 3), "See here");

    let checker_diag = diag.to_checker_diagnostic("default.ts");

    assert_eq!(checker_diag.related_information.len(), 1);
    assert_eq!(checker_diag.related_information[0].file, "other.ts");
    assert_eq!(checker_diag.related_information[0].start, 20);
    assert_eq!(checker_diag.related_information[0].length, 3);
    assert_eq!(checker_diag.related_information[0].message_text, "See here");
}

#[test]
fn test_diagnostic_collector_to_checker_diagnostics() {
    let interner = TypeInterner::new();
    let mut collector = DiagnosticCollector::new(&interner, "test.ts");

    let loc = SourceLocation::new("test.ts", 10, 25);
    collector.type_not_assignable(TypeId::STRING, TypeId::NUMBER, &loc);

    let checker_diags = collector.to_checker_diagnostics();
    assert_eq!(checker_diags.len(), 1);
    assert_eq!(checker_diags[0].file, "test.ts");
    assert_eq!(checker_diags[0].start, 10);
}

#[test]
fn test_diagnostic_builder_new_codes() {
    let interner = TypeInterner::new();
    let mut builder = DiagnosticBuilder::new(&interner);

    // Test cannot_find_name
    let diag = builder.cannot_find_name("myVar");
    assert_eq!(diag.code, codes::CANNOT_FIND_NAME);
    assert!(diag.message.contains("myVar"));

    // Test not_callable
    let diag = builder.not_callable(TypeId::NUMBER);
    assert_eq!(diag.code, codes::NOT_CALLABLE);
    assert!(diag.message.contains("not callable"));

    // Test argument_count_mismatch
    let diag = builder.argument_count_mismatch(2, 2, 5);
    assert_eq!(diag.code, codes::ARG_COUNT_MISMATCH);
    assert!(diag.message.contains("2"));
    assert!(diag.message.contains("5"));

    // Test readonly_property
    let diag = builder.readonly_property("x");
    assert_eq!(diag.code, codes::READONLY_PROPERTY);
    assert!(diag.message.contains("x"));
}

#[test]
fn test_too_many_parameters_reason_produces_ts2322_not_ts2554() {
    // When a function has too many required parameters for the target,
    // the failure reason should produce TS2322 (type not assignable),
    // not TS2554 (argument count mismatch). TS2554 is only for call sites.
    let reason = SubtypeFailureReason::TooManyParameters {
        source_count: 2,
        target_count: 0,
    };
    assert_eq!(
        reason.diagnostic_code(),
        codes::TYPE_NOT_ASSIGNABLE,
        "TooManyParameters should map to TS2322 (TYPE_NOT_ASSIGNABLE), not TS2554"
    );

    // Verify the to_diagnostic also uses TYPE_NOT_ASSIGNABLE
    let pending = reason.to_diagnostic(TypeId::STRING, TypeId::NUMBER);
    assert_eq!(
        pending.code,
        codes::TYPE_NOT_ASSIGNABLE,
        "TooManyParameters diagnostic should use TS2322 code"
    );
}

// =============================================================================
// Optional property/parameter `| undefined` display tests
// =============================================================================

#[test]
fn test_optional_property_shows_undefined() {
    // tsc displays optional properties as `name?: T | undefined` when
    // exactOptionalPropertyTypes is not enabled (the default).
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::opt(interner.intern_string("y"), TypeId::STRING),
    ]);

    let formatted = formatter.format(obj);
    assert_eq!(
        formatted, "{ x: number; y?: string | undefined; }",
        "Optional properties should include '| undefined' in display"
    );
}

#[test]
fn test_optional_property_already_has_undefined_no_duplicate() {
    // If the property type already includes undefined (e.g., `string | undefined`),
    // the formatter should not add a duplicate `| undefined`.
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let str_or_undef = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("val"),
        str_or_undef,
    )]);

    let formatted = formatter.format(obj);
    assert_eq!(
        formatted, "{ val?: string | undefined; }",
        "Should not add duplicate '| undefined' when type already includes it"
    );
}

#[test]
fn test_optional_function_param_shows_undefined() {
    // tsc displays optional function params with `| undefined` when the stored
    // type doesn't already include undefined (e.g., interface member function types).
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let formatted = formatter.format(func);
    assert_eq!(
        formatted, "(x: number, y?: number | undefined) => number",
        "Optional params should include '| undefined' in display"
    );
}

#[test]
fn test_optional_param_already_has_undefined_no_duplicate() {
    // If the param type already includes undefined, don't double-add it.
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let num_or_undef = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: num_or_undef,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let formatted = formatter.format(func);
    assert_eq!(
        formatted, "(x?: number | undefined) => void",
        "Should not add duplicate '| undefined' when param type already includes it"
    );
}

#[test]
fn test_required_property_no_undefined() {
    // Required properties should NOT get `| undefined` appended.
    let interner = TypeInterner::new();
    let mut formatter = TypeFormatter::new(&interner);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let formatted = formatter.format(obj);
    assert_eq!(
        formatted, "{ name: string; }",
        "Required properties should not include '| undefined'"
    );
}
