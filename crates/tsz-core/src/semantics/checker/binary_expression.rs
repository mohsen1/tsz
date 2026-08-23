use crate::bind::ScopeId;
use crate::program::SemanticCompletion;
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{
    Completion, DeferredBinaryOperator, DeferredLogicalOperator, DeferredType, TypeId, TypeKind,
};
use crate::source::{FileId, Span};
use crate::syntax::{BinaryOperator, Expression, ExpressionKind};

use super::Checker;

pub(super) struct BinaryEvaluation {
    pub(super) value: Option<TypeId>,
    pub(super) completion: SemanticCompletion,
    diagnostics: BinaryDiagnostics,
}

#[derive(Default)]
struct BinaryDiagnostics {
    boolean_bitwise: bool,
    invalid_left: bool,
    invalid_right: bool,
    incompatible: Option<(TypeId, TypeId)>,
}

impl BinaryEvaluation {
    fn into_completion(self) -> Completion<TypeId> {
        if let Some(value) = self.value {
            return Completion::Complete(value);
        }
        match self.completion {
            SemanticCompletion::Deferred => Completion::Deferred,
            SemanticCompletion::Cycle => Completion::Cycle,
            SemanticCompletion::Limit => Completion::Limit,
            SemanticCompletion::Complete => {
                unreachable!("a complete binary evaluation has a value")
            }
        }
    }
}

const fn deferred_operator(operator: BinaryOperator) -> Option<DeferredBinaryOperator> {
    match operator {
        BinaryOperator::Add => Some(DeferredBinaryOperator::Add),
        BinaryOperator::Subtract => Some(DeferredBinaryOperator::Subtract),
        BinaryOperator::Multiply => Some(DeferredBinaryOperator::Multiply),
        BinaryOperator::Divide => Some(DeferredBinaryOperator::Divide),
        BinaryOperator::Remainder => Some(DeferredBinaryOperator::Remainder),
        BinaryOperator::BitwiseAnd => Some(DeferredBinaryOperator::BitwiseAnd),
        BinaryOperator::BitwiseOr => Some(DeferredBinaryOperator::BitwiseOr),
        BinaryOperator::LessThan
        | BinaryOperator::LessThanEquals
        | BinaryOperator::GreaterThan
        | BinaryOperator::GreaterThanEquals
        | BinaryOperator::Equals
        | BinaryOperator::NotEquals
        | BinaryOperator::StrictEquals
        | BinaryOperator::StrictNotEquals
        | BinaryOperator::LogicalAnd
        | BinaryOperator::LogicalOr
        | BinaryOperator::NullishCoalesce
        | BinaryOperator::In
        | BinaryOperator::InstanceOf => None,
    }
}

const LEFT_ARITHMETIC_MESSAGE: &str = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.";
const RIGHT_ARITHMETIC_MESSAGE: &str = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.";

