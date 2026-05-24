//! Array shorthand formatting helpers.

use super::TypeFormatter;
use crate::types::{TypeData, TypeId};

impl<'a> TypeFormatter<'a> {
    /// Returns `true` when the element type of an array shorthand (`T[]` or
    /// `readonly T[]`) must be wrapped in parentheses to render unambiguously.
    ///
    /// Per the TypeScript grammar, `T[]` requires `T` to be a `PrimaryType`.
    /// These variants all bind looser than postfix `[]`, so rendering them
    /// without parens would let `[]` attach to the inner form.
    pub(super) fn requires_array_element_parens(&self, elem: TypeId) -> bool {
        matches!(
            self.interner.lookup(elem),
            Some(
                TypeData::Union(_)
                    | TypeData::Intersection(_)
                    | TypeData::Function(_)
                    | TypeData::Callable(_)
                    | TypeData::Conditional(_)
                    | TypeData::Infer(_)
                    | TypeData::KeyOf(_)
            )
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::TypeFormatter;
    use crate::construction::TypeInterner;
    use crate::types::{ConditionalType, TypeId, TypeParamInfo};

    #[test]
    fn format_array_of_infer_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("E"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let arr = db.array(infer);
        assert_eq!(fmt.format(arr), "(infer E)[]");
    }

    #[test]
    fn format_array_of_infer_is_parenthesized_with_renamed_binder() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        for name in ["E", "Q", "U", "_T"] {
            let infer = db.infer(TypeParamInfo {
                name: db.intern_string(name),
                constraint: None,
                default: None,
                is_const: false,
            });
            let arr = db.array(infer);
            assert_eq!(fmt.format(arr), format!("(infer {name})[]"));
        }
    }

    #[test]
    fn format_array_of_infer_with_constraint_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("E"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        });
        let arr = db.array(infer);
        assert_eq!(fmt.format(arr), "(infer E extends string)[]");
    }

    #[test]
    fn format_array_of_conditional_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let cond = db.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::NUMBER,
            true_type: TypeId::BOOLEAN,
            false_type: TypeId::NEVER,
            is_distributive: false,
        });
        let arr = db.array(cond);
        let result = fmt.format(arr);
        assert_eq!(
            result, "(string extends number ? boolean : never)[]",
            "Array of conditional should be parenthesized, got: {result}"
        );
    }

    #[test]
    fn format_array_of_keyof_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tp = db.type_param(TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let keyof = db.keyof(tp);
        let arr = db.array(keyof);
        assert_eq!(fmt.format(arr), "(keyof T)[]");
    }

    #[test]
    fn format_array_of_primitive_is_unparenthesized_control() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(db.array(TypeId::NUMBER)), "number[]");
    }

    #[test]
    fn format_readonly_array_of_infer_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("E"),
            constraint: None,
            default: None,
            is_const: false,
        });

        let readonly_array_base = db.unresolved_type_name(db.intern_string("ReadonlyArray"));
        let app = db.application(readonly_array_base, vec![infer]);
        assert_eq!(fmt.format(app), "readonly (infer E)[]");
    }

    #[test]
    fn format_infer_with_constraint_includes_extends() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("E"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        });
        assert_eq!(fmt.format(infer), "infer E extends string");
    }
}
