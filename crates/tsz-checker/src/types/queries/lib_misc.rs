//! Miscellaneous node, predicate, accessibility, and literal query helpers.

use crate::state::{CheckerState, MemberAccessLevel};
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{TypeId, TypePredicateTarget};

impl<'a> CheckerState<'a> {
    // Section 47: Node Predicate Utilities
    // ------------------------------------

    /// Check if a variable declaration is a catch clause variable.
    ///
    /// This function determines if a given variable declaration node is
    /// the variable declaration of a catch clause (try/catch statement).
    ///
    /// ## Catch Clause Variables:
    /// - Catch clause variables have special scoping rules
    /// - They are block-scoped to the catch block
    /// - They shadow variables with the same name in outer scopes
    /// - They cannot be accessed before declaration (TDZ applies)
    ///
    /// ## Examples:
    /// ```typescript
    /// try {
    ///   throw new Error("error");
    /// } catch (e) {
    ///   // e is a catch clause variable
    ///   console.log(e.message);
    /// }
    /// // is_catch_clause_variable_declaration(e_node) → true
    ///
    /// const x = 5;
    /// // is_catch_clause_variable_declaration(x_node) → false
    /// ```
    pub(crate) fn is_catch_clause_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::CATCH_CLAUSE {
            return false;
        }
        let Some(catch) = self.ctx.arena.get_catch_clause(parent_node) else {
            return false;
        };
        catch.variable_declaration == var_decl_idx
    }

    /// Check if a variable declaration is in a `for...in` statement.
    /// For-in loop variables are always typed as `string`.
    ///
    /// AST walk: `VariableDeclaration` → `VariableDeclarationList` → `ForInStatement`
    pub(crate) fn is_for_in_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        // VariableDeclaration → parent (VariableDeclarationList)
        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let vdl_idx = ext.parent;
        if vdl_idx.is_none() {
            return false;
        }
        // VariableDeclarationList → parent (ForInStatement?)
        let Some(vdl_ext) = self.ctx.arena.get_extended(vdl_idx) else {
            return false;
        };
        let for_in_idx = vdl_ext.parent;
        if for_in_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(for_in_idx) else {
            return false;
        };
        parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
    }

    /// Check if a variable declaration is in a `for...in` or `for...of` statement.
    /// These loop variables get their type from the iterable expression, not from
    /// the variable declaration itself.
    ///
    /// AST walk: `VariableDeclaration` → `VariableDeclarationList` → `ForInStatement`/`ForOfStatement`
    pub(crate) fn is_for_in_or_of_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let vdl_idx = ext.parent;
        if vdl_idx.is_none() {
            return false;
        }
        let Some(vdl_ext) = self.ctx.arena.get_extended(vdl_idx) else {
            return false;
        };
        let parent_idx = vdl_ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
            || parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
    }

    // Section 48: Type Predicate Utilities
    // -------------------------------------

    /// Get the target of a type predicate from a parameter name node.
    ///
    /// Type predicates are used in function signatures to narrow types
    /// based on runtime checks. The target can be either `this` or an
    /// identifier parameter name.
    ///
    /// ## Type Predicate Targets:
    /// - **This**: `asserts this is T` - Used in methods to narrow the receiver type
    /// - **Identifier**: `argName is T` - Used to narrow a parameter's type
    ///
    /// ## Examples:
    /// ```typescript
    /// // This type predicate
    /// function assertIsString(this: unknown): asserts this is string {
    ///   if (typeof this === 'string') {
    ///     return; // this is narrowed to string
    ///   }
    ///   throw new Error('Not a string');
    /// }
    /// // type_predicate_target(thisKeywordNode) → TypePredicateTarget::This
    ///
    /// // Identifier type predicate
    /// function isString(val: unknown): val is string {
    ///   return typeof val === 'string';
    /// }
    /// // type_predicate_target(valIdentifierNode) → TypePredicateTarget::Identifier("val")
    /// ```
    pub(crate) fn type_predicate_target(
        &self,
        param_name: NodeIndex,
    ) -> Option<TypePredicateTarget> {
        let node = self.ctx.arena.get(param_name)?;
        if node.kind == SyntaxKind::ThisKeyword as u16 || node.kind == syntax_kind_ext::THIS_TYPE {
            return Some(TypePredicateTarget::This);
        }

        self.ctx.arena.get_identifier(node).map(|ident| {
            TypePredicateTarget::Identifier(self.ctx.types.intern_string(&ident.escaped_text))
        })
    }

    // Section 49: Constructor Accessibility Utilities
    // -----------------------------------------------

    /// Convert a constructor access level to its string representation.
    ///
    /// This function is used for error messages to display the accessibility
    /// level of a constructor (private, protected, or public).
    ///
    /// ## Constructor Accessibility:
    /// - **Private**: `private constructor()` - Only accessible within the class
    /// - **Protected**: `protected constructor()` - Accessible within class and subclasses
    /// - **Public**: `constructor()` or `public constructor()` - Accessible everywhere
    ///
    /// ## Examples:
    /// ```typescript
    /// class Singleton {
    ///   private constructor() {} // Only accessible within Singleton
    /// }
    /// // constructor_access_name(Some(Private)) → "private"
    ///
    /// class Base {
    ///   protected constructor() {} // Accessible in Base and subclasses
    /// }
    /// // constructor_access_name(Some(Protected)) → "protected"
    ///
    /// class Public {
    ///   constructor() {} // Public by default
    /// }
    /// // constructor_access_name(None) → "public"
    /// ```
    pub(crate) const fn constructor_access_name(level: Option<MemberAccessLevel>) -> &'static str {
        match level {
            Some(MemberAccessLevel::Private) => "private",
            Some(MemberAccessLevel::Protected) => "protected",
            None => "public",
        }
    }

    /// Get the numeric rank of a constructor access level.
    ///
    /// This function assigns a numeric value to access levels for comparison:
    /// - Private (2) > Protected (1) > Public (0)
    ///
    /// Higher ranks indicate more restrictive access levels. This is used
    /// to determine if a constructor accessibility mismatch exists between
    /// source and target types.
    ///
    /// ## Rank Ordering:
    /// ```typescript
    /// Private (2)   - Most restrictive
    /// Protected (1) - Medium restrictiveness
    /// Public (0)    - Least restrictive
    /// ```
    ///
    /// ## Examples:
    /// ```typescript
    /// constructor_access_rank(Some(Private))    // → 2
    /// constructor_access_rank(Some(Protected)) // → 1
    /// constructor_access_rank(None)            // → 0 (Public)
    /// ```
    pub(crate) const fn constructor_access_rank(level: Option<MemberAccessLevel>) -> u8 {
        match level {
            Some(MemberAccessLevel::Private) => 2,
            Some(MemberAccessLevel::Protected) => 1,
            None => 0,
        }
    }

    /// Get the excluded symbol flags for a given symbol.
    ///
    /// Each symbol type (function, class, interface, etc.) has specific
    /// flags that represent incompatible symbols that cannot share the same name.
    /// This function returns those exclusion flags.
    ///
    /// ## Symbol Exclusion Rules:
    /// - Functions exclude other functions with the same name
    /// - Classes exclude interfaces with the same name (unless merging)
    /// - Variables exclude other variables with the same name in the same scope
    ///
    /// ## Examples:
    /// ```typescript
    /// // Function exclusions
    /// function foo() {}
    /// function foo() {} // ERROR: Duplicate function declaration
    ///
    /// // Class/Interface merging (allowed)
    /// interface Foo {}
    /// class Foo {} // Allowed: interface and class can merge
    ///
    /// // Variable exclusions
    /// let x = 1;
    /// let x = 2; // ERROR: Duplicate variable declaration
    /// ```
    const fn excluded_symbol_flags(flags: u32) -> u32 {
        if (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0 {
            return symbol_flags::FUNCTION_SCOPED_VARIABLE_EXCLUDES;
        }
        if (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 {
            return symbol_flags::BLOCK_SCOPED_VARIABLE_EXCLUDES;
        }
        if (flags & symbol_flags::FUNCTION) != 0 {
            return symbol_flags::CLASS;
        }
        if (flags & symbol_flags::CLASS) != 0 {
            return symbol_flags::FUNCTION;
        }
        if (flags & symbol_flags::INTERFACE) != 0 {
            return symbol_flags::INTERFACE_EXCLUDES;
        }
        if (flags & symbol_flags::TYPE_ALIAS) != 0 {
            return symbol_flags::TYPE_ALIAS_EXCLUDES;
        }
        if (flags & symbol_flags::REGULAR_ENUM) != 0 {
            return symbol_flags::REGULAR_ENUM_EXCLUDES;
        }
        if (flags & symbol_flags::CONST_ENUM) != 0 {
            return symbol_flags::CONST_ENUM_EXCLUDES;
        }
        // Check NAMESPACE_MODULE before VALUE_MODULE since namespaces have both flags
        // and NAMESPACE_MODULE_EXCLUDES (NONE) allows more merging than VALUE_MODULE_EXCLUDES
        if (flags & symbol_flags::NAMESPACE_MODULE) != 0 {
            return symbol_flags::NAMESPACE_MODULE_EXCLUDES;
        }
        if (flags & symbol_flags::VALUE_MODULE) != 0 {
            return symbol_flags::VALUE_MODULE_EXCLUDES;
        }
        if (flags & symbol_flags::GET_ACCESSOR) != 0 {
            return symbol_flags::GET_ACCESSOR_EXCLUDES;
        }
        if (flags & symbol_flags::SET_ACCESSOR) != 0 {
            return symbol_flags::SET_ACCESSOR_EXCLUDES;
        }
        if (flags & symbol_flags::METHOD) != 0 {
            return symbol_flags::METHOD_EXCLUDES;
        }
        if (flags & symbol_flags::ALIAS) != 0 {
            return symbol_flags::ALIAS_EXCLUDES;
        }
        symbol_flags::NONE
    }

    /// Check if two declarations conflict based on their symbol flags.
    ///
    /// This function determines whether two symbols with the given flags
    /// can coexist in the same scope without conflict.
    ///
    /// ## Conflict Rules:
    /// - **Static vs Instance**: Static and instance members with the same name don't conflict
    /// - **Exclusion Flags**: If either declaration excludes the other's flags, they conflict
    ///
    /// ## Examples:
    /// ```typescript
    /// class Example {
    ///   static x = 1;  // Static member
    ///   x = 2;         // Instance member - no conflict
    /// }
    ///
    /// class Conflict {
    ///   foo() {}      // Method
    ///   foo: number;  // Property - CONFLICT!
    /// }
    ///
    /// interface Merge {
    ///   foo(): void;
    /// }
    /// interface Merge {
    ///   bar(): void;  // No conflict - different members
    /// }
    /// ```
    pub(crate) const fn declarations_conflict(flags_a: u32, flags_b: u32) -> bool {
        // Static and instance members with the same name don't conflict
        let a_is_static = (flags_a & symbol_flags::STATIC) != 0;
        let b_is_static = (flags_b & symbol_flags::STATIC) != 0;
        if a_is_static != b_is_static {
            return false;
        }

        let excludes_a = Self::excluded_symbol_flags(flags_a);
        let excludes_b = Self::excluded_symbol_flags(flags_b);
        (flags_a & excludes_b) != 0 || (flags_b & excludes_a) != 0
    }

    // Section 51: Literal Type Utilities
    // ----------------------------------

    /// Infer a literal type from an initializer expression.
    ///
    /// This function attempts to infer the most specific literal type from an
    /// expression, enabling const declarations to have literal types.
    ///
    /// **Literal Type Inference:**
    /// - **String literals**: `"hello"` → `"hello"` (string literal type)
    /// - **Numeric literals**: `42` → `42` (numeric literal type)
    /// - **Boolean literals**: `true` → `true`, `false` → `false`
    /// - **Null literal**: `null` → null type
    /// - **Unary expressions**: `-42` → `-42`, `+42` → `42`
    ///
    /// **Non-Literal Expressions:**
    /// - Complex expressions return None (not a literal)
    /// - Function calls, object literals, etc. return None
    ///
    /// **Const Declarations:**
    /// - `const x = "hello"` infers type `"hello"` (not `string`)
    /// - `let y = "hello"` infers type `string` (widened)
    /// - This function enables the const behavior
    ///
    /// ## Examples:
    /// ```typescript
    /// // String literal
    /// const greeting = "hello";  // Type: "hello"
    /// literal_type_from_initializer(greeting_node) → Some("hello")
    ///
    /// // Numeric literal
    /// const count = 42;  // Type: 42
    /// literal_type_from_initializer(count_node) → Some(42)
    ///
    /// // Negative number
    /// const temp = -42;  // Type: -42
    /// literal_type_from_initializer(temp_node) → Some(-42)
    ///
    /// // Boolean
    /// const flag = true;  // Type: true
    /// literal_type_from_initializer(flag_node) → Some(true)
    ///
    /// // Non-literal
    /// const arr = [1, 2, 3];  // Type: number[]
    /// literal_type_from_initializer(arr_node) → None
    /// ```
    pub(crate) fn literal_type_from_initializer(&self, idx: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(self.ctx.types.literal_string(&lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                lit.value.map(|value| self.ctx.types.literal_number(value))
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                Some(self.ctx.types.literal_bigint(text))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(self.ctx.types.literal_boolean(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => {
                Some(self.ctx.types.literal_boolean(false))
            }
            k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
            // `undefined` in expression position is parsed as an Identifier with
            // text "undefined".  Treat it as a unit literal for discriminant narrowing.
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.ctx.arena.get_identifier(node)?;
                if ident.escaped_text == "undefined" {
                    Some(TypeId::UNDEFINED)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let operand = unary.operand;
                let operand_node = self.ctx.arena.get(operand)?;
                if operand_node.kind == SyntaxKind::BigIntLiteral as u16 {
                    let lit = self.ctx.arena.get_literal(operand_node)?;
                    let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                    let negative = op == SyntaxKind::MinusToken as u16;
                    return Some(self.ctx.types.literal_bigint_with_sign(negative, text));
                }
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = self.ctx.arena.get_literal(operand_node)?;
                let value = lit.value?;
                let value = if op == SyntaxKind::MinusToken as u16 {
                    -value
                } else {
                    value
                };
                Some(self.ctx.types.literal_number(value))
            }
            k if k == tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.ctx.arena.get_binary_expr(node)?;
                if binary.operator_token == tsz_scanner::SyntaxKind::CommaToken as u16 {
                    return self.literal_type_from_initializer(binary.right);
                }
                if binary.operator_token == tsz_scanner::SyntaxKind::AmpersandAmpersandToken as u16
                {
                    let left_ty = self.literal_type_from_initializer(binary.left);
                    let right_ty = self.literal_type_from_initializer(binary.right);
                    if let (Some(l), Some(r)) = (left_ty, right_ty) {
                        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(
                            self.ctx.types,
                        );
                        if let crate::query_boundaries::type_computation::core::BinaryOpResult::Success(res) =
                            evaluator.evaluate(l, r, "&&")
                        {
                            return Some(res);
                        }
                    }
                }
                if binary.operator_token == tsz_scanner::SyntaxKind::BarBarToken as u16 {
                    let left_ty = self.literal_type_from_initializer(binary.left);
                    let right_ty = self.literal_type_from_initializer(binary.right);
                    if let (Some(l), Some(r)) = (left_ty, right_ty) {
                        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(
                            self.ctx.types,
                        );
                        if let crate::query_boundaries::type_computation::core::BinaryOpResult::Success(res) =
                            evaluator.evaluate(l, r, "||")
                        {
                            return Some(res);
                        }
                    }
                }
                None
            }
            k if k == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if self.paren_has_jsdoc_type_cast(idx) {
                    return None;
                }
                let paren = self.ctx.arena.get_parenthesized(node)?;
                self.literal_type_from_initializer(paren.expression)
            }
            k if k == tsz_parser::parser::syntax_kind_ext::AS_EXPRESSION
                || k == tsz_parser::parser::syntax_kind_ext::TYPE_ASSERTION =>
            {
                let assertion = self.ctx.arena.get_type_assertion(node)?;
                if self.is_const_assertion_type_node(assertion.type_node) {
                    self.literal_type_from_initializer(assertion.expression)
                } else {
                    None
                }
            }
            k if k == tsz_parser::parser::syntax_kind_ext::TEMPLATE_EXPRESSION => {
                let template = self.ctx.arena.get_template_expr(node)?;
                // Get the head text (text before the first ${})
                let head_text = self
                    .ctx
                    .arena
                    .get(template.head)
                    .and_then(|n| self.ctx.arena.get_literal(n))
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let mut result = head_text;
                // For each span, try to evaluate the expression to a string literal
                for &span_idx in &template.template_spans.nodes {
                    let span_node = self.ctx.arena.get(span_idx)?;
                    let span = self.ctx.arena.get_template_span(span_node)?;
                    // Recursively evaluate the expression inside ${}
                    let expr_type = self.literal_type_from_initializer(span.expression)?;
                    // Stringify the literal type (handles string, number, bigint,
                    // boolean, null, undefined — not just string literals)
                    let expr_str = crate::query_boundaries::common::stringify_literal_type(
                        self.ctx.types,
                        expr_type,
                    )?;
                    result.push_str(&expr_str);
                    // Get the text after this expression (middle or tail)
                    let tail_text = self
                        .ctx
                        .arena
                        .get(span.literal)
                        .and_then(|n| self.ctx.arena.get_literal(n))
                        .map(|lit| lit.text.clone())
                        .unwrap_or_default();
                    result.push_str(&tail_text);
                }
                Some(self.ctx.types.literal_string(&result))
            }
            _ => None,
        }
    }
}