impl Checker<'_> {
    pub(super) fn infer_authored_binary_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) -> TypeId {
        let ExpressionKind::Binary {
            left,
            operator,
            right,
            operator_span,
        } = &expression.kind
        else {
            unreachable!("binary inference requires a binary expression")
        };
        let left_type = self.infer_expression(file, scope, left, None);
        let right_type = self.infer_expression(file, scope, right, None);
        if let Some(operator) = deferred_operator(*operator) {
            return self.infer_binary_expression(
                file,
                operator,
                left_type,
                right_type,
                [left.span, right.span, *operator_span, expression.span],
            );
        }
        let logical = match operator {
            BinaryOperator::LogicalAnd => Some(DeferredLogicalOperator::And),
            BinaryOperator::LogicalOr => Some(DeferredLogicalOperator::Or),
            BinaryOperator::NullishCoalesce => Some(DeferredLogicalOperator::Nullish),
            _ => None,
        };
        if let Some(operator) = logical {
            return self.store.intern(TypeKind::Deferred(DeferredType::Logical {
                operator,
                left: left_type,
                right: right_type,
            }));
        }
        self.store.builtins.boolean
    }
    fn infer_binary_expression(
        &mut self,
        file: FileId,
        operator: DeferredBinaryOperator,
        left: TypeId,
        right: TypeId,
        [left_span, right_span, operator_span, expression_span]: [Span; 4],
    ) -> TypeId {
        let evaluation = self.evaluate_binary(operator, left, right, 0);
        self.report_binary_diagnostics(
            file,
            operator,
            [left_span, right_span, operator_span, expression_span],
            &evaluation.diagnostics,
        );
        self.observe_completion(evaluation.completion);
        evaluation.value.unwrap_or_else(|| {
            self.store.intern(TypeKind::Deferred(DeferredType::Binary {
                operator,
                left,
                right,
            }))
        })
    }

    pub(super) fn evaluate_binary(
        &mut self,
        operator: DeferredBinaryOperator,
        left: TypeId,
        right: TypeId,
        depth: usize,
    ) -> BinaryEvaluation {
        let [left_forced, right_forced] = self.force_operands([left, right], depth);
        let (left, left_completion) = self.binary_operand(left, left_forced);
        let (right, right_completion) = self.binary_operand(right, right_forced);
        let operand_completion = left_completion.combine(right_completion);
        if operator == DeferredBinaryOperator::Add
            && !operand_completion.is_complete()
            && (type_is_string(&self.store, left) || type_is_string(&self.store, right))
        {
            return BinaryEvaluation {
                value: Some(self.store.builtins.string),
                completion: operand_completion,
                diagnostics: BinaryDiagnostics::default(),
            };
        }
        let (Some(left), Some(right)) = (left, right) else {
            return BinaryEvaluation {
                value: None,
                completion: operand_completion,
                diagnostics: BinaryDiagnostics::default(),
            };
        };
        let left_kind = self.store.kind(left);
        let right_kind = self.store.kind(right);
        let mut diagnostics = BinaryDiagnostics {
            boolean_bitwise: matches!(
                operator,
                DeferredBinaryOperator::BitwiseAnd | DeferredBinaryOperator::BitwiseOr
            ) && is_boolean_like(left_kind)
                && is_boolean_like(right_kind),
            ..BinaryDiagnostics::default()
        };
        let mut value = if diagnostics.boolean_bitwise {
            Some(self.store.builtins.number)
        } else if operator == DeferredBinaryOperator::Add {
            if ((is_string_like(left_kind) || is_string_like(right_kind))
                && (is_string_add_operand(left_kind) || is_error_sentinel(left_kind))
                && (is_string_add_operand(right_kind) || is_error_sentinel(right_kind)))
                || is_any_never_pair(left_kind, right_kind)
            {
                Some(self.store.builtins.string)
            } else if is_error_sentinel(left_kind) {
                Some(left)
            } else if is_error_sentinel(right_kind) {
                Some(right)
            } else if is_number_never_pair(left_kind, right_kind) {
                Some(self.store.builtins.number)
            } else if is_bigint_never_pair(left_kind, right_kind) {
                Some(self.store.builtins.bigint)
            } else if matches!(left_kind, TypeKind::Any) || matches!(right_kind, TypeKind::Any) {
                Some(self.store.builtins.any)
            } else {
                None
            }
        } else if is_error_sentinel(left_kind) {
            Some(left)
        } else if is_error_sentinel(right_kind) {
            Some(right)
        } else if is_number_pair(left_kind, right_kind) || is_any_never_pair(left_kind, right_kind)
        {
            Some(self.store.builtins.number)
        } else if is_bigint_pair(left_kind, right_kind) {
            Some(self.store.builtins.bigint)
        } else {
            None
        };
        if value.is_none() && is_number_bigint_mismatch(left_kind, right_kind) {
            diagnostics.incompatible = Some((left, right));
            value = Some(if operator == DeferredBinaryOperator::Add {
                self.store.builtins.any
            } else {
                self.store.builtins.error
            });
        } else if value.is_none() && operator != DeferredBinaryOperator::Add {
            diagnostics.invalid_left = is_known_invalid_arithmetic(left_kind);
            diagnostics.invalid_right = is_known_invalid_arithmetic(right_kind);
            if diagnostics.invalid_left || diagnostics.invalid_right {
                if matches!(left_kind, TypeKind::BigInt) || matches!(right_kind, TypeKind::BigInt) {
                    diagnostics.incompatible = Some((left, right));
                    value = Some(self.store.builtins.error);
                } else {
                    value = Some(self.store.builtins.number);
                }
            }
        }
        BinaryEvaluation {
            value,
            completion: if value.is_some() {
                operand_completion
            } else {
                operand_completion.combine(SemanticCompletion::Deferred)
            },
            diagnostics,
        }
    }

    fn report_binary_diagnostics(
        &mut self,
        file: FileId,
        operator: DeferredBinaryOperator,
        [left_span, right_span, operator_span, expression_span]: [Span; 4],
        diagnostics: &BinaryDiagnostics,
    ) {
        if diagnostics.boolean_bitwise {
            let suggested = if operator == DeferredBinaryOperator::BitwiseAnd {
                "&&"
            } else {
                "||"
            };
            self.push_diagnostic(file, operator_span, format!("The '{}' operator is not allowed for boolean types. Consider using '{}' instead.", operator_text(operator), suggested), 2447);
            return;
        }
        if diagnostics.invalid_left {
            self.push_diagnostic(file, left_span, LEFT_ARITHMETIC_MESSAGE.to_string(), 2362);
        }
        if diagnostics.invalid_right {
            self.push_diagnostic(file, right_span, RIGHT_ARITHMETIC_MESSAGE.to_string(), 2363);
        }
        if let Some((left, right)) = diagnostics.incompatible {
            let message = format!(
                "Operator '{}' cannot be applied to types '{}' and '{}'.",
                operator_text(operator),
                diagnostic_type_name(&self.store, operator, left),
                diagnostic_type_name(&self.store, operator, right)
            );
            self.push_diagnostic(file, expression_span, message, 2365);
        }
    }

    fn binary_operand(
        &mut self,
        original: TypeId,
        forced: Completion<TypeId>,
    ) -> (Option<TypeId>, SemanticCompletion) {
        match forced {
            Completion::Complete(value) => (Some(value), SemanticCompletion::Complete),
            Completion::Deferred if self.is_bigint_operand(original, 0) => (
                Some(self.store.builtins.bigint),
                SemanticCompletion::Complete,
            ),
            Completion::Deferred => (None, SemanticCompletion::Deferred),
            Completion::Cycle => (None, SemanticCompletion::Cycle),
            Completion::Limit => (None, SemanticCompletion::Limit),
        }
    }

    fn is_bigint_operand(&mut self, operand: TypeId, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        match self.store.kind(operand).clone() {
            TypeKind::BigInt | TypeKind::Deferred(DeferredType::BigIntLiteral) => true,
            TypeKind::Deferred(DeferredType::Value(declaration)) => {
                match self.declaration_value_type(declaration) {
                    Completion::Complete(value) if value != operand => {
                        self.is_bigint_operand(value, depth + 1)
                    }
                    Completion::Complete(_)
                    | Completion::Deferred
                    | Completion::Cycle
                    | Completion::Limit => false,
                }
            }
            _ => false,
        }
    }

    pub(super) fn force_binary(
        &mut self,
        operator: DeferredBinaryOperator,
        left: TypeId,
        right: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let evaluation = self.evaluate_binary(operator, left, right, depth);
        self.observe_completion(evaluation.completion);
        evaluation.into_completion()
    }

    pub(super) fn evaluate_logical(
        &mut self,
        operator: DeferredLogicalOperator,
        left: TypeId,
        right: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let left = match self.force_type(left, depth) {
            Completion::Complete(left) => left,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        let left_kind = self.store.kind(left);
        if matches!(left_kind, TypeKind::Error | TypeKind::Invalid(_)) {
            return Completion::Complete(left);
        }
        match operator {
            DeferredLogicalOperator::And => match known_truthiness(left_kind) {
                Some(true) => Completion::Complete(right),
                Some(false) => Completion::Complete(left),
                None => Completion::Deferred,
            },
            DeferredLogicalOperator::Or => match known_truthiness(left_kind) {
                Some(true) => Completion::Complete(left),
                Some(false) => Completion::Complete(right),
                None => Completion::Deferred,
            },
            DeferredLogicalOperator::Nullish => {
                if matches!(left_kind, TypeKind::Null | TypeKind::Undefined) {
                    Completion::Complete(right)
                } else if matches!(
                    left_kind,
                    TypeKind::Boolean
                        | TypeKind::Number
                        | TypeKind::String
                        | TypeKind::BigInt
                        | TypeKind::ObjectKeyword
                        | TypeKind::Symbol
                        | TypeKind::LiteralBoolean(_, _)
                        | TypeKind::LiteralNumber(_, _)
                        | TypeKind::LiteralString(_, _)
                        | TypeKind::Array(_)
                        | TypeKind::Tuple(_)
                        | TypeKind::Object(_)
                        | TypeKind::Function(_)
                ) {
                    Completion::Complete(left)
                } else {
                    Completion::Deferred
                }
            }
        }
    }
}

