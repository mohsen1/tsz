//! Helpers for string index-signature applicability during indexed access.

use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{IndexSignature, LiteralValue, TypeData, TypeId};

pub(super) fn string_index_signature_applies<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    string_index: &IndexSignature,
    index_type: TypeId,
) -> bool {
    if string_index.key_type == TypeId::STRING {
        // A plain `string` index signature covers every property access whose
        // key coerces to a string. Numbers (and numeric literals) coerce to
        // string keys, so they match a `string` index signature exactly like
        // `D[number]`, `D["x"]`, and `D[string]` do. Kept structural (no
        // subtype query) because this runs on the indexed-access hot path.
        return index_type == TypeId::STRING
            || index_type == TypeId::NUMBER
            || matches!(
                evaluator.interner().lookup(index_type),
                Some(TypeData::Literal(
                    LiteralValue::String(_) | LiteralValue::Number(_)
                ))
            )
            || is_string_like_index(evaluator, index_type);
    }

    if index_type == TypeId::STRING
        && matches!(
            evaluator.interner().lookup(string_index.key_type),
            Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. })
        )
    {
        return true;
    }

    let mut checker = SubtypeChecker::with_resolver(evaluator.interner(), evaluator.resolver());
    checker.is_subtype_of(index_type, string_index.key_type)
}

fn is_string_like_index<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    index_type: TypeId,
) -> bool {
    if index_type.is_intrinsic() {
        return false;
    }
    match evaluator.interner().lookup(index_type) {
        Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. }) => true,
        Some(TypeData::Intersection(list_id)) => evaluator
            .interner()
            .type_list(list_id)
            .iter()
            .any(|&member| is_string_like_intersection_member(evaluator, member)),
        _ => false,
    }
}

fn is_string_like_intersection_member<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    member: TypeId,
) -> bool {
    if member == TypeId::STRING {
        return true;
    }
    if member.is_intrinsic() {
        return false;
    }
    matches!(
        evaluator.interner().lookup(member),
        Some(
            TypeData::Literal(LiteralValue::String(_))
                | TypeData::TemplateLiteral(_)
                | TypeData::StringIntrinsic { .. }
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::evaluate::evaluate_index_access;
    use crate::intern::TypeInterner;
    use crate::types::{ObjectFlags, ObjectShape, TemplateSpan};

    #[test]
    fn template_pattern_string_index_rejects_non_matching_literal_key() {
        let db = TypeInterner::new();
        let prefix = db.intern_string("data-");
        let key_type = db.template_literal(vec![
            TemplateSpan::Text(prefix),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol: None,
        });

        let matching = db.literal_string("data-id");
        let non_matching = db.literal_string("other");

        assert_eq!(evaluate_index_access(&db, object, matching), TypeId::NUMBER);
        assert_eq!(
            evaluate_index_access(&db, object, non_matching),
            TypeId::UNDEFINED
        );
    }

    fn plain_string_index_object(db: &TypeInterner, value_type: TypeId) -> TypeId {
        db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol: None,
        })
    }

    #[test]
    fn numeric_literal_index_into_string_index_signature_resolves_value_type() {
        let db = TypeInterner::new();
        let object = plain_string_index_object(&db, TypeId::NUMBER);

        // Reported repro plus adjacent numeric literals.
        for n in [42.0, 0.0, -1.0, 1_000_000.0] {
            assert_eq!(
                evaluate_index_access(&db, object, db.literal_number(n)),
                TypeId::NUMBER,
                "D[{n}] should resolve to the string index value type"
            );
        }

        // `D[number]` and `D[string]` already worked; keep them as controls.
        assert_eq!(
            evaluate_index_access(&db, object, TypeId::NUMBER),
            TypeId::NUMBER
        );
        assert_eq!(
            evaluate_index_access(&db, object, TypeId::STRING),
            TypeId::NUMBER
        );
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_string("x")),
            TypeId::NUMBER
        );
    }

    #[test]
    fn numeric_literal_index_is_structural_over_value_type() {
        // Renaming/changing the value type must not affect the rule.
        let db = TypeInterner::new();
        let object = plain_string_index_object(&db, TypeId::BOOLEAN);
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(7.0)),
            TypeId::BOOLEAN
        );
    }

    #[test]
    fn numeric_index_signature_takes_precedence_over_string() {
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::BOOLEAN,
                readonly: false,
                param_name: None,
            }),
            symbol: None,
        });
        // A numeric literal prefers the number index signature.
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(5.0)),
            TypeId::BOOLEAN
        );
    }

    #[test]
    fn numeric_index_signature_only_resolves_for_numeric_literal() {
        // `E = { [k: number]: boolean }`; `E[5]` -> boolean control.
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: None,
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::BOOLEAN,
                readonly: false,
                param_name: None,
            }),
            symbol: None,
        });
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(5.0)),
            TypeId::BOOLEAN
        );
    }

    #[test]
    fn numeric_literal_index_without_any_index_signature_is_undefined() {
        // Negative control: no string or number index signature.
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol: None,
        });
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(42.0)),
            TypeId::UNDEFINED
        );
    }

    #[test]
    fn numeric_literal_does_not_match_template_pattern_string_index() {
        // A template-literal-pattern key must NOT be matched by a numeric
        // literal index (the pattern only covers matching string keys).
        let db = TypeInterner::new();
        let prefix = db.intern_string("data-");
        let key_type = db.template_literal(vec![
            TemplateSpan::Text(prefix),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol: None,
        });
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(42.0)),
            TypeId::UNDEFINED
        );
    }
}
