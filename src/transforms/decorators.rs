//! Decorator Transform to ES5
//!
//! This module transforms TypeScript decorators to ES5-compatible code.
//!
//! Decorators are transformed using the `__decorate` helper function:
//!
//! ```typescript
//! @decorator
//! class Foo { }
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! var Foo = /** @class */ (function () {
//!     function Foo() { }
//!     Foo = __decorate([decorator], Foo);
//!     return Foo;
//! }());
//! ```

use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::ThinNodeArena;
use crate::parser::NodeIndex;
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;

/// Decorator transformer producing IR nodes
pub struct DecoratorTransformer<'a> {
    arena: &'a ThinNodeArena,
    /// Whether to emit decorator metadata
    emit_decorator_metadata: bool,
}

impl<'a> DecoratorTransformer<'a> {
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        Self {
            arena,
            emit_decorator_metadata: false,
        }
    }

    /// Enable/disable decorator metadata emission
    pub fn set_emit_decorator_metadata(&mut self, emit: bool) {
        self.emit_decorator_metadata = emit;
    }

    /// Collect decorators from a node's modifiers
    pub fn collect_decorators(&self, modifiers: &Option<crate::parser::NodeList>) -> Vec<NodeIndex> {
        let mut decorators = Vec::new();

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == syntax_kind_ext::DECORATOR {
                        decorators.push(mod_idx);
                    }
                }
            }
        }

        decorators
    }

    /// Transform a decorated class to IR
    pub fn transform_class_decorators(
        &self,
        class_name: &str,
        decorators: &[NodeIndex],
    ) -> Option<IRNode> {
        if decorators.is_empty() {
            return None;
        }

        // Build: ClassName = __decorate([decorator1, decorator2, ...], ClassName);
        let decorator_array = self.build_decorator_array(decorators);

        Some(IRNode::expr_stmt(IRNode::assign(
            IRNode::id(class_name),
            IRNode::call(
                IRNode::id("__decorate"),
                vec![decorator_array, IRNode::id(class_name)],
            ),
        )))
    }

    /// Transform method decorators to IR
    pub fn transform_method_decorators(
        &self,
        class_name: &str,
        method_name: &str,
        decorators: &[NodeIndex],
        is_static: bool,
    ) -> Option<IRNode> {
        if decorators.is_empty() {
            return None;
        }

        // Build:
        // __decorate([decorator1, decorator2, ...], ClassName.prototype, "methodName", null);
        // or for static:
        // __decorate([decorator1, decorator2, ...], ClassName, "methodName", null);

        let decorator_array = self.build_decorator_array(decorators);

        let target = if is_static {
            IRNode::id(class_name)
        } else {
            IRNode::prop(IRNode::id(class_name), "prototype")
        };

        Some(IRNode::expr_stmt(IRNode::call(
            IRNode::id("__decorate"),
            vec![
                decorator_array,
                target,
                IRNode::string(method_name),
                IRNode::NullLiteral,
            ],
        )))
    }

    /// Transform property decorators to IR
    pub fn transform_property_decorators(
        &self,
        class_name: &str,
        property_name: &str,
        decorators: &[NodeIndex],
        is_static: bool,
    ) -> Option<IRNode> {
        if decorators.is_empty() {
            return None;
        }

        // Build:
        // __decorate([decorator1, decorator2, ...], ClassName.prototype, "propertyName", void 0);

        let decorator_array = self.build_decorator_array(decorators);

        let target = if is_static {
            IRNode::id(class_name)
        } else {
            IRNode::prop(IRNode::id(class_name), "prototype")
        };

        Some(IRNode::expr_stmt(IRNode::call(
            IRNode::id("__decorate"),
            vec![
                decorator_array,
                target,
                IRNode::string(property_name),
                IRNode::void_0(),
            ],
        )))
    }

    /// Transform accessor decorators to IR
    pub fn transform_accessor_decorators(
        &self,
        class_name: &str,
        accessor_name: &str,
        decorators: &[NodeIndex],
        is_static: bool,
    ) -> Option<IRNode> {
        if decorators.is_empty() {
            return None;
        }

        // Build:
        // __decorate([decorator1, decorator2, ...], ClassName.prototype, "accessorName",
        //     Object.getOwnPropertyDescriptor(ClassName.prototype, "accessorName"));

        let decorator_array = self.build_decorator_array(decorators);

        let target = if is_static {
            IRNode::id(class_name)
        } else {
            IRNode::prop(IRNode::id(class_name), "prototype")
        };

        let descriptor = IRNode::call(
            IRNode::prop(IRNode::id("Object"), "getOwnPropertyDescriptor"),
            vec![target.clone(), IRNode::string(accessor_name)],
        );

        Some(IRNode::expr_stmt(IRNode::call(
            IRNode::id("__decorate"),
            vec![
                decorator_array,
                target,
                IRNode::string(accessor_name),
                descriptor,
            ],
        )))
    }

    /// Transform parameter decorators to IR
    pub fn transform_parameter_decorators(
        &self,
        class_name: &str,
        method_name: &str,
        param_index: u32,
        decorators: &[NodeIndex],
        is_static: bool,
        is_constructor: bool,
    ) -> Option<IRNode> {
        if decorators.is_empty() {
            return None;
        }

        // Build:
        // __param(paramIndex, decorator)
        // Then applied via __decorate

        let decorator_array = self.build_param_decorator_array(decorators, param_index);

        let target = if is_static {
            IRNode::id(class_name)
        } else {
            IRNode::prop(IRNode::id(class_name), "prototype")
        };

        let method_key = if is_constructor {
            IRNode::void_0()
        } else {
            IRNode::string(method_name)
        };

        Some(IRNode::expr_stmt(IRNode::call(
            IRNode::id("__decorate"),
            vec![
                decorator_array,
                target,
                method_key,
                IRNode::NullLiteral,
            ],
        )))
    }

    /// Build an array of decorator expressions
    fn build_decorator_array(&self, decorators: &[NodeIndex]) -> IRNode {
        let elements: Vec<IRNode> = decorators
            .iter()
            .filter_map(|&idx| self.transform_decorator_expression(idx))
            .collect();

        IRNode::ArrayLiteral(elements)
    }

    /// Build parameter decorator array with __param wrapper
    fn build_param_decorator_array(&self, decorators: &[NodeIndex], param_index: u32) -> IRNode {
        let elements: Vec<IRNode> = decorators
            .iter()
            .filter_map(|&idx| {
                let expr = self.transform_decorator_expression(idx)?;
                // Wrap in __param(index, decorator)
                Some(IRNode::call(
                    IRNode::id("__param"),
                    vec![IRNode::number(&param_index.to_string()), expr],
                ))
            })
            .collect();

        IRNode::ArrayLiteral(elements)
    }

    /// Transform a decorator expression to IR
    fn transform_decorator_expression(&self, decorator_idx: NodeIndex) -> Option<IRNode> {
        let decorator_node = self.arena.get(decorator_idx)?;

        if decorator_node.kind != syntax_kind_ext::DECORATOR {
            return None;
        }

        let decorator = self.arena.get_decorator(decorator_node)?;

        // Transform the decorator's expression
        self.transform_expression(decorator.expression)
    }

    /// Transform an expression to IR (simplified)
    fn transform_expression(&self, expr_idx: NodeIndex) -> Option<IRNode> {
        if expr_idx.is_none() {
            return None;
        }

        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.arena.get_identifier(expr_node)?;
                Some(IRNode::id(&ident.escaped_text))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(expr_node)?;
                let callee = self.transform_expression(call.expression)?;
                let mut args = Vec::new();
                if let Some(ref arg_list) = call.arguments {
                    for &arg_idx in &arg_list.nodes {
                        if let Some(arg) = self.transform_expression(arg_idx) {
                            args.push(arg);
                        }
                    }
                }
                Some(IRNode::call(callee, args))
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(expr_node)?;
                let object = self.transform_expression(access.expression)?;
                let property = self.get_identifier_text(access.name_or_argument)?;
                Some(IRNode::prop(object, &property))
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                let lit = self.arena.get_literal(expr_node)?;
                Some(IRNode::string(&lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(expr_node)?;
                Some(IRNode::number(&lit.text))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(IRNode::BooleanLiteral(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(IRNode::BooleanLiteral(false)),
            k if k == SyntaxKind::NullKeyword as u16 => Some(IRNode::NullLiteral),
            _ => {
                // Fallback to AST reference
                Some(IRNode::ASTRef(expr_idx))
            }
        }
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        let ident = self.arena.get_identifier(node)?;
        Some(ident.escaped_text.clone())
    }

    /// Generate metadata decorator for a class if emitDecoratorMetadata is enabled
    pub fn generate_metadata_decorator(
        &self,
        class_name: &str,
        _class_idx: NodeIndex,
    ) -> Option<IRNode> {
        if !self.emit_decorator_metadata {
            return None;
        }

        // __metadata("design:type", Type)
        // This is a simplified version - full implementation would analyze types
        Some(IRNode::expr_stmt(IRNode::call(
            IRNode::id("__decorate"),
            vec![
                IRNode::ArrayLiteral(vec![
                    IRNode::call(
                        IRNode::id("__metadata"),
                        vec![
                            IRNode::string("design:type"),
                            IRNode::id("Function"),
                        ],
                    ),
                ]),
                IRNode::id(class_name),
            ],
        )))
    }
}

/// Information about decorators on a class member
#[derive(Debug, Clone)]
pub struct MemberDecoratorInfo {
    pub member_name: String,
    pub decorators: Vec<NodeIndex>,
    pub is_static: bool,
    pub kind: MemberKind,
}

/// Kind of class member
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Method,
    Property,
    Accessor,
    Constructor,
}

/// Collect all decorator information from a class
pub fn collect_class_decorator_info<'a>(
    arena: &'a ThinNodeArena,
    class_idx: NodeIndex,
) -> Option<ClassDecoratorInfo> {
    let class_node = arena.get(class_idx)?;
    let class_data = arena.get_class(class_node)?;

    // Get class name
    let class_name = if !class_data.name.is_none() {
        let name_node = arena.get(class_data.name)?;
        let ident = arena.get_identifier(name_node)?;
        ident.escaped_text.clone()
    } else {
        return None; // Anonymous classes can't be decorated meaningfully
    };

    let transformer = DecoratorTransformer::new(arena);

    // Collect class decorators
    let class_decorators = transformer.collect_decorators(&class_data.modifiers);

    // Collect member decorators
    let mut member_decorators = Vec::new();

    for &member_idx in &class_data.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };

        let (modifiers, name_idx, is_static, kind) = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = arena.get_method_decl(member_node) {
                    let is_static = has_static_modifier(arena, &method.modifiers);
                    (method.modifiers.clone(), method.name, is_static, MemberKind::Method)
                } else {
                    continue;
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = arena.get_property_decl(member_node) {
                    let is_static = has_static_modifier(arena, &prop.modifiers);
                    (prop.modifiers.clone(), prop.name, is_static, MemberKind::Property)
                } else {
                    continue;
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = arena.get_accessor(member_node) {
                    let is_static = has_static_modifier(arena, &accessor.modifiers);
                    (accessor.modifiers.clone(), accessor.name, is_static, MemberKind::Accessor)
                } else {
                    continue;
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                if let Some(ctor) = arena.get_constructor(member_node) {
                    (ctor.modifiers.clone(), NodeIndex::NONE, false, MemberKind::Constructor)
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        let decorators = transformer.collect_decorators(&modifiers);
        if !decorators.is_empty() {
            let name = if !name_idx.is_none() {
                get_member_name(arena, name_idx).unwrap_or_else(|| "constructor".to_string())
            } else {
                "constructor".to_string()
            };

            member_decorators.push(MemberDecoratorInfo {
                member_name: name,
                decorators,
                is_static,
                kind,
            });
        }
    }

    Some(ClassDecoratorInfo {
        class_name,
        class_decorators,
        member_decorators,
    })
}

/// Information about all decorators in a class
#[derive(Debug, Clone)]
pub struct ClassDecoratorInfo {
    pub class_name: String,
    pub class_decorators: Vec<NodeIndex>,
    pub member_decorators: Vec<MemberDecoratorInfo>,
}

fn has_static_modifier(arena: &ThinNodeArena, modifiers: &Option<crate::parser::NodeList>) -> bool {
    if let Some(mods) = modifiers {
        for &mod_idx in &mods.nodes {
            if let Some(mod_node) = arena.get(mod_idx) {
                if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                    return true;
                }
            }
        }
    }
    false
}

fn get_member_name(arena: &ThinNodeArena, name_idx: NodeIndex) -> Option<String> {
    let name_node = arena.get(name_idx)?;
    if name_node.kind == SyntaxKind::Identifier as u16 {
        let ident = arena.get_identifier(name_node)?;
        return Some(ident.escaped_text.clone());
    }
    if name_node.kind == SyntaxKind::StringLiteral as u16 {
        let lit = arena.get_literal(name_node)?;
        return Some(lit.text.clone());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decorator_transformer_creation() {
        let arena = ThinNodeArena::new();
        let transformer = DecoratorTransformer::new(&arena);
        assert!(!transformer.emit_decorator_metadata);
    }

    #[test]
    fn test_collect_empty_decorators() {
        let arena = ThinNodeArena::new();
        let transformer = DecoratorTransformer::new(&arena);
        let decorators = transformer.collect_decorators(&None);
        assert!(decorators.is_empty());
    }

    #[test]
    fn test_transform_class_decorators_empty() {
        let arena = ThinNodeArena::new();
        let transformer = DecoratorTransformer::new(&arena);
        let result = transformer.transform_class_decorators("Foo", &[]);
        assert!(result.is_none());
    }
}