fn type_is_string(store: &crate::semantics::types::TypeStore, ty: Option<TypeId>) -> bool {
    let Some(ty) = ty else {
        return false;
    };
    is_string_like(store.kind(ty))
}

const fn is_number_like(kind: &TypeKind) -> bool {
    matches!(kind, TypeKind::Number | TypeKind::LiteralNumber(_, _))
}

const fn is_boolean_like(kind: &TypeKind) -> bool {
    matches!(kind, TypeKind::Boolean | TypeKind::LiteralBoolean(_, _))
}

const fn is_number_bigint_mismatch(left: &TypeKind, right: &TypeKind) -> bool {
    is_number_like(left) && matches!(right, TypeKind::BigInt)
        || is_number_like(right) && matches!(left, TypeKind::BigInt)
}

const fn is_known_invalid_arithmetic(kind: &TypeKind) -> bool {
    matches!(
        kind,
        TypeKind::Boolean
            | TypeKind::String
            | TypeKind::LiteralBoolean(_, _)
            | TypeKind::LiteralString(_, _)
    )
}

fn diagnostic_type_name(
    store: &crate::semantics::types::TypeStore,
    operator: DeferredBinaryOperator,
    ty: TypeId,
) -> String {
    match (operator, store.kind(ty)) {
        (op, TypeKind::LiteralString(_, _)) if op != DeferredBinaryOperator::Add => "string".into(),
        (op, TypeKind::LiteralNumber(_, _)) if op != DeferredBinaryOperator::Add => "number".into(),
        (op, TypeKind::LiteralBoolean(_, _)) if op != DeferredBinaryOperator::Add => {
            "boolean".into()
        }
        _ => store.display(ty),
    }
}

