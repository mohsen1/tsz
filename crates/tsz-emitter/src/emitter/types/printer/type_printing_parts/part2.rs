#[cfg(test)]
mod tests {
    use tsz_solver::DefId;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::types::{IntrinsicKind, TupleElement, TypeId, TypeParamInfo};

    use super::TypePrinter;

    #[test]
    fn unscoped_type_parameter_prints_constraint_or_unknown() {
        let interner = TypeInterner::new();
        let s = interner.intern_string("S");

        let unconstrained = interner.type_param(TypeParamInfo {
            name: s,
            constraint: None,
            default: None,
            is_const: false,
        });
        assert_eq!(
            TypePrinter::new(&interner).print_type(unconstrained),
            "unknown"
        );

        let constrained = interner.type_param(TypeParamInfo {
            name: s,
            constraint: Some(TypeId::NUMBER),
            default: None,
            is_const: false,
        });
        assert_eq!(
            TypePrinter::new(&interner).print_type(constrained),
            "number"
        );

        assert_eq!(
            TypePrinter::replace_type_param_name_with_any("S[]", "S"),
            "any[]"
        );
    }

    #[test]
    fn type_param_intersection_with_empty_object_prints_as_non_nullable() {
        // Regression: tsc's truthy-narrowing of a type-parameter-typed
        // value yields `T & {}` structurally and renders it as the
        // alias `NonNullable<T>`. tsz constructs the same intersection
        // in narrowing without storing the alias on every code path,
        // so the printer must recover the spelling from the structural
        // shape (mirroring the diagnostic compound formatter).
        let interner = TypeInterner::new();
        let t_atom = interner.intern_string("T");
        let t = interner.type_param(TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
        });
        let empty = interner.object(Vec::new());

        // Mark `T` as visible in the printer scope so it renders as `T`
        // rather than its `unknown` fallback for unscoped type parameters.
        let printer = TypePrinter::new(&interner).with_outer_type_params(vec![t_atom]);
        let intersection = interner.intersection2(t, empty);
        assert_eq!(printer.print_type(intersection), "NonNullable<T>");

