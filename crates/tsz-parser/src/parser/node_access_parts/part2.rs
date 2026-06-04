impl Node {
    /// Check if this is an identifier node
    #[inline]
    #[must_use]
    pub const fn is_identifier(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::Identifier as u16
    }

    /// Check if this is a string literal
    #[inline]
    #[must_use]
    pub const fn is_string_literal(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::StringLiteral as u16
    }

    /// Check if this is a numeric literal
    #[inline]
    #[must_use]
    pub const fn is_numeric_literal(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::NumericLiteral as u16
    }

    /// Check if this is a function declaration
    #[inline]
    #[must_use]
    pub const fn is_function_declaration(&self) -> bool {
        use super::syntax_kind_ext::FUNCTION_DECLARATION;
        self.kind == FUNCTION_DECLARATION
    }

    /// Check if this is a class declaration
    #[inline]
    #[must_use]
    pub const fn is_class_declaration(&self) -> bool {
        use super::syntax_kind_ext::CLASS_DECLARATION;
        self.kind == CLASS_DECLARATION
    }

    /// Check if this is any class-like node (class declaration or class expression).
    #[inline]
    #[must_use]
    pub const fn is_class_like(&self) -> bool {
        use super::syntax_kind_ext::{CLASS_DECLARATION, CLASS_EXPRESSION};
        matches!(self.kind, CLASS_DECLARATION | CLASS_EXPRESSION)
    }

    /// Check if this is any kind of function-like node
    #[inline]
    #[must_use]
    pub const fn is_function_like(&self) -> bool {
        matches!(
            self.kind,
            FUNCTION_DECLARATION
                | FUNCTION_EXPRESSION
                | ARROW_FUNCTION
                | METHOD_DECLARATION
                | CONSTRUCTOR
                | GET_ACCESSOR
                | SET_ACCESSOR
        )
    }

    /// Check if this is an anonymous function-valued expression
    /// (`function () {}`, `function name() {}`, or `(a) => {}`).
    #[inline]
    #[must_use]
    pub const fn is_function_expression_or_arrow(&self) -> bool {
        matches!(self.kind, FUNCTION_EXPRESSION | ARROW_FUNCTION)
    }

    /// Check if this is a get or set accessor declaration.
    #[inline]
    #[must_use]
    pub const fn is_accessor(&self) -> bool {
        matches!(self.kind, GET_ACCESSOR | SET_ACCESSOR)
    }

    /// Check if this is a non-arrow function-like node (creates its own `this` binding).
    ///
    /// Arrow functions capture `this` from their enclosing scope and are excluded.
    /// Class bodies also create a `this` scope but are not included here — use
    /// [`is_class_like`] for that boundary check.
    #[inline]
    #[must_use]
    pub const fn is_non_arrow_function_like(&self) -> bool {
        matches!(
            self.kind,
            FUNCTION_DECLARATION
                | FUNCTION_EXPRESSION
                | METHOD_DECLARATION
                | CONSTRUCTOR
                | GET_ACCESSOR
                | SET_ACCESSOR
        )
    }

    /// Check if this is a binding pattern (array or object destructuring)
    #[inline]
    #[must_use]
    pub const fn is_binding_pattern(&self) -> bool {
        self.kind == OBJECT_BINDING_PATTERN || self.kind == ARRAY_BINDING_PATTERN
    }

    /// Check if this is a statement
    #[inline]
    #[must_use]
    pub fn is_statement(&self) -> bool {
        (BLOCK..=DEBUGGER_STATEMENT).contains(&self.kind) || self.kind == VARIABLE_STATEMENT
    }

    /// Check if this is a declaration
    #[inline]
    #[must_use]
    pub fn is_declaration(&self) -> bool {
        (VARIABLE_DECLARATION..=EXPORT_SPECIFIER).contains(&self.kind)
    }

    /// Check if this is a type node
    #[inline]
    #[must_use]
    pub fn is_type_node(&self) -> bool {
        (TYPE_PREDICATE..=IMPORT_TYPE).contains(&self.kind)
    }
}

#[cfg(test)]
mod is_missing_recovery_identifier_tests {
    use super::*;
    use crate::parser::node::NodeArena;
    use tsz_common::interner::Atom;
    use tsz_scanner::SyntaxKind;

    #[test]
    fn returns_true_for_synthesized_recovery_placeholder() {
        let mut arena = NodeArena::with_capacity(8);
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            0,
            IdentifierData {
                atom: Atom::NONE,
                escaped_text: String::new(),
                original_text: None,
                type_arguments: None,
            },
        );
        assert!(arena.is_missing_recovery_identifier(idx));
    }

    #[test]
    fn returns_false_for_real_named_identifier() {
        let mut arena = NodeArena::with_capacity(8);
        // A real identifier has a non-NONE atom AND non-empty escaped_text;
        // either condition alone is enough for the helper to reject it.
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            3,
            IdentifierData {
                atom: Atom(1),
                escaped_text: "foo".to_string(),
                original_text: None,
                type_arguments: None,
            },
        );
        assert!(!arena.is_missing_recovery_identifier(idx));
    }

    #[test]
    fn returns_false_when_only_atom_is_set() {
        let mut arena = NodeArena::with_capacity(8);
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            0,
            IdentifierData {
                atom: Atom(1),
                escaped_text: String::new(),
                original_text: None,
                type_arguments: None,
            },
        );
        assert!(!arena.is_missing_recovery_identifier(idx));
    }

    #[test]
    fn returns_false_when_only_escaped_text_is_set() {
        let mut arena = NodeArena::with_capacity(8);
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            3,
            IdentifierData {
                atom: Atom::NONE,
                escaped_text: "x".to_string(),
                original_text: None,
                type_arguments: None,
            },
        );
        assert!(!arena.is_missing_recovery_identifier(idx));
    }

    #[test]
    fn returns_false_for_non_identifier_node() {
        let arena = NodeArena::with_capacity(8);
        // Default-init NodeIndex points at nothing — get() returns None.
        assert!(!arena.is_missing_recovery_identifier(NodeIndex::NONE));
    }
}
