//! Super keyword type computation.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of the `super` keyword.
    ///
    /// Computes the type of `super` expressions:
    /// - `super()` calls: returns the base class constructor type
    /// - `super.property` access: returns the base class instance type
    /// - Static context: returns constructor type
    /// - Instance context: returns instance type
    pub(crate) fn get_type_of_super_keyword(&mut self, idx: NodeIndex) -> TypeId {
        // Check super expression validity and emit any errors
        self.check_super_expression(idx);

        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return TypeId::ERROR;
        };

        let mut extends_expr_idx = NodeIndex::NONE;
        let mut extends_type_args = None;
        if let Some(current_class) = self.ctx.arena.get_class_at(class_info.class_idx)
            && let Some(heritage_clauses) = &current_class.heritage_clauses
        {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let Some(&type_idx) = heritage.types.nodes.first() else {
                    continue;
                };
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
                    extends_expr_idx = expr_type_args.expression;
                    extends_type_args = expr_type_args.type_arguments.clone();
                } else {
                    extends_expr_idx = type_idx;
                }
                break;
            }
        }

        // Detect `super(...)` usage by checking if the parent is a CallExpression whose callee is `super`.
        let is_super_call = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent).map(|n| (ext.parent, n)))
            .and_then(|(parent_idx, parent_node)| {
                if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                    return None;
                }
                let call = self.ctx.arena.get_call_expr(parent_node)?;
                Some(call.expression == idx && parent_idx.is_some())
            })
            .unwrap_or(false);

        let is_static_context = self.find_enclosing_static_block(idx).is_some()
            || self.is_this_in_static_class_member(idx);

        if is_super_call || is_static_context {
            if extends_expr_idx.is_some()
                && let Some(ctor_type) = self.base_constructor_type_from_expression(
                    extends_expr_idx,
                    extends_type_args.as_ref(),
                )
            {
                // For super() calls without explicit type arguments, verify
                // the resolved type has construct signatures. When the base
                // class is forward-referenced (used before its declaration),
                // identifier resolution may return a stale symbol type (a
                // Callable with static properties but missing construct
                // signatures) even though the direct class constructor type
                // computation produces the correct type. In that case, fall
                // through to get_class_constructor_type which builds the
                // constructor type from the AST and always includes construct
                // signatures (including default constructors).
                //
                // When type arguments ARE present, missing construct sigs may
                // be intentional (e.g., `extends Base<any>` where Base is not
                // generic — apply_type_arguments_to_constructor_type_for_extends
                // deliberately strips construct sigs so TS2346 fires). In that
                // case, return the type as-is.
                if is_super_call && extends_type_args.is_none() {
                    let has_construct_sigs =
                        crate::query_boundaries::common::construct_signatures_for_type(
                            self.ctx.types,
                            ctor_type,
                        )
                        .is_some_and(|sigs| !sigs.is_empty());

                    if !has_construct_sigs {
                        // Fall through: super() target lacks construct
                        // signatures without type args — likely a forward
                        // reference. Try direct class constructor type lookup.
                    } else {
                        return ctor_type;
                    }
                } else {
                    return ctor_type;
                }
            }

            let Some(base_class_idx) = self.get_base_class_idx(class_info.class_idx) else {
                return TypeId::ERROR;
            };
            let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
                return TypeId::ERROR;
            };
            let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                return TypeId::ERROR;
            };
            return self.get_class_constructor_type(base_class_idx, base_class);
        }

        if extends_expr_idx.is_some()
            && let Some(instance_type) = self
                .base_instance_type_from_expression(extends_expr_idx, extends_type_args.as_ref())
        {
            return instance_type;
        }

        let Some(base_class_idx) = self.get_base_class_idx(class_info.class_idx) else {
            return TypeId::ERROR;
        };
        let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
            return TypeId::ERROR;
        };
        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            return TypeId::ERROR;
        };

        self.get_class_instance_type(base_class_idx, base_class)
    }

    /// Returns true when `super_idx` is the `super` keyword inside a class whose
    /// `extends` clause references a class declared *after* it in source order
    /// (i.e. TS2449 "Class 'X' used before its declaration" already fires on
    /// the extends clause).  In this situation tsc suppresses the secondary
    /// TS2346 ("Call target does not contain any signatures.") on `super()`
    /// because the forward-reference is the real root cause and will be
    /// resolved at runtime via hoisting.
    pub(crate) fn is_super_call_in_forward_referenced_extends(&self, super_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;

        let _ = super_idx; // super_idx currently unused; reserved for future precision
        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };
        let Some(current_class) = self.ctx.arena.get_class_at(class_info.class_idx) else {
            return false;
        };
        let Some(heritage_clauses) = &current_class.heritage_clauses else {
            return false;
        };
        for &clause_idx in &heritage_clauses.nodes {
            let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let Some(&type_idx) = heritage.types.nodes.first() else {
                continue;
            };
            // Walk through `Foo<T>` expression-with-type-arguments.
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            // Only simple identifier bases can trigger TS2449.  Anything else
            // (qualified name, call expression, member access) does not match
            // the tsc suppression rule.
            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(expr_idx) else {
                continue;
            };
            let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
                continue;
            };
            if symbol.flags & symbol_flags::CLASS == 0 {
                continue;
            }
            // Same-file classes only — cross-file references have no runtime
            // ordering relationship worth TDZ-checking here.
            if symbol.import_module.is_some() {
                continue;
            }
            if symbol.decl_file_idx != u32::MAX
                && symbol.decl_file_idx != self.ctx.current_file_idx as u32
            {
                continue;
            }
            let decl_idx = symbol
                .declarations
                .iter()
                .copied()
                .find(|&d| {
                    self.ctx.arena.get(d).is_some_and(|n| {
                        n.kind == syntax_kind_ext::CLASS_DECLARATION
                            || n.kind == syntax_kind_ext::CLASS_EXPRESSION
                    })
                })
                .or_else(|| symbol.primary_declaration());
            let Some(decl_idx) = decl_idx else {
                continue;
            };
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            // Ambient classes are hoisted with no TDZ semantics.
            if self.is_ambient_declaration(decl_idx) {
                continue;
            }
            let Some(usage_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };
            // TS2449 fires when usage precedes declaration in source order.
            if usage_node.pos < decl_node.pos {
                return true;
            }
        }
        false
    }
}