        let printer = TypePrinter::new(&interner).with_outer_type_params(vec![t_atom]);
        let intersection_swapped = interner.intersection2(empty, t);
        assert_eq!(printer.print_type(intersection_swapped), "NonNullable<T>");
    }

    #[test]
    fn boxed_object_intersection_prints_primitive_surface() {
        let interner = TypeInterner::new();
        let object_def = DefId(1);
        let object_type = interner.lazy(object_def);
        interner.set_boxed_type(IntrinsicKind::Object, object_type);
        interner.register_boxed_def_id(IntrinsicKind::Object, object_def);

        let text = TypePrinter::new(&interner)
            .print_type(interner.intersection(vec![object_type, TypeId::STRING]));
        assert_eq!(text, "string");

        let literal = interner.literal_string("def");
        let text = TypePrinter::new(&interner)
            .print_type(interner.intersection(vec![object_type, literal]));
        assert_eq!(text, "\"def\"");
    }

    #[test]
    fn mapped_constraint_trims_parser_recovered_as_keyword() {
        assert_eq!(
            TypePrinter::trim_mapped_constraint_trailing_as("T[number]as"),
            "T[number]"
        );
        assert_eq!(
            TypePrinter::trim_mapped_constraint_trailing_as("T[number] as"),
            "T[number]"
        );
        assert_eq!(
            TypePrinter::trim_mapped_constraint_trailing_as("Alias"),
            "Alias"
        );
        assert_eq!(
            TypePrinter::split_recovered_mapped_as_clause("T[number]as Item[Attr]"),
            Some(("T[number]", "Item[Attr]"))
        );
        assert_eq!(
            TypePrinter::mapped_name_type_text("as `get${Capitalize<string & K>}`"),
            "`get${Capitalize<string & K>}`"
        );
        assert_eq!(
            TypePrinter::mapped_name_type_text("as as `get${Capitalize<string & K>}`"),
            "`get${Capitalize<string & K>}`"
        );
        assert_eq!(TypePrinter::mapped_name_type_text("asserts T"), "asserts T");
    }

    #[test]
    fn optional_param_display_omits_synthesized_primitive_undefined() {
        let interner = TypeInterner::new();
        let separator = interner.intern_string("separator");
        let ty = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "join",
            false,
            &[],
            &[tsz_solver::ParamInfo::optional(separator, ty)],
            None,
            TypeId::STRING,
        );
        assert_eq!(printed, "join(separator?: string): string");
    }

    #[test]
    fn optional_param_display_preserves_callback_undefined() {
        let interner = TypeInterner::new();
        let compare_fn = interner.intern_string("compareFn");
        let callback = interner.function(tsz_solver::FunctionShape::new(
            vec![
                tsz_solver::ParamInfo::required(interner.intern_string("a"), TypeId::NUMBER),
                tsz_solver::ParamInfo::required(interner.intern_string("b"), TypeId::NUMBER),
            ],
            TypeId::NUMBER,
        ));
        let ty = interner.union2(callback, TypeId::UNDEFINED);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "sort",
            false,
            &[],
            &[tsz_solver::ParamInfo::optional(compare_fn, ty)],
            None,
            TypeId::VOID,
        );
        assert_eq!(
            printed,
            "sort(compareFn?: ((a: number, b: number) => number) | undefined): void"
        );
    }

    #[test]
    fn optional_param_display_preserves_explicit_union_origin() {
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let ty = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
        interner.replace_union_origin_for_display(ty, vec![TypeId::STRING, TypeId::UNDEFINED]);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "set",
            false,
            &[],
            &[tsz_solver::ParamInfo::optional(value, ty)],
            None,
            TypeId::VOID,
        );
        assert_eq!(printed, "set(value?: string | undefined): void");
    }

    #[test]
    fn optional_param_display_strips_plain_type_param_undefined() {
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let value = interner.intern_string("value");
        let t_param = tsz_solver::types::TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        };
        let t_type = interner.type_param(t_param);
        let ty = interner.union2(t_type, TypeId::UNDEFINED);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "set",
            false,
            &[t_param],
            &[tsz_solver::ParamInfo::optional(value, ty)],
            None,
            TypeId::VOID,
        );
        assert_eq!(printed, "set<T>(value?: T): void");
    }

    #[test]
    fn optional_param_display_preserves_defaulted_type_param_undefined() {
        let interner = TypeInterner::new();
        let this_name = interner.intern_string("This");
        let this_arg = interner.intern_string("thisArg");
        let this_param = tsz_solver::types::TypeParamInfo {
            name: this_name,
            constraint: None,
            default: Some(TypeId::UNDEFINED),
            is_const: false,
        };
        let this_type = interner.type_param(this_param);
        let ty = interner.union2(this_type, TypeId::UNDEFINED);
        let printed = TypePrinter::new(&interner).print_method_signature(
            "flatMap",
            false,
            &[this_param],
            &[tsz_solver::ParamInfo::optional(this_arg, ty)],
            None,
            TypeId::VOID,
        );
        assert_eq!(
            printed,
            "flatMap<This = undefined>(thisArg?: This | undefined): void"
        );
    }

    #[test]
    fn labeled_tuple_typeids_print_compact_even_with_indent() {
        // Declaration AST tuple nodes own source trivia such as member JSDoc and
        // choose multiline output there. Solver tuple `TypeId`s only carry the
        // public tuple shape, so labels alone should not force multiline text.
        let interner = TypeInterner::new();
        let elem = interner.intern_string("elem");
        let index = interner.intern_string("index");
        let tuple = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::OBJECT,
                name: Some(elem),
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::NUMBER,
                name: Some(index),
                optional: false,
                rest: false,
            },
        ]);

        let printed = TypePrinter::new(&interner)
            .with_indent_level(1)
            .print_type(tuple);
        assert_eq!(printed, "[elem: object, index: number]");

        let nested = interner.tuple(vec![TupleElement {
            type_id: tuple,
            name: None,
            optional: false,
            rest: false,
        }]);
        let printed = TypePrinter::new(&interner)
            .with_indent_level(1)
            .print_type(nested);
        assert_eq!(printed, "[[elem: object, index: number]]");
    }
}
