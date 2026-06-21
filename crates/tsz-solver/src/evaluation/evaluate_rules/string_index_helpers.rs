//! Helpers for index-signature applicability (string and numeric) during
//! indexed access.

use crate::construction::TypeDatabase;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{IndexSignature, IntrinsicKind, LiteralValue, TypeData, TypeId};

/// Whether an index-signature key type's key space includes the broad `symbol`
/// type, peeling transparent `Lazy` alias wrappers (e.g. `PropertyKey` resolves
/// to `string | number | symbol`). Shared by `keyof` key-kind classification
/// and mapped-type source classification; the bounded peel guards against
/// pathological alias cycles. See #14315.
pub(super) fn index_signature_key_includes_symbol(
    interner: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    key_type: TypeId,
) -> bool {
    let mut current = key_type;
    for _ in 0..8 {
        if current == TypeId::SYMBOL {
            return true;
        }
        match interner.lookup(current) {
            Some(TypeData::Intrinsic(IntrinsicKind::Symbol)) => return true,
            Some(TypeData::Union(members)) => {
                return interner
                    .type_list(members)
                    .iter()
                    .any(|&m| index_signature_key_includes_symbol(interner, resolver, m));
            }
            Some(TypeData::Lazy(def_id)) => match resolver.resolve_lazy(def_id, interner) {
                Some(resolved) if resolved != current => current = resolved,
                _ => return false,
            },
            _ => return false,
        }
    }
    false
}

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
    if let Some(db) = evaluator.query_db() {
        checker = checker.with_query_db(db);
    }
    checker.is_subtype_of(index_type, string_index.key_type)
}

/// Whether `index_type` is a *number subtype* that should resolve through a
/// numeric index signature (and, failing that, a string index signature, since
/// numeric keys coerce to string keys).
///
/// This is the numeric mirror of [`string_index_signature_applies`]. A numeric
/// index signature's `key_type` is always `number`, so applicability reduces to
/// "is `index_type` a subtype of `number`". The common cases (`number`, a
/// numeric literal, and a `number & { brand }` intersection) are answered
/// structurally on the hot path; anything else falls back to a subtype query so
/// general number subtypes (e.g. an enum type) still apply, matching tsc's
/// `isApplicableIndexType`.
pub(super) fn number_index_signature_applies<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    index_type: TypeId,
) -> bool {
    if index_type == TypeId::NUMBER {
        return true;
    }
    // A single interner lookup answers both structural fast paths: a numeric
    // literal, or a `number & { brand }` intersection.
    match evaluator.interner().lookup(index_type) {
        Some(TypeData::Literal(LiteralValue::Number(_))) => return true,
        Some(TypeData::Intersection(list_id))
            if evaluator
                .interner()
                .type_list(list_id)
                .iter()
                .any(|&member| is_number_like_intersection_member(evaluator, member)) =>
        {
            return true;
        }
        _ => {}
    }
    let mut checker = SubtypeChecker::with_resolver(evaluator.interner(), evaluator.resolver());
    if let Some(db) = evaluator.query_db() {
        checker = checker.with_query_db(db);
    }
    checker.is_subtype_of(index_type, TypeId::NUMBER)
}

