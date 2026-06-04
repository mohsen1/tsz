#[cfg(test)]
mod array_element_paren_tests {
    use super::DeclarationEmitter;

    fn paren(s: &str) -> String {
        DeclarationEmitter::parenthesize_type_text_in_array_element_position(s)
    }

    // --- Top-level union must be parenthesized in array-element position. ---
    // Rule: postfix `[]` binds tighter than `|`, so `string | number` rendered
    // as an array element becomes `(string | number)[]`, never `string |
    // number[]`. Varies the member spellings to prove it is structural.

    #[test]
    fn top_level_union_is_parenthesized() {
        assert_eq!(paren("string | number"), "(string | number)");
        assert_eq!(
            paren("boolean | bigint | symbol"),
            "(boolean | bigint | symbol)"
        );
    }

    #[test]
    fn top_level_union_with_object_members_is_parenthesized() {
        assert_eq!(paren("{ a: 1 } | { b: 2 }"), "({ a: 1 } | { b: 2 })");
    }

    // --- A union nested inside another constructor must NOT trigger parens. ---
    // `Box<string | number>` is already a `PrimaryType`; the `|` is nested
    // inside the angle brackets, so the array element stays bare.

    #[test]
    fn nested_union_inside_application_is_not_parenthesized() {
        assert_eq!(paren("Box<string | number>"), "Box<string | number>");
        assert_eq!(paren("Map<string, A | B>"), "Map<string, A | B>");
    }

    #[test]
    fn nested_union_inside_tuple_is_not_parenthesized() {
        assert_eq!(
            paren("[string | number, boolean]"),
            "[string | number, boolean]"
        );
    }

    // --- Top-level intersection must be parenthesized. ---

    #[test]
    fn top_level_intersection_is_parenthesized() {
        assert_eq!(paren("A & B"), "(A & B)");
        assert_eq!(paren("{ x: 1 } & { y: 2 }"), "({ x: 1 } & { y: 2 })");
    }

    #[test]
    fn nested_intersection_inside_application_is_not_parenthesized() {
        assert_eq!(paren("Foo<A & B>"), "Foo<A & B>");
    }

    // --- Function / constructor types bind looser than `[]`. ---

    #[test]
    fn function_type_is_parenthesized() {
        assert_eq!(paren("() => void"), "(() => void)");
        assert_eq!(paren("(x: number) => string"), "((x: number) => string)");
    }

    #[test]
    fn constructor_type_is_parenthesized() {
        assert_eq!(paren("new () => Foo"), "(new () => Foo)");
    }

    // --- Conditional / keyof / infer bind looser than `[]`. ---

    #[test]
    fn conditional_type_is_parenthesized() {
        assert_eq!(
            paren("T extends string ? 1 : 0"),
            "(T extends string ? 1 : 0)"
        );
        // Renamed bound variable: proves the rule is not keyed on `T`.
        assert_eq!(
            paren("Elem extends number ? A : B"),
            "(Elem extends number ? A : B)"
        );
    }

    #[test]
    fn keyof_type_is_parenthesized() {
        assert_eq!(paren("keyof T"), "(keyof T)");
        assert_eq!(paren("keyof SomeOther"), "(keyof SomeOther)");
    }

    #[test]
    fn infer_type_is_parenthesized() {
        assert_eq!(paren("infer E"), "(infer E)");
        assert_eq!(paren("infer Q9"), "(infer Q9)");
    }

    // --- Primary types stay bare; already-parenthesized text is untouched. ---

    #[test]
    fn primary_types_stay_bare() {
        assert_eq!(paren("number"), "number");
        assert_eq!(paren("string"), "string");
        assert_eq!(paren("Box<number>"), "Box<number>");
        assert_eq!(paren("[number, string]"), "[number, string]");
        assert_eq!(paren("{ a: number }"), "{ a: number }");
    }

    #[test]
    fn already_parenthesized_text_is_not_double_wrapped() {
        assert_eq!(paren("(string | number)"), "(string | number)");
        assert_eq!(paren("(() => void)"), "(() => void)");
    }

    #[test]
    fn empty_text_is_passed_through() {
        assert_eq!(paren(""), "");
        assert_eq!(paren("   "), "");
    }
}

#[cfg(test)]
mod object_index_signature_rewrite_tests {
    use super::DeclarationEmitter;

    fn rewrite(line: &str) -> Option<String> {
        DeclarationEmitter::object_index_signature_line_with_key(line, "[x: number]:")
    }

    #[test]
    fn rewrites_string_index_key_to_number_key() {
        assert_eq!(
            rewrite("    [x: string]: boolean;").as_deref(),
            Some("    [x: number]: boolean;")
        );
        assert_eq!(
            rewrite("\t[x: string]: Widget;").as_deref(),
            Some("\t[x: number]: Widget;")
        );
    }

    #[test]
    fn preserves_readonly_modifier_and_value_text() {
        assert_eq!(
            rewrite("    readonly [x: string]: Foo | Bar;").as_deref(),
            Some("    readonly [x: number]: Foo | Bar;")
        );
    }

    #[test]
    fn ignores_non_string_index_lines() {
        assert_eq!(rewrite("    [x: number]: boolean;"), None);
        assert_eq!(rewrite("    value: boolean;"), None);
    }

    #[test]
    fn rewrites_index_signature_value_type_without_changing_key() {
        assert_eq!(
            DeclarationEmitter::object_index_signature_line_with_value_type(
                "    [x: string]: Beta | Alpha;",
                "Alpha | Beta",
            )
            .as_deref(),
            Some("    [x: string]: Alpha | Beta;")
        );
        assert_eq!(
            DeclarationEmitter::object_index_signature_line_with_value_type(
                "    readonly [x: number]: Second | First;",
                "First | Second",
            )
            .as_deref(),
            Some("    readonly [x: number]: First | Second;")
        );
    }

    #[test]
    fn preserves_index_signature_spacing_and_suffix() {
        assert_eq!(
            DeclarationEmitter::object_index_signature_line_with_value_type(
                "\t[x: symbol]:   Old | New;  ",
                "New | Old",
            )
            .as_deref(),
            Some("\t[x: symbol]:   New | Old;  ")
        );
    }
}
