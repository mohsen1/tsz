use crate::bind::{ScopeId, TypeofWitness};
use crate::program::SemanticCompletion;
use crate::semantics::relation::{RelationContext, RelationMode};
use crate::semantics::types::{
    Completion, DeferredBinaryOperator, DeferredLogicalOperator, DeferredType, TypeId, TypeKind,
};
use crate::source::{FileId, Span};
use crate::syntax::{BinaryOperator, Expression, ExpressionKind};

use super::{
    Checker,
    capabilities::completion_state,
    relation_diagnostic::{ContextualType, RelationDiagnosticStyle},
};

pub(super) struct BinaryEvaluation {
    pub(super) result: Completion<TypeId>,
    pub(super) dependency_completion: SemanticCompletion,
    diagnostics: BinaryDiagnostics,
}

#[derive(Default)]
struct BinaryDiagnostics {
    boolean_bitwise_suggestion: Option<&'static str>,
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
    BitwiseXor => BitwiseXor, "^";
    BitwiseOr => BitwiseOr, "|";
    LeftShift => LeftShift, "<<";
    SignedRightShift => SignedRightShift, ">>";
    UnsignedRightShift => UnsignedRightShift, ">>>";
}

impl BinaryEvaluation {
    const fn completion(&self) -> SemanticCompletion {
        self.dependency_completion
            .combine(completion_state(&self.result))
    }
}

const LEFT_ARITHMETIC_MESSAGE: &str = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.";
const RIGHT_ARITHMETIC_MESSAGE: &str = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.";
const LEFT_INSTANCEOF_MESSAGE: &str = "The left-hand side of an 'instanceof' expression must be of type 'any', an object type or a type parameter.";
const RIGHT_INSTANCEOF_MESSAGE: &str = "The right-hand side of an 'instanceof' expression must be either of type 'any', a class, function, or other type assignable to the 'Function' interface type, or an object type with a 'Symbol.hasInstance' method.";