/// Whether a non-numeric index signature's key space *includes the broad
/// `symbol` type* and therefore serves symbol-bearing keys. A bare `symbol`
/// signature (`[k: symbol]`) and a `string | symbol` / `PropertyKey` signature
/// both qualify; a plain `string` (or template/literal-pattern) signature does
/// not. Alias key types (e.g. `PropertyKey`) are resolved by the subtype query.
///
/// This is `key_type`-only (independent of the indexing key) so the
/// `string_index` slot can be routed to the symbol path exactly when its
/// declared key space accepts symbols — replacing the former `key_type ==
/// symbol` slot heuristic, which mis-routed union/alias keys (see #14315).
pub(super) fn index_signature_accepts_symbol<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    index_signature: &IndexSignature,
) -> bool {
    // Structural fast path (bare `symbol`, a `string | symbol` / `PropertyKey`
    // union, or a `Lazy` alias resolving to one): avoids constructing a
    // `SubtypeChecker` on the hot indexed-access path.
    if index_signature_key_includes_symbol(
        evaluator.interner(),
        evaluator.resolver(),
        index_signature.key_type,
    ) {
        return true;
    }
    // General fallback for non-structural keys (e.g. an intersection): `symbol`
    // is a subtype of the key type.
    let mut checker = SubtypeChecker::with_resolver(evaluator.interner(), evaluator.resolver());
    if let Some(db) = evaluator.query_db() {
        checker = checker.with_query_db(db);
    }
    checker.is_subtype_of(TypeId::SYMBOL, index_signature.key_type)
}

