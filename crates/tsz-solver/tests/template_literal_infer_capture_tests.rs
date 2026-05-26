use super::*;
use crate::construction::TypeInterner;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::type_queries::extended::{is_string_like_type, string_like_type_for_type};

fn infer_slot(interner: &TypeInterner, name: &str) -> TypeId {
    let name = interner.intern_string(name);
    interner.intern(TypeData::Infer(TypeParamInfo {
        name,
        constraint: None,
        default: None,
        is_const: false,
    }))
}

fn conditional_head_capture(
    interner: &TypeInterner,
    param_name: &str,
    head_name: &str,
    tail_name: &str,
    separator: &str,
) -> (tsz_common::interner::Atom, TypeId) {
    let input_name = interner.intern_string(param_name);
    let input = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: input_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let head = infer_slot(interner, head_name);
    let tail = infer_slot(interner, tail_name);
    let separator = interner.intern_string(separator);
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(head),
        TemplateSpan::Text(separator),
        TemplateSpan::Type(tail),
    ]);
    let conditional = interner.conditional(ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: head,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    (input_name, conditional)
}

#[test]
fn conditional_infer_template_literal_captures_number_segment_as_string_subtype() {
    let interner = TypeInterner::new();
    let dash = interner.intern_string("-");
    let (input_name, conditional) = conditional_head_capture(&interner, "S", "A", "B", "-");
    let source = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(dash),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut subst = TypeSubstitution::new();
    subst.insert(input_name, source);
    let result = evaluate_type(&interner, instantiate_type(&interner, conditional, &subst));
    let expected = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);

    assert_eq!(result, expected, "A must be `${{number}}`");
    assert_ne!(result, TypeId::NUMBER, "A must not be bare `number`");
}

#[test]
fn conditional_infer_template_literal_unequal_spans_splits_source_text() {
    let interner = TypeInterner::new();
    let (input_name, conditional) = conditional_head_capture(&interner, "S", "A", "B", "-");
    let source = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string("-x")),
    ]);

    let mut subst = TypeSubstitution::new();
    subst.insert(input_name, source);
    let result = evaluate_type(&interner, instantiate_type(&interner, conditional, &subst));
    let expected = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);

    assert_eq!(
        result, expected,
        "A must be `${{number}}` after text splitting"
    );
    assert_ne!(
        result,
        TypeId::NEVER,
        "pattern matching must not take the false branch"
    );
}

#[test]
fn conditional_infer_template_literal_renamed_vars_capture_string_subtype() {
    let interner = TypeInterner::new();
    let slash = interner.intern_string("/");
    let (input_name, conditional) =
        conditional_head_capture(&interner, "Input", "Head", "Tail", "/");
    let source = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::BIGINT),
        TemplateSpan::Text(slash),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut subst = TypeSubstitution::new();
    subst.insert(input_name, source);
    let result = evaluate_type(&interner, instantiate_type(&interner, conditional, &subst));

    assert_ne!(result, TypeId::BIGINT, "Head must not be bare `bigint`");
    assert_ne!(
        result,
        TypeId::NEVER,
        "matching must not depend on infer variable names"
    );
}

#[test]
fn string_like_type_for_type_wraps_non_string_intrinsics() {
    let interner = TypeInterner::new();
    let db: &dyn crate::construction::TypeDatabase = &interner;

    let wrapped_number = string_like_type_for_type(db, TypeId::NUMBER);
    assert_ne!(wrapped_number, TypeId::NUMBER);
    assert!(matches!(
        interner.lookup(wrapped_number),
        Some(TypeData::TemplateLiteral(_))
    ));

    let wrapped_bigint = string_like_type_for_type(db, TypeId::BIGINT);
    assert_ne!(wrapped_bigint, TypeId::BIGINT);
    assert!(matches!(
        interner.lookup(wrapped_bigint),
        Some(TypeData::TemplateLiteral(_))
    ));
}

#[test]
fn string_like_type_for_type_preserves_string_domain() {
    let interner = TypeInterner::new();
    let db: &dyn crate::construction::TypeDatabase = &interner;
    let literal = interner.literal_string("foo");
    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);

    assert_eq!(
        string_like_type_for_type(db, TypeId::STRING),
        TypeId::STRING
    );
    assert_eq!(string_like_type_for_type(db, TypeId::ANY), TypeId::ANY);
    assert_eq!(string_like_type_for_type(db, literal), literal);
    assert_eq!(string_like_type_for_type(db, template), template);
    assert!(is_string_like_type(db, TypeId::STRING));
    assert!(is_string_like_type(db, TypeId::ANY));
    assert!(is_string_like_type(db, literal));
    assert!(is_string_like_type(db, template));
    assert!(!is_string_like_type(db, TypeId::NUMBER));
    assert!(!is_string_like_type(db, TypeId::BIGINT));
    assert!(!is_string_like_type(db, TypeId::BOOLEAN));
}