impl Checker<'_> {
    pub(super) fn infer_authored_binary_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        expected: ContextualType,
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
        if *operator == BinaryOperator::Comma {
            self.infer_expression(file, scope, left, None);
            return self.infer_expression_contextual(file, scope, right, expected);
        }
        if *operator == BinaryOperator::InstanceOf {
            return self.infer_instanceof_expression(file, scope, left, right);
        }
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

    fn infer_instanceof_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        left: &Expression,
        right: &Expression,
    ) -> TypeId {
        let left_type = self.infer_expression(file, scope, left, None);
        let right_type = self.infer_expression(file, scope, right, None);
        for (operand, expression, right, code, message) in [
            (left_type, left, false, 2358, LEFT_INSTANCEOF_MESSAGE),
            (right_type, right, true, 2359, RIGHT_INSTANCEOF_MESSAGE),
        ] {
            let outcome = self.instanceof_operand(operand, right, 0);
            if let Completion::Complete(false) = self.require_completion(outcome) {
                self.push_diagnostic(file, expression.span, message.into(), code);
            }
        }
        self.store.builtins.boolean
    }

    fn instanceof_operand(
        &mut self,
        operand: TypeId,
        right: bool,
        depth: usize,
    ) -> Completion<bool> {
        if right
            && let TypeKind::Deferred(DeferredType::Value(declaration)) = self.store.kind(operand)
            && self
                .program
                .standard_library
                .is_instanceof_constructor_value(*declaration)
        {
            return Completion::Complete(true);
        }
        let operand = completed!(self.force_operand(operand, depth));
        if right
            && let TypeKind::LibraryReference { declaration, .. } = self.store.kind(operand)
            && self.program.standard_library.is_function_type(*declaration)
        {
            return Completion::Complete(true);
        }
        match self.store.kind(operand).clone() {
            TypeKind::TypeParameter { .. } => {
                match completed!(self.type_parameter_constraint(operand)) {
                    None => Completion::Complete(!right),
                    Some(constraint) => {
                        let constraint = completed!(self.force_operand(constraint, depth + 1));
                        if matches!(
                            self.store.kind(constraint),
                            TypeKind::TypeParameter { .. }
                                | TypeKind::Union(_)
                                | TypeKind::Intersection(_)
                        ) {
                            Completion::Deferred
                        } else if right && matches!(self.store.kind(constraint), TypeKind::Any) {
                            Completion::Complete(false)
                        } else {
                            self.instanceof_operand(constraint, right, depth + 1)
                        }
                    }
                }
            }
            TypeKind::Union(members) => {
                let mut incomplete = SemanticCompletion::Complete;
                for member in members {
                    let outcome = self.instanceof_operand(member, right, depth + 1);
                    match outcome {
                        Completion::Complete(value) if value == !right => {
                            return Completion::Complete(value);
                        }
                        Completion::Complete(_) => {}
                        _ => incomplete = incomplete.combine(completion_state(&outcome)),
                    }
                }
                match incomplete {
                    SemanticCompletion::Complete => Completion::Complete(right),
                    SemanticCompletion::Deferred => Completion::Deferred,
                    SemanticCompletion::Cycle => Completion::Cycle,
                    SemanticCompletion::Limit => Completion::Limit,
                }
            }
            kind => instanceof_atomic(&kind, right, self.options.effective_strict_null_checks())
                .map_or(Completion::Deferred, Completion::Complete),
        }
    }

    pub(super) fn infer_compound_add_assignment(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression_span: Span,
        operator_span: Span,
        left: &Expression,
        right: &Expression,
    ) -> TypeId {
        let target = self.infer_assignment_target(file, scope, left);
        let left_type = target.unwrap_or(self.store.builtins.error);
        let right_type = self.infer_expression(file, scope, right, None);
        let source = self.infer_binary_expression(
            file,
            DeferredBinaryOperator::Add,
            left_type,
            right_type,
            [left.span, right.span, operator_span, expression_span],
        );
        if let Some(target) = target {
            self.report_relation(
                source,
                target,
                left.span,
                Some(right),
                RelationMode::Assignment,
                RelationDiagnosticStyle::Type,
            );
        }
        source
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
        self.observe_completion(evaluation.completion());
        match evaluation.result {
            Completion::Complete(value) => value,
            _ => self.store.intern(TypeKind::Deferred(DeferredType::Binary {
                operator,
                left,
                right,
            })),
        }
    }

    pub(super) fn evaluate_binary(
        &mut self,
        operator: DeferredBinaryOperator,
        left: TypeId,
        right: TypeId,
        depth: usize,
    ) -> BinaryEvaluation {
        let (left, left_completion) = self.binary_operand(left, depth);
        let (right, right_completion) = self.binary_operand(right, depth);
        let operand_completion = left_completion.combine(right_completion);
        if operator == DeferredBinaryOperator::Add
            && !operand_completion.is_complete()
            && (type_is_string(&self.store, left) || type_is_string(&self.store, right))
        {
            return BinaryEvaluation {
                result: Completion::Complete(self.store.builtins.string),
                dependency_completion: operand_completion,
                diagnostics: BinaryDiagnostics::default(),
            };
        }
        let (Some(left), Some(right)) = (left, right) else {
            return BinaryEvaluation {
                result: incomplete_binary_result(operand_completion),
                dependency_completion: operand_completion,
                diagnostics: BinaryDiagnostics::default(),
            };
        };
        let left_kind = self.store.kind(left);
        let right_kind = self.store.kind(right);
        let [left_flags, right_flags] = [left_kind, right_kind].map(binary_kind_flags);
        if operator == DeferredBinaryOperator::UnsignedRightShift {
            let supported = has_flag(left_flags, NUMBER_LIKE)
                && matches!(
                        right_kind,
                        TypeKind::LiteralNumber(value, _)
                            if value.array_index().is_some_and(|value| value < 32)
                );
            return BinaryEvaluation {
                result: if supported {
                    Completion::Complete(self.store.builtins.number)
                } else {
                    Completion::Deferred
                },
                dependency_completion: operand_completion,
                diagnostics: BinaryDiagnostics::default(),
            };
        }
        let boolean_operands =
            has_flag(left_flags, BOOLEAN_LIKE) && has_flag(right_flags, BOOLEAN_LIKE);
        let mut diagnostics = BinaryDiagnostics {
            boolean_bitwise_suggestion: match operator {
                DeferredBinaryOperator::BitwiseAnd if boolean_operands => Some("&&"),
                DeferredBinaryOperator::BitwiseXor if boolean_operands => Some("!=="),
                DeferredBinaryOperator::BitwiseOr if boolean_operands => Some("||"),
                _ => None,
            },
            ..BinaryDiagnostics::default()
        };
        let mut value = if diagnostics.boolean_bitwise_suggestion.is_some() {
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
            result: value.map_or(Completion::Deferred, Completion::Complete),
            dependency_completion: operand_completion,
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
        if let Some(suggested) = diagnostics.boolean_bitwise_suggestion {
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
            let left_name = diagnostic_type_name(&self.store, operator, left);
            let right_name = diagnostic_type_name(&self.store, operator, right);
            let (Completion::Complete(left_name), Completion::Complete(right_name)) = (
                self.require_file_completion(file, left_name),
                self.require_file_completion(file, right_name),
            ) else {
                return;
            };
            let message = format!(
                "Operator '{}' cannot be applied to types '{}' and '{}'.",
                operator_text(operator),
                left_name,
                right_name
            );
            self.push_diagnostic(file, expression_span, message, 2365);
        }
    }

    fn binary_operand(
        &mut self,
        operand: TypeId,
        depth: usize,
    ) -> (Option<TypeId>, SemanticCompletion) {
        let bigint = self.store.builtins.bigint;
        if self.is_bigint_operand(operand, 0) {
            return (Some(bigint), SemanticCompletion::Complete);
        }
        match self.force_operand(operand, depth) {
            Completion::Complete(value) => (Some(value), SemanticCompletion::Complete),
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
        self.observe_completion(evaluation.completion());
        evaluation.result
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

const fn incomplete_binary_result(completion: SemanticCompletion) -> Completion<TypeId> {
    match completion {
        SemanticCompletion::Complete | SemanticCompletion::Deferred => Completion::Deferred,
        SemanticCompletion::Cycle => Completion::Cycle,
        SemanticCompletion::Limit => Completion::Limit,
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
) -> Completion<String> {
    match (operator, store.kind(ty)) {
        (op, TypeKind::LiteralString(_, _)) if op != DeferredBinaryOperator::Add => {
            Completion::Complete("string".into())
        }
        (op, TypeKind::LiteralNumber(_, _)) if op != DeferredBinaryOperator::Add => {
            Completion::Complete("number".into())
        }
        (op, TypeKind::LiteralBoolean(_, _)) if op != DeferredBinaryOperator::Add => {
            Completion::Complete("boolean".into())
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

fn instanceof_atomic(kind: &TypeKind, right: bool, strict_nulls: bool) -> Option<bool> {
    use TypeKind::*;
    let (domain, object_like) = Checker::flow_type_domain(kind);
    Some(match right {
        false if domain.is_some() && !object_like => false,
        false if matches!(kind, Never | Void) => false,
        false => (!matches!(
            kind,
            TypeParameter { .. }
                | Union(_)
                | Intersection(_)
                | LibraryReference { .. }
                | Deferred(_)
        ))
        .then_some(true)?,
        true if domain == Some(TypeofWitness::Function) => true,
        true if matches!(kind, Any | Never | Error | Invalid(_)) => true,
        true if matches!(kind, Null | Undefined) && !strict_nulls => true,
        true if domain.is_some() && !object_like => false,
        true if matches!(kind, Unknown | Void | ObjectKeyword) => false,
        true if matches!(kind, Object(shape) if shape.properties.is_empty()) => false,
        true => return None,
    })
}
