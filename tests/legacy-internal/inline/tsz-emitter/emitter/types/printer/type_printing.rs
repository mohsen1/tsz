//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/types/printer/type_printing.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 85d49bd8090cddb2e25ab5765914f22db5826792c75a142f72df0a49d4a80119 1481 unscoped_type_parameter_prints_constraint_or_unknown
    #[test]
    fn unscoped_type_parameter_prints_constraint_or_unknown() {
        let interner = TypeInterner::new();
        let s = interner.intern_string("S");

        let unconstrained = interner.type_param(TypeParamInfo {
            name: s,
            constraint: None,
            default: None,
            is_const: false,
            origin: tsz_solver::types::TypeParamOrigin::User,
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
            origin: tsz_solver::types::TypeParamOrigin::User,
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
// TSZ_INLINE_TEST_END 85d49bd8090cddb2e25ab5765914f22db5826792c75a142f72df0a49d4a80119

// TSZ_INLINE_TEST_BEGIN b401c4c934d1e32ea0debd2840a7cb0e1026bc2b807f16d99e5b9cb6ccedad1b 1516 type_param_intersection_with_empty_object_prints_as_non_nullable
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
            origin: tsz_solver::types::TypeParamOrigin::User,
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
// TSZ_INLINE_TEST_END b401c4c934d1e32ea0debd2840a7cb0e1026bc2b807f16d99e5b9cb6ccedad1b

// TSZ_INLINE_TEST_BEGIN e9a53bae3caf83b259a630729441b571af04a456933dcb141fb00e2b58642cb0 1546 boxed_object_intersection_prints_primitive_surface
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
// TSZ_INLINE_TEST_END e9a53bae3caf83b259a630729441b571af04a456933dcb141fb00e2b58642cb0

// TSZ_INLINE_TEST_BEGIN 057c5cfd1bdec73c238d116038f3a8abe6f8e90e79dd0acef2ba436797c8d2ee 1564 degraded_application_base_drops_type_arguments
    #[test]
    fn degraded_application_base_drops_type_arguments() {
        // A type application whose base cannot be resolved to a nameable
        // reference renders the base as the bare `any` fallback. Appending the
        // type arguments would emit `any<number, string>`, which is not valid
        // TypeScript (`any` is not generic) — the printer must drop them and
        // keep just the fallback, matching tsc's rendering of a truncated or
        // unnameable recursive application.
        let interner = TypeInterner::new();
        let base = interner.lazy(DefId(9999));
        let app = interner.application(base, vec![TypeId::NUMBER, TypeId::STRING]);
        let printed = TypePrinter::new(&interner).print_type(app);
        assert!(
            !printed.contains('<'),
            "degraded base must not carry type arguments, got: {printed}"
        );
        assert_eq!(printed, "any");
    }
// TSZ_INLINE_TEST_END 057c5cfd1bdec73c238d116038f3a8abe6f8e90e79dd0acef2ba436797c8d2ee

// TSZ_INLINE_TEST_BEGIN 3977f0744676226dfbd1ffc5e320068c4ac8914181f4cb86d08c521b90f22a13 1583 base_text_cannot_carry_type_arguments_classifies_degraded_fallbacks
    #[test]
    fn base_text_cannot_carry_type_arguments_classifies_degraded_fallbacks() {
        assert!(TypePrinter::base_text_cannot_carry_type_arguments("any"));
        assert!(TypePrinter::base_text_cannot_carry_type_arguments(
            "unknown"
        ));
        assert!(TypePrinter::base_text_cannot_carry_type_arguments("never"));
        assert!(TypePrinter::base_text_cannot_carry_type_arguments(
            crate::ELIDED_ANY
        ));
        // Real, nameable reference heads must keep their type arguments.
        assert!(!TypePrinter::base_text_cannot_carry_type_arguments("Foo"));
        assert!(!TypePrinter::base_text_cannot_carry_type_arguments(
            "Promise"
        ));
        assert!(!TypePrinter::base_text_cannot_carry_type_arguments(
            "import(\"./m\").Bar"
        ));
        // A member named after a keyword (e.g. `ns.any`) is still a reference.
        assert!(!TypePrinter::base_text_cannot_carry_type_arguments(
            "ns.any"
        ));
    }
// TSZ_INLINE_TEST_END 3977f0744676226dfbd1ffc5e320068c4ac8914181f4cb86d08c521b90f22a13

// TSZ_INLINE_TEST_BEGIN 9906121991fe926d20654368bf51c888d34ccbf807558e7a694f652b18e6b213 1607 mapped_constraint_trims_parser_recovered_as_keyword
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
// TSZ_INLINE_TEST_END 9906121991fe926d20654368bf51c888d34ccbf807558e7a694f652b18e6b213

// TSZ_INLINE_TEST_BEGIN 5fbad56283b101d94a6ffe2e06e7976e517985136360d7caad1b88e8bb8c5000 1636 optional_param_display_omits_synthesized_primitive_undefined
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
            None,
        );
        assert_eq!(printed, "join(separator?: string): string");
    }
// TSZ_INLINE_TEST_END 5fbad56283b101d94a6ffe2e06e7976e517985136360d7caad1b88e8bb8c5000

// TSZ_INLINE_TEST_BEGIN 9d07884e337581788a8f288aff2985b165ca96573134aa4b3ae5965fd734204c 1653 optional_param_display_preserves_callback_undefined
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
            None,
        );
        assert_eq!(
            printed,
            "sort(compareFn?: ((a: number, b: number) => number) | undefined): void"
        );
    }
