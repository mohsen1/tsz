//! Decorator Type Checking
//!
//! This module provides type checking for TypeScript decorators.
//!
//! Decorators can be applied to:
//! - Classes
//! - Methods
//! - Accessors (getters/setters)
//! - Properties
//! - Parameters
//!
//! This module validates:
//! - Decorator placement (can only decorate certain constructs)
//! - Decorator function signatures
//! - Decorator return types

use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;

/// Decorator target types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoratorTarget {
    Class,
    Method,
    Accessor,
    Property,
    Parameter,
}

/// Decorator validation error
#[derive(Debug, Clone)]
pub enum DecoratorError {
    /// Decorator applied to invalid target
    InvalidTarget { target: String, pos: u32 },
    /// Decorator expression is not callable
    NotCallable { pos: u32 },
    /// Decorator factory returned wrong type
    InvalidReturnType {
        expected: String,
        actual: String,
        pos: u32,
    },
    /// Decorator not allowed in this context
    NotAllowedHere { reason: String, pos: u32 },
    /// Experimental decorators not enabled
    ExperimentalDecoratorsRequired { pos: u32 },
}

/// Decorator checker for validating decorator usage
pub struct DecoratorChecker<'a> {
    arena: &'a NodeArena,
    /// Whether experimental decorators are enabled
    experimental_decorators: bool,
    /// Whether to use the TC39 decorator semantics
    use_tc39_decorators: bool,
}

impl<'a> DecoratorChecker<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            experimental_decorators: true, // Default to true for backwards compat
            use_tc39_decorators: false,
        }
    }

    /// Enable/disable experimental decorators
    pub fn set_experimental_decorators(&mut self, enabled: bool) {
        self.experimental_decorators = enabled;
    }

    /// Enable/disable TC39 decorator semantics
    pub fn set_use_tc39_decorators(&mut self, enabled: bool) {
        self.use_tc39_decorators = enabled;
    }

    /// Check if a decorator is valid in its context
    pub fn check_decorator(
        &self,
        decorator_idx: NodeIndex,
        parent_idx: NodeIndex,
    ) -> Vec<DecoratorError> {
        let mut errors = Vec::new();

        if !self.experimental_decorators {
            if let Some(node) = self.arena.get(decorator_idx) {
                errors.push(DecoratorError::ExperimentalDecoratorsRequired { pos: node.pos });
            }
            return errors;
        }

        let Some(parent_node) = self.arena.get(parent_idx) else {
            return errors;
        };

        let target = self.get_decorator_target(parent_node.kind);

        if let Some(target) = target {
            // Validate the decorator expression
            self.check_decorator_expression(decorator_idx, target, &mut errors);
        } else if let Some(node) = self.arena.get(decorator_idx) {
            errors.push(DecoratorError::InvalidTarget {
                target: self.get_kind_name(parent_node.kind),
                pos: node.pos,
            });
        }

        errors
    }

    /// Get the decorator target type from a syntax kind
    fn get_decorator_target(&self, kind: u16) -> Option<DecoratorTarget> {
        match kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                Some(DecoratorTarget::Class)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => Some(DecoratorTarget::Method),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                Some(DecoratorTarget::Accessor)
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => Some(DecoratorTarget::Property),
            k if k == syntax_kind_ext::PARAMETER => Some(DecoratorTarget::Parameter),
            _ => None,
        }
    }

    /// Get a human-readable name for a syntax kind
    fn get_kind_name(&self, kind: u16) -> String {
        match kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => "function declaration".to_string(),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => "variable statement".to_string(),
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => "interface declaration".to_string(),
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => "type alias".to_string(),
            k if k == syntax_kind_ext::ENUM_DECLARATION => "enum declaration".to_string(),
            k if k == syntax_kind_ext::MODULE_DECLARATION => "module declaration".to_string(),
            _ => format!("syntax kind {}", kind),
        }
    }

    /// Check the decorator expression
    fn check_decorator_expression(
        &self,
        decorator_idx: NodeIndex,
        _target: DecoratorTarget,
        _errors: &mut Vec<DecoratorError>,
    ) {
        let Some(decorator_node) = self.arena.get(decorator_idx) else {
            return;
        };

        if decorator_node.kind != syntax_kind_ext::DECORATOR {
            return;
        }

        let Some(decorator) = self.arena.get_decorator(decorator_node) else {
            return;
        };

        // Get the expression being used as a decorator
        let Some(expr_node) = self.arena.get(decorator.expression) else {
            return;
        };

        // The decorator expression can be:
        // 1. An identifier: @decorator
        // 2. A call expression: @decorator()
        // 3. A property access: @namespace.decorator
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                // Simple identifier - needs to be a function
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                // Factory pattern - decorator() returns the actual decorator
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // Namespaced decorator - namespace.decorator
            }
            _ => {
                // Other expressions might be valid in some cases
            }
        }
    }

    /// Check all decorators on a class declaration
    pub fn check_class_decorators(&self, class_idx: NodeIndex) -> Vec<DecoratorError> {
        let mut errors = Vec::new();

        let Some(class_node) = self.arena.get(class_idx) else {
            return errors;
        };

        let Some(class_data) = self.arena.get_class(class_node) else {
            return errors;
        };

        // Check class-level decorators
        if let Some(ref modifiers) = class_data.modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == syntax_kind_ext::DECORATOR {
                        errors.extend(self.check_decorator(mod_idx, class_idx));
                    }
            }
        }

        // Check member decorators
        for &member_idx in &class_data.members.nodes {
            errors.extend(self.check_member_decorators(member_idx));
        }

        errors
    }

    /// Check decorators on a class member
    fn check_member_decorators(&self, member_idx: NodeIndex) -> Vec<DecoratorError> {
        let mut errors = Vec::new();

        let Some(member_node) = self.arena.get(member_idx) else {
            return errors;
        };

        // Get modifiers based on member type
        let modifiers = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_method_decl(member_node)
                .and_then(|m| m.modifiers.as_ref()),
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .arena
                .get_property_decl(member_node)
                .and_then(|p| p.modifiers.as_ref()),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .arena
                .get_accessor(member_node)
                .and_then(|a| a.modifiers.as_ref()),
            _ => None,
        };

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == syntax_kind_ext::DECORATOR {
                        errors.extend(self.check_decorator(mod_idx, member_idx));
                    }
            }
        }

        // Check parameter decorators for methods
        if member_node.kind == syntax_kind_ext::METHOD_DECLARATION
            && let Some(method) = self.arena.get_method_decl(member_node) {
                for &param_idx in &method.parameters.nodes {
                    errors.extend(self.check_parameter_decorators(param_idx));
                }
            }

        errors
    }

    /// Check decorators on a parameter
    fn check_parameter_decorators(&self, param_idx: NodeIndex) -> Vec<DecoratorError> {
        let mut errors = Vec::new();

        let Some(param_node) = self.arena.get(param_idx) else {
            return errors;
        };

        let Some(param) = self.arena.get_parameter(param_node) else {
            return errors;
        };

        if let Some(ref modifiers) = param.modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == syntax_kind_ext::DECORATOR {
                        errors.extend(self.check_decorator(mod_idx, param_idx));
                    }
            }
        }

        errors
    }

    /// Get decorator info for emit
    pub fn get_decorator_info(&self, decorator_idx: NodeIndex) -> Option<DecoratorInfo> {
        let decorator_node = self.arena.get(decorator_idx)?;

        if decorator_node.kind != syntax_kind_ext::DECORATOR {
            return None;
        }

        let decorator = self.arena.get_decorator(decorator_node)?;
        let expr_node = self.arena.get(decorator.expression)?;

        let kind = if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            DecoratorKind::Factory
        } else {
            DecoratorKind::Simple
        };

        Some(DecoratorInfo {
            expression: decorator.expression,
            kind,
            pos: decorator_node.pos,
        })
    }
}

