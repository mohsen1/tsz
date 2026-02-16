//! Binary operator error reporting (TS2362, TS2363, TS2365, TS2469).

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Report TS2506: Circular class inheritance (class C extends C).
    pub(crate) fn error_circular_class_inheritance(
        &mut self,
        extends_expr_idx: NodeIndex,
        class_idx: NodeIndex,
    ) {
        // Get the class name for the error message
        let class_name = if let Some(class_node) = self.ctx.arena.get(class_idx)
            && let Some(class) = self.ctx.arena.get_class(class_node)
            && !class.name.is_none()
            && let Some(name_node) = self.ctx.arena.get(class.name)
        {
            self.ctx
                .arena
                .get_identifier(name_node)
                .map(|id| id.escaped_text.clone())
        } else {
            None
        };

        let name = class_name.unwrap_or_else(|| String::from("<class>"));

        let Some(loc) = self.get_source_location(extends_expr_idx) else {
            return;
        };

        let message = format_message(
            diagnostic_messages::IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_BASE_EXPRESSION,
            &[&name],
        );

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_BASE_EXPRESSION,
            category: DiagnosticCategory::Error,
            message_text: message,
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: Vec::new(),
        });
    }

    /// Report TS2507: "Type 'X' is not a constructor function type"
    /// This is for extends clauses where the base type isn't a constructor.
    pub fn error_not_a_constructor_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        // Suppress error if type is ERROR/ANY/UNKNOWN - prevents cascading errors
        if type_id == TypeId::ERROR || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return;
        }

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = self.ctx.create_type_formatter();
        let type_str = formatter.format(type_id);

        let message =
            diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE.replace("{0}", &type_str);

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
            category: DiagnosticCategory::Error,
            message_text: message,
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: Vec::new(),
        });
    }

    /// Report TS2351: "This expression is not constructable. Type 'X' has no construct signatures."
    /// This is for `new` expressions where the expression type has no construct signatures.
    pub fn error_not_constructable_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        if type_id == TypeId::ERROR || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return;
        }

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = self.ctx.create_type_formatter();
        let type_str = formatter.format(type_id);

        let message =
            diagnostic_messages::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE.replace("{0}", &type_str);

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE,
            category: DiagnosticCategory::Error,
            message_text: message,
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: Vec::new(),
        });
    }

    // =========================================================================
    // Binary Operator Errors
    // =========================================================================

    /// Emit errors for binary operator type mismatches.
    /// Emits TS18050 for null/undefined operands, TS2362 for left-hand side,
    /// TS2363 for right-hand side, or TS2365 for general operator errors.
    pub(crate) fn emit_binary_operator_error(
        &mut self,
        node_idx: NodeIndex,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
        op: &str,
    ) {
        // Suppress cascade errors from unresolved types
        if left_type == TypeId::ERROR
            || right_type == TypeId::ERROR
            || left_type == TypeId::UNKNOWN
            || right_type == TypeId::UNKNOWN
        {
            return;
        }

        // Track nullish operands for proper error reporting
        // NOTE: TSC emits TS2365 for '+' operator with null/undefined, but TS18050 for other arithmetic operators
        let left_is_nullish = left_type == TypeId::NULL || left_type == TypeId::UNDEFINED;
        let right_is_nullish = right_type == TypeId::NULL || right_type == TypeId::UNDEFINED;
        let mut emitted_nullish_error = false;

        // TS18050 is only emitted for strictly-arithmetic and bitwise operators with null/undefined operands.
        // The `+` operator is NOT included: tsc emits TS2365 for `null + null`, not TS18050,
        // because `+` can be string concatenation and has its own type-checking path.
        // Relational operators (<, >, <=, >=) also emit TS18050, but only for literal null/undefined.
        // For now, we only handle arithmetic/bitwise since our evaluator doesn't distinguish
        // literal values from variables typed as null/undefined.
        let should_emit_nullish_error = matches!(
            op,
            "-" | "*" | "/" | "%" | "**" | "&" | "|" | "^" | "<<" | ">>" | ">>>"
        );

        // Emit TS18050 for null/undefined operands in arithmetic operations (except +)
        if left_is_nullish && should_emit_nullish_error {
            let value_name = if left_type == TypeId::NULL {
                "null"
            } else {
                "undefined"
            };
            if let Some(loc) = self.get_source_location(left_idx) {
                let message = format_message(
                    diagnostic_messages::THE_VALUE_CANNOT_BE_USED_HERE,
                    &[value_name],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
                emitted_nullish_error = true;
            }
        }

        if right_is_nullish && should_emit_nullish_error {
            let value_name = if right_type == TypeId::NULL {
                "null"
            } else {
                "undefined"
            };
            if let Some(loc) = self.get_source_location(right_idx) {
                let message = format_message(
                    diagnostic_messages::THE_VALUE_CANNOT_BE_USED_HERE,
                    &[value_name],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
                emitted_nullish_error = true;
            }
        }

        // If BOTH operands are null/undefined AND we emitted TS18050 for them, we're done
        if left_is_nullish && right_is_nullish && emitted_nullish_error {
            return;
        }

        use tsz_solver::BinaryOpEvaluator;

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);

        // TS2469: Check if either operand is a symbol type
        // TS2469 is emitted when an operator cannot be applied to type 'symbol'
        // We check both operands and emit TS2469 for the symbol operand(s)
        let left_is_symbol = evaluator.is_symbol_like(left_type);
        let right_is_symbol = evaluator.is_symbol_like(right_type);

        if left_is_symbol || right_is_symbol {
            // Format type strings first to avoid holding formatter across mutable borrows
            let left_type_str =
                left_is_symbol.then(|| self.ctx.create_type_formatter().format(left_type));
            let right_type_str =
                right_is_symbol.then(|| self.ctx.create_type_formatter().format(right_type));

            // Emit TS2469 for symbol operands
            if let (Some(loc), Some(type_str)) =
                (self.get_source_location(left_idx), left_type_str.as_deref())
            {
                let message = format_message(
                    diagnostic_messages::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                    &[op, type_str],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }

            if let (Some(loc), Some(type_str)) = (
                self.get_source_location(right_idx),
                right_type_str.as_deref(),
            ) {
                let message = format_message(
                    diagnostic_messages::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                    &[op, type_str],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }

            // If both are symbols, we're done (no need for TS2365)
            if left_is_symbol && right_is_symbol {
                return;
            }

            // If only one is symbol, continue to check the other operand
            // (but we've already emitted TS2469 for the symbol)
        }

        let mut formatter = self.ctx.create_type_formatter();
        let left_str = formatter.format(left_type);
        let right_str = formatter.format(right_type);

        // Check if this is an arithmetic or bitwise operator
        // These operators require integer operands and emit TS2362/TS2363
        // Note: + is handled separately - it can be string concatenation or arithmetic
        let is_arithmetic = matches!(op, "-" | "*" | "/" | "%" | "**");
        let is_bitwise = matches!(op, "&" | "|" | "^" | "<<" | ">>" | ">>>");
        let requires_numeric_operands = is_arithmetic || is_bitwise;

        // Evaluate types to resolve unevaluated conditional/mapped types before checking.
        // e.g., DeepPartial<number> | number → number
        let eval_left = self.evaluate_type_for_binary_ops(left_type);
        let eval_right = self.evaluate_type_for_binary_ops(right_type);

        // Check if operands have valid arithmetic types using BinaryOpEvaluator
        // This properly handles number, bigint, any, and enum types (unions of number literals)
        // Note: evaluator was already created above for symbol checking
        // Skip arithmetic checks for symbol operands (we already emitted TS2469)
        let left_is_valid_arithmetic =
            !left_is_symbol && evaluator.is_arithmetic_operand(eval_left);
        let right_is_valid_arithmetic =
            !right_is_symbol && evaluator.is_arithmetic_operand(eval_right);

        // For + operator, TSC always emits TS2365 ("Operator '+' cannot be applied to types"),
        // never TS2362/TS2363. This is because + can be either string concatenation or arithmetic,
        // so TSC uses the general error regardless of the operand types.
        if op == "+" {
            if let Some(loc) = self.get_source_location(node_idx) {
                let message = format!(
                    "Operator '{op}' cannot be applied to types '{left_str}' and '{right_str}'."
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }
            return;
        }

        if requires_numeric_operands {
            // For arithmetic and bitwise operators, emit specific left/right errors (TS2362, TS2363)
            // Skip operands that already got TS18050 (null/undefined with strictNullChecks)
            let mut emitted_specific_error = emitted_nullish_error;
            if !left_is_valid_arithmetic
                && (!left_is_nullish || !emitted_nullish_error)
                && let Some(loc) = self.get_source_location(left_idx)
            {
                let message = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                self.ctx.diagnostics.push(Diagnostic {
                        code: diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                        category: DiagnosticCategory::Error,
                        message_text: message,
                        file: self.ctx.file_name.clone(),
                        start: loc.start,
                        length: loc.length(),
                        related_information: Vec::new(),
                    });
                emitted_specific_error = true;
            }
            if !right_is_valid_arithmetic
                && (!right_is_nullish || !emitted_nullish_error)
                && let Some(loc) = self.get_source_location(right_idx)
            {
                let message = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                self.ctx.diagnostics.push(Diagnostic {
                        code: diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                        category: DiagnosticCategory::Error,
                        message_text: message,
                        file: self.ctx.file_name.clone(),
                        start: loc.start,
                        length: loc.length(),
                        related_information: Vec::new(),
                    });
                emitted_specific_error = true;
            }
            // If both operands are valid arithmetic types but the operation still failed
            // (e.g., mixing number and bigint), emit TS2365
            if !emitted_specific_error && let Some(loc) = self.get_source_location(node_idx) {
                let message = format!(
                    "Operator '{op}' cannot be applied to types '{left_str}' and '{right_str}'."
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }
            return;
        }

        // Handle relational operators: <, >, <=, >=
        // These require both operands to be comparable. When types have no relationship,
        // emit TS2365: "Operator '<' cannot be applied to types 'X' and 'Y'."
        let is_relational = matches!(op, "<" | ">" | "<=" | ">=");
        if is_relational {
            if let Some(loc) = self.get_source_location(node_idx) {
                let message = format!(
                    "Operator '{op}' cannot be applied to types '{left_str}' and '{right_str}'."
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }
            return;
        }

        // Handle bitwise operators: &, |, ^, <<, >>, >>>
        let is_bitwise = matches!(op, "&" | "|" | "^" | "<<" | ">>" | ">>>");
        if is_bitwise {
            // TS2447: For &, |, ^ with both boolean operands, emit special error
            let left_is_boolean = evaluator.is_boolean_like(left_type);
            let right_is_boolean = evaluator.is_boolean_like(right_type);
            let is_boolean_bitwise =
                matches!(op, "&" | "|" | "^") && left_is_boolean && right_is_boolean;

            if is_boolean_bitwise {
                let suggestion = if op == "&" {
                    "&&"
                } else if op == "|" {
                    "||"
                } else {
                    "!=="
                };
                if let Some(loc) = self.get_source_location(node_idx) {
                    let message = format!(
                        "The '{op}' operator is not allowed for boolean types. Consider using '{suggestion}' instead."
                    );
                    self.ctx.diagnostics.push(Diagnostic {
                        code: diagnostic_codes::THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD,
                        category: DiagnosticCategory::Error,
                        message_text: message,
                        file: self.ctx.file_name.clone(),
                        start: loc.start,
                        length: loc.length(),
                        related_information: Vec::new(),
                    });
                }
            } else {
                // For other invalid bitwise operands, emit TS2362/TS2363
                let mut emitted_specific_error = emitted_nullish_error;
                if !left_is_valid_arithmetic
                    && (!left_is_nullish || !emitted_nullish_error)
                    && let Some(loc) = self.get_source_location(left_idx)
                {
                    let message = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                    self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                            category: DiagnosticCategory::Error,
                            message_text: message,
                            file: self.ctx.file_name.clone(),
                            start: loc.start,
                            length: loc.length(),
                            related_information: Vec::new(),
                        });
                    emitted_specific_error = true;
                }
                if !right_is_valid_arithmetic
                    && (!right_is_nullish || !emitted_nullish_error)
                    && let Some(loc) = self.get_source_location(right_idx)
                {
                    let message = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                    self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                            category: DiagnosticCategory::Error,
                            message_text: message,
                            file: self.ctx.file_name.clone(),
                            start: loc.start,
                            length: loc.length(),
                            related_information: Vec::new(),
                        });
                    emitted_specific_error = true;
                }
                if !emitted_specific_error && let Some(loc) = self.get_source_location(node_idx) {
                    let message = format!(
                        "Operator '{op}' cannot be applied to types '{left_str}' and '{right_str}'."
                    );
                    self.ctx.diagnostics.push(Diagnostic {
                        code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                        category: DiagnosticCategory::Error,
                        message_text: message,
                        file: self.ctx.file_name.clone(),
                        start: loc.start,
                        length: loc.length(),
                        related_information: Vec::new(),
                    });
                }
            }
        }
    }
}