// TSZ_INLINE_TEST_END 9d07884e337581788a8f288aff2985b165ca96573134aa4b3ae5965fd734204c

// TSZ_INLINE_TEST_BEGIN 20e927e9a8889002bf917d1ed09619c2477e76de341ada799dc1ed1f0887a408 1680 optional_param_display_preserves_explicit_union_origin
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
            None,
        );
        assert_eq!(printed, "set(value?: string | undefined): void");
    }
// TSZ_INLINE_TEST_END 20e927e9a8889002bf917d1ed09619c2477e76de341ada799dc1ed1f0887a408

// TSZ_INLINE_TEST_BEGIN 00c8641a516fa48070f5f5ed6ffeeff4fafbeba4bdca783677aefd4aafcb9949 1698 optional_param_display_strips_plain_type_param_undefined
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
            origin: tsz_solver::types::TypeParamOrigin::User,
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
            None,
        );
        assert_eq!(printed, "set<T>(value?: T): void");
    }
// TSZ_INLINE_TEST_END 00c8641a516fa48070f5f5ed6ffeeff4fafbeba4bdca783677aefd4aafcb9949

// TSZ_INLINE_TEST_BEGIN 946bb7ee3102cd77976439fe78801bed29016af0eed1a9ae46ffd15a90783ed1 1724 optional_param_display_preserves_defaulted_type_param_undefined
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
            origin: tsz_solver::types::TypeParamOrigin::User,
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
            None,
        );
        assert_eq!(
            printed,
            "flatMap<This = undefined>(thisArg?: This | undefined): void"
        );
    }
// TSZ_INLINE_TEST_END 946bb7ee3102cd77976439fe78801bed29016af0eed1a9ae46ffd15a90783ed1

// TSZ_INLINE_TEST_BEGIN 8072b3095f40f6ddf44c980bc60ff92e03d2ab3b189c05d6a78ea7f182636b73 1753 method_signature_prints_explicit_this_parameter
    #[test]
    fn method_signature_prints_explicit_this_parameter() {
        let interner = TypeInterner::new();
        let a = interner.intern_string("a");
        let printed = TypePrinter::new(&interner).print_method_signature(
            "get",
            false,
            &[],
            &[ParamInfo::required(a, TypeId::NUMBER)],
            None,
            TypeId::NUMBER,
            Some(TypeId::STRING),
        );
        assert_eq!(printed, "get(this: string, a: number): number");
    }
// TSZ_INLINE_TEST_END 8072b3095f40f6ddf44c980bc60ff92e03d2ab3b189c05d6a78ea7f182636b73

// TSZ_INLINE_TEST_BEGIN b0cbcd595156563b2448bd005d4b09391151e0348c9fa5ca1f5bc62b0a8c8510 1769 labeled_tuple_typeids_print_compact_even_with_indent
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
// TSZ_INLINE_TEST_END b0cbcd595156563b2448bd005d4b09391151e0348c9fa5ca1f5bc62b0a8c8510