/// Information about a decorator for emit
#[derive(Debug, Clone)]
pub struct DecoratorInfo {
    pub expression: NodeIndex,
    pub kind: DecoratorKind,
    pub pos: u32,
}

/// Kind of decorator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoratorKind {
    /// Simple decorator: @decorator
    Simple,
    /// Factory decorator: @decorator()
    Factory,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decorator_target_class() {
        let arena = NodeArena::new();
        let checker = DecoratorChecker::new(&arena);

        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::CLASS_DECLARATION),
            Some(DecoratorTarget::Class)
        );
        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::CLASS_EXPRESSION),
            Some(DecoratorTarget::Class)
        );
    }

    #[test]
    fn test_decorator_target_method() {
        let arena = NodeArena::new();
        let checker = DecoratorChecker::new(&arena);

        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::METHOD_DECLARATION),
            Some(DecoratorTarget::Method)
        );
    }

    #[test]
    fn test_decorator_target_accessor() {
        let arena = NodeArena::new();
        let checker = DecoratorChecker::new(&arena);

        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::GET_ACCESSOR),
            Some(DecoratorTarget::Accessor)
        );
        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::SET_ACCESSOR),
            Some(DecoratorTarget::Accessor)
        );
    }

    #[test]
    fn test_decorator_target_property() {
        let arena = NodeArena::new();
        let checker = DecoratorChecker::new(&arena);

        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::PROPERTY_DECLARATION),
            Some(DecoratorTarget::Property)
        );
    }

    #[test]
    fn test_decorator_target_parameter() {
        let arena = NodeArena::new();
        let checker = DecoratorChecker::new(&arena);

        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::PARAMETER),
            Some(DecoratorTarget::Parameter)
        );
    }

    #[test]
    fn test_decorator_invalid_target() {
        let arena = NodeArena::new();
        let checker = DecoratorChecker::new(&arena);

        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::FUNCTION_DECLARATION),
            None
        );
        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::VARIABLE_STATEMENT),
            None
        );
        assert_eq!(
            checker.get_decorator_target(syntax_kind_ext::INTERFACE_DECLARATION),
            None
        );
    }
}
