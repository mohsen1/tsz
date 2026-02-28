use crate::intern::TypeInterner;
use crate::type_queries::extended::{IndexKeyKind, classify_index_key};
use crate::types::{IntrinsicKind, LiteralValue, OrderedFloat, TemplateSpan, TypeData, TypeId};

#[test]
fn classify_string_intrinsic() {
    let interner = TypeInterner::new();
    let s = interner.intern(TypeData::Intrinsic(IntrinsicKind::String));
    assert!(matches!(
        classify_index_key(&interner, s),
        IndexKeyKind::String
    ));
}

#[test]
fn classify_number_intrinsic() {
    let interner = TypeInterner::new();
    let n = interner.intern(TypeData::Intrinsic(IntrinsicKind::Number));
    assert!(matches!(
        classify_index_key(&interner, n),
        IndexKeyKind::Number
    ));
}

#[test]
fn classify_string_literal() {
    let interner = TypeInterner::new();
    let atom = interner.intern_string("hello");
    let s = interner.intern(TypeData::Literal(LiteralValue::String(atom)));
    assert!(matches!(
        classify_index_key(&interner, s),
        IndexKeyKind::StringLiteral
    ));
}

#[test]
fn classify_number_literal() {
    let interner = TypeInterner::new();
    let n = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(42.0))));
    assert!(matches!(
        classify_index_key(&interner, n),
        IndexKeyKind::NumberLiteral
    ));
}

#[test]
fn classify_template_literal_number_is_numeric_string() {
    // `${number}` should be classified as NumericStringLike
    let interner = TypeInterner::new();
    let tl = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);
    assert!(
        matches!(
            classify_index_key(&interner, tl),
            IndexKeyKind::NumericStringLike
        ),
        "Template literal `${{number}}` should be NumericStringLike"
    );
}

#[test]
fn classify_template_literal_string_is_template_string() {
    // `${string}` should be classified as TemplateLiteralString
    let interner = TypeInterner::new();
    let tl = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);
    assert!(
        matches!(
            classify_index_key(&interner, tl),
            IndexKeyKind::TemplateLiteralString
        ),
        "Template literal `${{string}}` should be TemplateLiteralString"
    );
}

#[test]
fn classify_template_literal_with_text_prefix_is_template_string() {
    // `hello${number}` has a text prefix, so NOT a pure numeric string type
    let interner = TypeInterner::new();
    let hello = interner.intern_string("hello");
    let tl = interner.template_literal(vec![
        TemplateSpan::Text(hello),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);
    assert!(
        matches!(
            classify_index_key(&interner, tl),
            IndexKeyKind::TemplateLiteralString
        ),
        "Template literal `hello${{number}}` should be TemplateLiteralString, not NumericStringLike"
    );
}
