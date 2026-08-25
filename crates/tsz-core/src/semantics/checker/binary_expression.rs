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

macro_rules! binary_operator_schema {
    ($($syntax:ident => $deferred:ident, $text:literal;)*) => {
        const fn deferred_operator(operator: BinaryOperator) -> Option<DeferredBinaryOperator> {
            match operator {
                $(BinaryOperator::$syntax => Some(DeferredBinaryOperator::$deferred),)*
                _ => None,
            }
        }

        const fn operator_text(operator: DeferredBinaryOperator) -> &'static str {
            match operator {
                $(DeferredBinaryOperator::$deferred => $text,)*
            }
        }
    };
}

binary_operator_schema! {
    Add => Add, "+";
    Subtract => Subtract, "-";
    Multiply => Multiply, "*";
    Divide => Divide, "/";
    Remainder => Remainder, "%";
    BitwiseAnd => BitwiseAnd, "&";
    BitwiseOr => BitwiseOr, "|";
    UnsignedRightShift => UnsignedRightShift, ">>>";
}

impl BinaryEvaluation {
    fn into_completion(self) -> Completion<TypeId> {
        match (self.value, self.completion) {
            (Some(value), _) => Completion::Complete(value),
            (None, SemanticCompletion::Deferred) => Completion::Deferred,
            (None, SemanticCompletion::Cycle) => Completion::Cycle,
            (None, SemanticCompletion::Limit) => Completion::Limit,
            (None, SemanticCompletion::Complete) => unreachable!("complete binary lacks value"),
        }
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
        if let Some(operator) = match operator {
            BinaryOperator::LogicalAnd => Some(DeferredLogicalOperator::And),
            BinaryOperator::LogicalOr => Some(DeferredLogicalOperator::Or),
            BinaryOperator::NullishCoalesce => Some(DeferredLogicalOperator::Nullish),
            _ => None,
        } {
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
        spans: [Span; 4],
    ) -> TypeId {
        let evaluation = self.evaluate_binary(operator, left, right, 0);
        self.report_binary_diagnostics(file, operator, spans, &evaluation.diagnostics);
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
        // Preserve this typed operation until TS7 operand and TS6807 diagnostics are owned.
        if operator == DeferredBinaryOperator::UnsignedRightShift {
            return BinaryEvaluation {
                value: None,
                completion: SemanticCompletion::Deferred,
                diagnostics: BinaryDiagnostics::default(),
            };
        }
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
        let [left_flags, right_flags] = [left_kind, right_kind].map(binary_kind_flags);
        let mut diagnostics = BinaryDiagnostics {
            boolean_bitwise: matches!(
                operator,
                DeferredBinaryOperator::BitwiseAnd | DeferredBinaryOperator::BitwiseOr
            ) && has_flag(left_flags, BOOLEAN_LIKE)
                && has_flag(right_flags, BOOLEAN_LIKE),
            ..BinaryDiagnostics::default()
        };
        let mut value = if diagnostics.boolean_bitwise {
            Some(self.store.builtins.number)
        } else if operator == DeferredBinaryOperator::Add {
            if ((has_flag(left_flags | right_flags, STRING_LIKE))
                && has_flag(left_flags, STRING_ADD_OPERAND | ERROR_SENTINEL)
                && has_flag(right_flags, STRING_ADD_OPERAND | ERROR_SENTINEL))
                || is_any_never_pair(left_flags, right_flags)
            {
                Some(self.store.builtins.string)
            } else if has_flag(left_flags, ERROR_SENTINEL) {
                Some(left)
            } else if has_flag(right_flags, ERROR_SENTINEL) {
                Some(right)
            } else if is_number_never_pair(left_flags, right_flags) {
                Some(self.store.builtins.number)
            } else if is_bigint_never_pair(left_flags, right_flags) {
                Some(self.store.builtins.bigint)
            } else if has_flag(left_flags | right_flags, ANY) {
                Some(self.store.builtins.any)
            } else {
                None
            }
        } else if has_flag(left_flags, ERROR_SENTINEL) {
            Some(left)
        } else if has_flag(right_flags, ERROR_SENTINEL) {
            Some(right)
        } else if is_number_never_pair(left_flags, right_flags)
            || unordered_pair(left_flags, right_flags, ANY, ANY | NUMBER_LIKE)
            || is_any_never_pair(left_flags, right_flags)
        {
            Some(self.store.builtins.number)
        } else if is_bigint_never_pair(left_flags, right_flags)
            || unordered_pair(left_flags, right_flags, ANY, BIGINT)
        {
            Some(self.store.builtins.bigint)
        } else {
            None
        };
        if value.is_none() && unordered_pair(left_flags, right_flags, NUMBER_LIKE, BIGINT) {
            diagnostics.incompatible = Some((left, right));
            value = Some(if operator == DeferredBinaryOperator::Add {
                self.store.builtins.any
            } else {
                self.store.builtins.error
            });
        } else if value.is_none() && operator != DeferredBinaryOperator::Add {
            diagnostics.invalid_left = has_flag(left_flags, INVALID_ARITHMETIC);
            diagnostics.invalid_right = has_flag(right_flags, INVALID_ARITHMETIC);
            if diagnostics.invalid_left || diagnostics.invalid_right {
                if has_flag(left_flags | right_flags, BIGINT) {
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
        let left = completed!(self.force_type(left, depth));
        let left_kind = self.store.kind(left);
        if matches!(left_kind, TypeKind::Error | TypeKind::Invalid(_)) {
            return Completion::Complete(left);
        }
        match operator {
            DeferredLogicalOperator::And | DeferredLogicalOperator::Or => {
                known_truthiness(left_kind).map_or(Completion::Deferred, |truthy| {
                    let select_right = truthy == matches!(operator, DeferredLogicalOperator::And);
                    Completion::Complete(if select_right { right } else { left })
                })
            }
            DeferredLogicalOperator::Nullish => {
                if matches!(left_kind, TypeKind::Null | TypeKind::Undefined) {
                    Completion::Complete(right)
                } else if has_flag(binary_kind_flags(left_kind), DEFINITELY_NON_NULLISH) {
                    Completion::Complete(left)
                } else {
                    Completion::Deferred
                }
            }
        }
    }
}

fn type_is_string(store: &crate::semantics::types::TypeStore, ty: Option<TypeId>) -> bool {
    ty.is_some_and(|ty| has_flag(binary_kind_flags(store.kind(ty)), STRING_LIKE))
}

const ANY: u16 = 1 << 0;
const NEVER: u16 = 1 << 1;
const NUMBER_LIKE: u16 = 1 << 2;
const BIGINT: u16 = 1 << 3;
const STRING_LIKE: u16 = 1 << 4;
const BOOLEAN_LIKE: u16 = 1 << 5;
const INVALID_ARITHMETIC: u16 = 1 << 6;
const DEFINITELY_NON_NULLISH: u16 = 1 << 7;
const STRING_ADD_OPERAND: u16 = 1 << 8;
const ERROR_SENTINEL: u16 = 1 << 9;

const fn binary_kind_flags(kind: &TypeKind) -> u16 {
    match kind {
        TypeKind::Any => ANY | STRING_ADD_OPERAND,
        TypeKind::Never => NEVER | STRING_ADD_OPERAND,
        TypeKind::Null | TypeKind::Undefined => STRING_ADD_OPERAND,
        TypeKind::Boolean | TypeKind::LiteralBoolean(_, _) => {
            BOOLEAN_LIKE | INVALID_ARITHMETIC | STRING_ADD_OPERAND | DEFINITELY_NON_NULLISH
        }
        TypeKind::Number | TypeKind::LiteralNumber(_, _) => {
            NUMBER_LIKE | STRING_ADD_OPERAND | DEFINITELY_NON_NULLISH
        }
        TypeKind::String | TypeKind::LiteralString(_, _) => {
            STRING_LIKE | INVALID_ARITHMETIC | STRING_ADD_OPERAND | DEFINITELY_NON_NULLISH
        }
        TypeKind::BigInt => BIGINT | STRING_ADD_OPERAND | DEFINITELY_NON_NULLISH,
        TypeKind::ObjectKeyword
        | TypeKind::Symbol
        | TypeKind::Array(_)
        | TypeKind::Tuple(_)
        | TypeKind::Object(_)
        | TypeKind::Function(_) => DEFINITELY_NON_NULLISH,
        TypeKind::Error | TypeKind::Invalid(_) => ERROR_SENTINEL,
        _ => 0,
    }
}

const fn has_flag(flags: u16, flag: u16) -> bool {
    flags & flag != 0
}

const fn unordered_pair(left: u16, right: u16, first: u16, second: u16) -> bool {
    has_flag(left, first) && has_flag(right, second)
        || has_flag(right, first) && has_flag(left, second)
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

const fn is_any_never_pair(left: u16, right: u16) -> bool {
    unordered_pair(left, right, ANY, NEVER)
}

const fn is_number_never_pair(left: u16, right: u16) -> bool {
    unordered_pair(left, right, NUMBER_LIKE, NUMBER_LIKE | NEVER) || has_flag(left & right, NEVER)
}

const fn is_bigint_never_pair(left: u16, right: u16) -> bool {
    unordered_pair(left, right, BIGINT, BIGINT | NEVER)
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
