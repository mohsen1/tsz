use std::collections::HashSet;

use crate::program::SemanticCompletion;
use crate::semantics::types::{DeferredType, TypeId};

use super::{Checker, ReferenceExpansionStack};

impl Checker<'_> {
    pub(super) fn visit_deferred_operands(
        &mut self,
        deferred: &DeferredType,
        active: &mut HashSet<TypeId>,
        references: &mut ReferenceExpansionStack,
        state: &mut SemanticCompletion,
    ) {
        match deferred {
            DeferredType::Reference { arguments, .. } => {
                self.combine_required_children(arguments.iter().copied(), active, references, state)
            }
            DeferredType::FlowReference { declared, .. } => {
                self.combine_required_children([*declared], active, references, state);
            }
            DeferredType::Value(_)
            | DeferredType::LexicalThis { .. }
            | DeferredType::BigIntLiteral
            | DeferredType::NumericRecovery
            | DeferredType::Utf16StringLiteral
            | DeferredType::UniqueSymbol
            | DeferredType::GenericCall
            | DeferredType::GenericFunction
            | DeferredType::ObjectShape => {}
            DeferredType::Call { callee, .. }
            | DeferredType::Unary {
                operand: callee, ..
            }
            | DeferredType::KeyOf(callee)
            | DeferredType::Property { object: callee, .. } => {
                self.combine_required_children([*callee], active, references, state);
            }
            DeferredType::Construct {
                callee,
                type_arguments,
                ..
            } => {
                self.combine_required_children([*callee], active, references, state);
                self.combine_required_children(
                    type_arguments.iter().copied(),
                    active,
                    references,
                    state,
                );
            }
            DeferredType::Predicate { asserted, .. } => {
                self.combine_required_children(asserted.iter().copied(), active, references, state);
            }
            DeferredType::Binary { left, right, .. }
            | DeferredType::Logical { left, right, .. }
            | DeferredType::ElementAccess {
                object: left,
                index: right,
                ..
            } => self.combine_required_children([*left, *right], active, references, state),
            DeferredType::Conditional {
                check,
                extends,
                when_true,
                when_false,
            } => self.combine_required_children(
                [*check, *extends, *when_true, *when_false],
                active,
                references,
                state,
            ),
            DeferredType::Mapped {
                constraint,
                name_type,
                value,
                ..
            } => {
                self.combine_required_children([*constraint, *value], active, references, state);
                self.combine_required_children(
                    name_type.iter().copied(),
                    active,
                    references,
                    state,
                );
            }
            DeferredType::IndexedAccess { object, index } => {
                self.combine_required_children([*object, *index], active, references, state);
            }
        }
    }
}