fn is_number_like_intersection_member<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    member: TypeId,
) -> bool {
    if member == TypeId::NUMBER {
        return true;
    }
    if member.is_intrinsic() {
        return false;
    }
    matches!(
        evaluator.interner().lookup(member),
        Some(TypeData::Literal(LiteralValue::Number(_)))
    )
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
    use crate::types::{ObjectFlags, ObjectShape, PropertyInfo, TemplateSpan};

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
            symbol_index: None,
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
            symbol_index: None,
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
            symbol_index: None,
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
            symbol_index: None,
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
            symbol_index: None,
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
            symbol_index: None,
            symbol: None,
        });
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(42.0)),
            TypeId::UNDEFINED
        );
    }

    // A `number & { brand }` tagged-number intersection used as an index key.
    // Structural over the brand: the property name/type are arbitrary and do not
    // change the rule.
    fn tagged_number(db: &TypeInterner, brand_name: &str, brand_type: TypeId) -> TypeId {
        let brand_atom = db.intern_string(brand_name);
        let brand = db.object(vec![PropertyInfo::new(brand_atom, brand_type)]);
        db.intersection2(TypeId::NUMBER, brand)
    }

    #[test]
    fn tagged_number_index_resolves_numeric_index_signature() {
        // `{ [x: number]: V }[number & { brand }]` -> V, exactly like `D[number]`.
        // This is the numeric mirror of the existing `string & { brand }` path
        // and the regression repro from operatorsAndIntersectionTypes.ts.
        for value_type in [TypeId::STRING, TypeId::BOOLEAN, TypeId::NUMBER] {
            let db = TypeInterner::new();
            let object = db.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
                properties: Vec::new(),
                string_index: None,
                number_index: Some(IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type,
                    readonly: false,
                    param_name: None,
                }),
                symbol_index: None,
                symbol: None,
            });
            // Vary the brand name/type so the rule is structural, not name-driven.
            let key = tagged_number(&db, "serialNo", TypeId::BOOLEAN);
            assert_eq!(
                evaluate_index_access(&db, object, key),
                value_type,
                "tagged number key must resolve through the numeric index signature"
            );
        }
    }

    #[test]
    fn tagged_number_index_falls_back_to_string_index_signature() {
        // `{ [x: string]: V }[number & { brand }]` -> V: a numeric key coerces to
        // a string key, so it resolves through a string index signature when no
        // numeric one is present.
        let db = TypeInterner::new();
        let object = plain_string_index_object(&db, TypeId::BOOLEAN);
        let key = tagged_number(&db, "tag", db.literal_number(1.0));
        assert_eq!(evaluate_index_access(&db, object, key), TypeId::BOOLEAN);
    }

    #[test]
    fn tagged_number_index_prefers_numeric_over_string_index_signature() {
        // With both signatures present the numeric one wins, like a bare `number`
        // key.
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
            symbol_index: None,
            symbol: None,
        });
        let key = tagged_number(&db, "id", TypeId::STRING);
        assert_eq!(evaluate_index_access(&db, object, key), TypeId::BOOLEAN);
    }

    #[test]
    fn tagged_number_index_without_any_index_signature_stays_deferred() {
        // Negative control: no index signature to resolve through. A generic
        // (intersection) key with nothing to match is kept as a deferred
        // `IndexAccess` rather than collapsed to a concrete value type.
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol_index: None,
            symbol: None,
        });
        let key = tagged_number(&db, "tag", TypeId::NUMBER);
        let result = evaluate_index_access(&db, object, key);
        assert!(
            matches!(db.lookup(result), Some(TypeData::IndexAccess(_, _))),
            "an unmatched tagged-number key must stay deferred, got {result:?}"
        );
    }

    #[test]
    fn tagged_string_does_not_match_numeric_index_signature() {
        // Negative control / asymmetry: a `string & { brand }` key is NOT a
        // number subtype, so it must not resolve through a numeric-only index
        // signature (tsc errors on this). It must not collapse to the numeric
        // value type; it stays a deferred `IndexAccess`.
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
            symbol_index: None,
            symbol: None,
        });
        let brand_atom = db.intern_string("g");
        let brand = db.object(vec![PropertyInfo::new(brand_atom, TypeId::NUMBER)]);
        let tagged_string = db.intersection2(TypeId::STRING, brand);
        let result = evaluate_index_access(&db, object, tagged_string);
        assert_ne!(
            result,
            TypeId::BOOLEAN,
            "a tagged-string key must not match the numeric index signature"
        );
        assert!(
            matches!(db.lookup(result), Some(TypeData::IndexAccess(_, _))),
            "an unmatched tagged-string key must stay deferred, got {result:?}"
        );
    }

    /// A non-numeric index signature whose key spans `symbol` (here a
    /// `string | number | symbol` union, the structural form of `PropertyKey`)
    /// serves symbol-bearing keys, and its `keyof` includes `symbol`. Regression
    /// for the dropped symbol arm (#14315).
    #[test]
    fn property_key_union_index_serves_symbol_and_keyof_includes_symbol() {
        use crate::evaluation::evaluate::evaluate_keyof;

        let db = TypeInterner::new();
        let key = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type: key,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
        });

        // Indexed access by `symbol` resolves through the signature value type.
        assert_eq!(
            evaluate_index_access(&db, object, TypeId::SYMBOL),
            TypeId::NUMBER,
            "a `string | number | symbol` keyed signature must serve a symbol index"
        );
        // A unique-symbol key is a subtype of `symbol`, so it resolves too.
        let uniq = db.unique_symbol(crate::types::SymbolRef(11));
        assert_eq!(
            evaluate_index_access(&db, object, uniq),
            TypeId::NUMBER,
            "a unique-symbol key must serve through the symbol-bearing signature"
        );

        // `keyof` includes the `symbol` arm.
        let keyof = evaluate_keyof(&db, object);
        let includes_symbol = match db.lookup(keyof) {
            Some(TypeData::Union(members)) => db.type_list(members).contains(&TypeId::SYMBOL),
            _ => keyof == TypeId::SYMBOL,
        };
        assert!(
            includes_symbol,
            "keyof of a symbol-bearing index signature must include `symbol`, got {:?}",
            db.lookup(keyof)
        );
    }

    /// A symbol-only index signature serves symbol keys but not bare `string`
    /// (the fix must not over-accept).
    #[test]
    fn symbol_only_index_rejects_string_key() {
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type: TypeId::SYMBOL,
                value_type: TypeId::BOOLEAN,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
        });
        assert_eq!(
            evaluate_index_access(&db, object, TypeId::SYMBOL),
            TypeId::BOOLEAN,
            "a symbol-only signature must serve a symbol index"
        );
        assert_ne!(
            evaluate_index_access(&db, object, TypeId::STRING),
            TypeId::BOOLEAN,
            "a symbol-only signature must not serve a bare string index"
        );
    }
}