const fn operator_text(operator: DeferredBinaryOperator) -> &'static str {
    match operator {
        DeferredBinaryOperator::Add => "+",
        DeferredBinaryOperator::Subtract => "-",
        DeferredBinaryOperator::Multiply => "*",
        DeferredBinaryOperator::Divide => "/",
        DeferredBinaryOperator::Remainder => "%",
        DeferredBinaryOperator::BitwiseAnd => "&",
        DeferredBinaryOperator::BitwiseOr => "|",
    }
}

const fn is_string_like(kind: &TypeKind) -> bool {
    matches!(kind, TypeKind::String | TypeKind::LiteralString(_, _))
}

const fn is_string_add_operand(kind: &TypeKind) -> bool {
    matches!(
        kind,
        TypeKind::Any
            | TypeKind::Null
            | TypeKind::Undefined
            | TypeKind::Boolean
            | TypeKind::Number
            | TypeKind::String
            | TypeKind::BigInt
            | TypeKind::LiteralBoolean(_, _)
            | TypeKind::LiteralNumber(_, _)
            | TypeKind::LiteralString(_, _)
            | TypeKind::Never
    )
}

const fn is_error_sentinel(kind: &TypeKind) -> bool {
    matches!(kind, TypeKind::Error | TypeKind::Invalid(_))
}

const fn is_any_number_pair(left: &TypeKind, right: &TypeKind) -> bool {
    matches!(left, TypeKind::Any) && (matches!(right, TypeKind::Any) || is_number_like(right))
        || matches!(right, TypeKind::Any) && is_number_like(left)
}

const fn is_any_never_pair(left: &TypeKind, right: &TypeKind) -> bool {
    matches!(left, TypeKind::Any) && matches!(right, TypeKind::Never)
        || matches!(right, TypeKind::Any) && matches!(left, TypeKind::Never)
}

const fn is_number_pair(left: &TypeKind, right: &TypeKind) -> bool {
    is_number_never_pair(left, right) || is_any_number_pair(left, right)
}

const fn is_number_never_pair(left: &TypeKind, right: &TypeKind) -> bool {
    is_number_like(left) && (is_number_like(right) || matches!(right, TypeKind::Never))
        || is_number_like(right) && matches!(left, TypeKind::Never)
        || matches!(left, TypeKind::Never) && matches!(right, TypeKind::Never)
}

const fn is_bigint_pair(left: &TypeKind, right: &TypeKind) -> bool {
    is_bigint_never_pair(left, right) || is_any_bigint_pair(left, right)
}

const fn is_bigint_never_pair(left: &TypeKind, right: &TypeKind) -> bool {
    matches!(left, TypeKind::BigInt) && matches!(right, TypeKind::BigInt | TypeKind::Never)
        || matches!(right, TypeKind::BigInt) && matches!(left, TypeKind::Never)
}

const fn is_any_bigint_pair(left: &TypeKind, right: &TypeKind) -> bool {
    matches!(left, TypeKind::Any) && matches!(right, TypeKind::BigInt)
        || matches!(right, TypeKind::Any) && matches!(left, TypeKind::BigInt)
}

fn known_truthiness(kind: &TypeKind) -> Option<bool> {
    match kind {
        TypeKind::Null | TypeKind::Undefined | TypeKind::LiteralBoolean(false, _) => Some(false),
        TypeKind::LiteralBoolean(true, _)
        | TypeKind::Array(_)
        | TypeKind::Tuple(_)
        | TypeKind::Object(_)
        | TypeKind::Function(_)
        | TypeKind::ShapeFunction(_) => Some(true),
        TypeKind::LiteralString(value, _) => Some(!value.is_empty()),
        TypeKind::LiteralNumber(value, _) => Some(value.is_truthy()),
        _ => None,
    }
}
