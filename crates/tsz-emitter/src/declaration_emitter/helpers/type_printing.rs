//! Type/symbol printing and module path resolution

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

use super::DtsCacheResolver;

pub(crate) struct ResolvedDeclarationTypeText {
    pub(crate) type_id: tsz_solver::types::TypeId,
    pub(crate) canonical_type_text: String,
    pub(crate) emitted_type_text: String,
}

impl<'a> DeclarationEmitter<'a> {
    fn symbol_is_nameable_type_for_emit(&self, sym_id: SymbolId) -> bool {
        self.binder
            .and_then(|binder| binder.symbols.get(sym_id))
            .is_none_or(|symbol| {
                if symbol.flags & symbol_flags::TYPE_ALIAS != 0
                    && self.symbol_is_function_local_type_alias(symbol)
                {
                    return false;
                }
                symbol.flags
                    & (symbol_flags::CLASS | symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)
                    != 0
            })
    }

    fn symbol_is_function_local_type_alias(&self, symbol: &tsz_binder::Symbol) -> bool {
        symbol.declarations.iter().copied().any(|decl_idx| {
            let mut current = decl_idx;
            for _ in 0..32 {
                let Some(parent_idx) = self.arena.parent_of(current) else {
                    return false;
                };
                if !parent_idx.is_some() {
                    return false;
                }
                let Some(parent_node) = self.arena.get(parent_idx) else {
                    return false;
                };
                if self.arena.get_source_file(parent_node).is_some() {
                    return false;
                }
                if self.arena.get_function(parent_node).is_some() {
                    return true;
                }
                current = parent_idx;
            }
            false
        })
    }

    fn should_preserve_named_application_for_emit(
        &self,
        type_id: tsz_solver::types::TypeId,
        interner: &tsz_solver::TypeInterner,
    ) -> bool {
        let Some(app_id) = tsz_solver::visitor::application_id(interner, type_id) else {
            return false;
        };
        let app = interner.type_application(app_id);
        if app.args.is_empty() {
            return false;
        }
        if let Some(sym_ref) = tsz_solver::visitor::type_query_symbol(interner, app.base) {
            return self.symbol_is_nameable_type_for_emit(SymbolId(sym_ref.0));
        }
        if let Some(def_id) = tsz_solver::visitor::lazy_def_id(interner, app.base)
            && let Some(cache) = self.type_cache.as_ref()
        {
            if let Some(sym_id) = cache.def_to_symbol.get(&def_id).copied() {
                return self.symbol_is_nameable_type_for_emit(sym_id);
            }
            if cache.def_to_name.contains_key(&def_id) {
                return true;
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn should_preserve_named_application_for_inferred_emit(
        &self,
        type_id: tsz_solver::types::TypeId,
        interner: &tsz_solver::TypeInterner,
    ) -> bool {
        if !self.should_preserve_named_application_for_emit(type_id, interner) {
            return false;
        }

        let Some(app_id) = tsz_solver::visitor::application_id(interner, type_id) else {
            return true;
        };
        let app = interner.type_application(app_id);
        let Some(def_id) = tsz_solver::visitor::lazy_def_id(interner, app.base) else {
            return true;
        };
        let Some(cache) = self.type_cache.as_ref() else {
            return true;
        };
        let Some(base_type) = cache.def_types.get(&def_id.0).copied() else {
            return true;
        };

        tsz_solver::visitor::conditional_type_id(interner, base_type).is_none()
            && !self.type_contains_mapped_type_for_inferred_emit(base_type, interner, 0)
    }

    pub(in crate::declaration_emitter) fn should_expand_named_application_for_inferred_declaration(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };
        tsz_solver::visitor::application_id(interner, type_id).is_some()
            && !self.should_preserve_named_application_for_inferred_emit(type_id, interner)
    }

    pub(in crate::declaration_emitter) fn type_text_starts_with_function_local_type_alias(
        &self,
        type_text: &str,
    ) -> bool {
        let Some(alias_name) = Self::leading_type_reference_name_for_emit(type_text) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };

        binder.symbols.iter().any(|symbol| {
            symbol.escaped_name == alias_name
                && symbol.flags & symbol_flags::TYPE_ALIAS != 0
                && self.symbol_is_function_local_type_alias(symbol)
        })
    }

    fn leading_type_reference_name_for_emit(type_text: &str) -> Option<&str> {
        let trimmed = type_text.trim_start();
        if trimmed.starts_with("import(") || trimmed.starts_with("typeof ") {
            return None;
        }
        let end = trimmed
            .char_indices()
            .find_map(|(idx, ch)| {
                (!(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())).then_some(idx)
            })
            .unwrap_or(trimmed.len());
        if end == 0 {
            return None;
        }
        let name = &trimmed[..end];
        name.chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
            .then_some(name)
    }

    fn type_contains_mapped_type_for_inferred_emit(
        &self,
        type_id: tsz_solver::types::TypeId,
        interner: &tsz_solver::TypeInterner,
        depth: usize,
    ) -> bool {
        if depth > 16 {
            return false;
        }
        let Some(type_data) = interner.lookup(type_id) else {
            return false;
        };

        match type_data {
            tsz_solver::types::TypeData::Mapped(_) => true,
            tsz_solver::types::TypeData::Lazy(def_id) => self
                .type_cache
                .as_ref()
                .and_then(|cache| cache.def_types.get(&def_id.0).copied())
                .is_some_and(|resolved| {
                    self.type_contains_mapped_type_for_inferred_emit(resolved, interner, depth + 1)
                }),
            tsz_solver::types::TypeData::Application(app_id) => {
                let app = interner.type_application(app_id);
                self.type_contains_mapped_type_for_inferred_emit(app.base, interner, depth + 1)
                    || app.args.iter().copied().any(|arg| {
                        self.type_contains_mapped_type_for_inferred_emit(arg, interner, depth + 1)
                    })
            }
            tsz_solver::types::TypeData::Union(list_id)
            | tsz_solver::types::TypeData::Intersection(list_id) => {
                interner.type_list(list_id).iter().copied().any(|member| {
                    self.type_contains_mapped_type_for_inferred_emit(member, interner, depth + 1)
                })
            }
            tsz_solver::types::TypeData::Array(elem)
            | tsz_solver::types::TypeData::ReadonlyType(elem)
            | tsz_solver::types::TypeData::KeyOf(elem)
            | tsz_solver::types::TypeData::NoInfer(elem) => {
                self.type_contains_mapped_type_for_inferred_emit(elem, interner, depth + 1)
            }
            tsz_solver::types::TypeData::IndexAccess(object, index) => {
                self.type_contains_mapped_type_for_inferred_emit(object, interner, depth + 1)
                    || self.type_contains_mapped_type_for_inferred_emit(index, interner, depth + 1)
            }
            tsz_solver::types::TypeData::StringIntrinsic { type_arg, .. } => {
                self.type_contains_mapped_type_for_inferred_emit(type_arg, interner, depth + 1)
            }
            _ => false,
        }
    }

    fn display_alias_for_declaration_emit(
        &self,
        type_id: tsz_solver::types::TypeId,
        interner: &tsz_solver::TypeInterner,
    ) -> tsz_solver::types::TypeId {
        self.display_alias_for_policy(
            type_id,
            interner,
            Self::should_preserve_named_application_for_emit,
        )
    }

    fn display_alias_for_policy(
        &self,
        type_id: tsz_solver::types::TypeId,
        interner: &tsz_solver::TypeInterner,
        preserve_named_application: fn(
            &Self,
            tsz_solver::types::TypeId,
            &tsz_solver::TypeInterner,
        ) -> bool,
    ) -> tsz_solver::types::TypeId {
        interner
            .get_display_alias(type_id)
            .filter(|&alias| preserve_named_application(self, alias, interner))
            .unwrap_or(type_id)
    }

    fn apply_display_aliases_to_preserved_application_args(
        &self,
        type_id: tsz_solver::types::TypeId,
        interner: &tsz_solver::TypeInterner,
        preserve_named_application: fn(
            &Self,
            tsz_solver::types::TypeId,
            &tsz_solver::TypeInterner,
        ) -> bool,
    ) -> tsz_solver::types::TypeId {
        let Some(app_id) = tsz_solver::visitor::application_id(interner, type_id) else {
            return type_id;
        };
        let app = interner.type_application(app_id);
        if app.args.is_empty() {
            return type_id;
        }

        let mut changed = false;
        let args = app
            .args
            .iter()
            .copied()
            .map(|arg| {
                let evaluated = if let Some(cache) = &self.type_cache {
                    let resolver = DtsCacheResolver { cache };
                    let mut evaluator =
                        tsz_solver::TypeEvaluator::with_resolver(interner, &resolver)
                            .with_expanded_application_display_alias_args();
                    evaluator.set_max_mapped_keys(1_024);
                    evaluator.evaluate(arg)
                } else {
                    let mut evaluator = tsz_solver::TypeEvaluator::new(interner)
                        .with_expanded_application_display_alias_args();
                    evaluator.set_max_mapped_keys(1_024);
                    evaluator.evaluate(arg)
                };
                let aliased =
                    self.display_alias_for_policy(evaluated, interner, preserve_named_application);
                changed |= aliased != arg;
                aliased
            })
            .collect::<Vec<_>>();

        if changed {
            interner.application(app.base, args)
        } else {
            type_id
        }
    }

    fn reduce_conditional_alias_application_for_inferred_emit(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;
        let cache = self.type_cache.as_ref()?;
        let app_id = tsz_solver::visitor::application_id(interner, type_id)?;
        let app = interner.type_application(app_id);
        let def_id = tsz_solver::visitor::lazy_def_id(interner, app.base)?;
        let body = cache.def_types.get(&def_id.0).copied()?;
        tsz_solver::visitor::conditional_type_id(interner, body)?;

        let type_params = cache.def_type_params.get(&def_id.0)?;
        let instantiated = tsz_solver::instantiate_generic(interner, body, type_params, &app.args);
        let resolver = DtsCacheResolver { cache };
        let mut evaluator = tsz_solver::TypeEvaluator::with_resolver(interner, &resolver);
        evaluator.set_max_mapped_keys(1_024);
        Some(evaluator.evaluate(instantiated))
    }

    fn reduce_conditional_aliases_for_inferred_emit(
        &self,
        type_id: tsz_solver::types::TypeId,
        depth: usize,
    ) -> tsz_solver::types::TypeId {
        if depth > 16 {
            return type_id;
        }

        if let Some(reduced) = self.reduce_conditional_alias_application_for_inferred_emit(type_id)
            && reduced != type_id
        {
            return self.reduce_conditional_aliases_for_inferred_emit(reduced, depth + 1);
        }

        let Some(interner) = self.type_interner else {
            return type_id;
        };
        let Some(type_data) = interner.lookup(type_id) else {
            return type_id;
        };
        match type_data {
            tsz_solver::types::TypeData::Application(app_id) => {
                let app = interner.type_application(app_id);
                let mut changed = false;
                let args = app
                    .args
                    .iter()
                    .copied()
                    .map(|arg| {
                        let reduced =
                            self.reduce_conditional_aliases_for_inferred_emit(arg, depth + 1);
                        changed |= reduced != arg;
                        reduced
                    })
                    .collect::<Vec<_>>();
                if changed {
                    interner.application(app.base, args)
                } else {
                    type_id
                }
            }
            tsz_solver::types::TypeData::Function(shape_id) => {
                let shape = interner.function_shape(shape_id);
                let mut changed = false;
                let params = shape
                    .params
                    .iter()
                    .copied()
                    .map(|mut param| {
                        let reduced = self
                            .reduce_conditional_aliases_for_inferred_emit(param.type_id, depth + 1);
                        changed |= reduced != param.type_id;
                        param.type_id = reduced;
                        param
                    })
                    .collect::<Vec<_>>();
                let this_type = shape.this_type.map(|this_type| {
                    let reduced =
                        self.reduce_conditional_aliases_for_inferred_emit(this_type, depth + 1);
                    changed |= reduced != this_type;
                    reduced
                });
                let return_type =
                    self.reduce_conditional_aliases_for_inferred_emit(shape.return_type, depth + 1);
                changed |= return_type != shape.return_type;
                if changed {
                    interner.function(tsz_solver::types::FunctionShape {
                        type_params: shape.type_params.clone(),
                        params,
                        this_type,
                        return_type,
                        type_predicate: shape.type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
                } else {
                    type_id
                }
            }
            tsz_solver::types::TypeData::Conditional(cond_id) => {
                let cond = interner.get_conditional(cond_id);
                let check_type =
                    self.reduce_conditional_aliases_for_inferred_emit(cond.check_type, depth + 1);
                let extends_type =
                    self.reduce_conditional_aliases_for_inferred_emit(cond.extends_type, depth + 1);
                let true_type =
                    self.reduce_conditional_aliases_for_inferred_emit(cond.true_type, depth + 1);
                let false_type =
                    self.reduce_conditional_aliases_for_inferred_emit(cond.false_type, depth + 1);
                if check_type == cond.check_type
                    && extends_type == cond.extends_type
                    && true_type == cond.true_type
                    && false_type == cond.false_type
                {
                    return type_id;
                }
                let reduced_cond = interner.conditional(tsz_solver::types::ConditionalType {
                    check_type,
                    extends_type,
                    true_type,
                    false_type,
                    is_distributive: cond.is_distributive,
                });
                let evaluated = if let Some(cache) = &self.type_cache {
                    let resolver = DtsCacheResolver { cache };
                    let mut evaluator =
                        tsz_solver::TypeEvaluator::with_resolver(interner, &resolver);
                    evaluator.set_max_mapped_keys(1_024);
                    evaluator.evaluate(reduced_cond)
                } else {
                    let mut evaluator = tsz_solver::TypeEvaluator::new(interner);
                    evaluator.set_max_mapped_keys(1_024);
                    evaluator.evaluate(reduced_cond)
                };
                self.reduce_conditional_aliases_for_inferred_emit(evaluated, depth + 1)
            }
            _ => type_id,
        }
    }

    pub(in crate::declaration_emitter) fn type_contains_conditional_alias_application_for_inferred_emit(
        &self,
        type_id: tsz_solver::types::TypeId,
        depth: usize,
    ) -> bool {
        if depth > 16 {
            return false;
        }
        let (Some(interner), Some(cache)) = (self.type_interner, self.type_cache.as_ref()) else {
            return false;
        };
        let Some(type_data) = interner.lookup(type_id) else {
            return false;
        };
        match type_data {
            tsz_solver::types::TypeData::Application(app_id) => {
                let app = interner.type_application(app_id);
                if let Some(def_id) = tsz_solver::visitor::lazy_def_id(interner, app.base)
                    && let Some(body) = cache.def_types.get(&def_id.0).copied()
                    && tsz_solver::visitor::conditional_type_id(interner, body).is_some()
                {
                    return true;
                }
                app.args.iter().copied().any(|arg| {
                    self.type_contains_conditional_alias_application_for_inferred_emit(
                        arg,
                        depth + 1,
                    )
                })
            }
            tsz_solver::types::TypeData::Function(shape_id) => {
                let shape = interner.function_shape(shape_id);
                shape.params.iter().any(|param| {
                    self.type_contains_conditional_alias_application_for_inferred_emit(
                        param.type_id,
                        depth + 1,
                    )
                }) || shape.this_type.is_some_and(|this_type| {
                    self.type_contains_conditional_alias_application_for_inferred_emit(
                        this_type,
                        depth + 1,
                    )
                }) || self.type_contains_conditional_alias_application_for_inferred_emit(
                    shape.return_type,
                    depth + 1,
                )
            }
            tsz_solver::types::TypeData::Conditional(cond_id) => {
                let cond = interner.get_conditional(cond_id);
                self.type_contains_conditional_alias_application_for_inferred_emit(
                    cond.check_type,
                    depth + 1,
                ) || self.type_contains_conditional_alias_application_for_inferred_emit(
                    cond.extends_type,
                    depth + 1,
                ) || self.type_contains_conditional_alias_application_for_inferred_emit(
                    cond.true_type,
                    depth + 1,
                ) || self.type_contains_conditional_alias_application_for_inferred_emit(
                    cond.false_type,
                    depth + 1,
                )
            }
            _ => false,
        }
    }

    pub(crate) fn get_node_type_or_names(
        &self,
        node_ids: &[NodeIndex],
    ) -> Option<tsz_solver::types::TypeId> {
        for &node_id in node_ids {
            if let Some(type_id) = self.get_node_type(node_id) {
                return Some(type_id);
            }

            if let Some(type_id) = self.recover_expression_type_from_structure(node_id) {
                return Some(type_id);
            }

            let Some(node) = self.arena.get(node_id) else {
                continue;
            };

            for related_id in self.get_node_type_related_nodes(node) {
                if let Some(type_id) = self.get_node_type(related_id) {
                    return Some(type_id);
                }

                if let Some(type_id) = self.recover_expression_type_from_structure(related_id) {
                    return Some(type_id);
                }
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn recover_expression_type_from_structure(
        &self,
        node_id: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let node = self.arena.get(node_id)?;
        let interner = self.type_interner?;

        match node.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(node)?;
                let callee_type = self
                    .get_node_type_or_names(&[call.expression])
                    .or_else(|| self.get_type_via_symbol(call.expression))?;
                // Guard: do not use the un-instantiated return type of a
                // generic function/callable.  Free type variables cannot be
                // resolved without inference from the checker.
                match interner.lookup(callee_type) {
                    Some(tsz_solver::types::TypeData::Function(sid))
                        if !interner.function_shape(sid).type_params.is_empty() =>
                    {
                        return None;
                    }
                    Some(tsz_solver::types::TypeData::Callable(sid))
                        if interner
                            .callable_shape(sid)
                            .call_signatures
                            .iter()
                            .any(|s| !s.type_params.is_empty()) =>
                    {
                        return None;
                    }
                    _ => {}
                }
                tsz_solver::type_queries::get_return_type(interner, callee_type)
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::AWAIT_EXPRESSION =>
            {
                let inner = self.arena.get_unary_expr_ex(node)?.expression;
                self.get_node_type_or_names(&[inner])
                    .or_else(|| self.get_type_via_symbol(inner))
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let inner = self.arena.get_unary_expr_ex(node)?.expression;
                self.get_node_type_or_names(&[inner])
                    .or_else(|| self.get_type_via_symbol(inner))
            }
            _ => None,
        }
    }

    pub(crate) fn get_node_type_related_nodes(&self, node: &Node) -> Vec<NodeIndex> {
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    let mut related = Vec::with_capacity(1);
                    if decl.initializer.is_some() {
                        related.push(decl.initializer);
                    }
                    related.push(decl.type_annotation);
                    related
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(decl) = self.arena.get_property_decl(node) {
                    let mut related = Vec::with_capacity(2);
                    if decl.initializer.is_some() {
                        related.push(decl.initializer);
                    }
                    related.push(decl.type_annotation);
                    related
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                if let Some(param) = self.arena.get_parameter(node) {
                    if param.initializer.is_some() {
                        vec![param.initializer]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access_expr) = self.arena.get_access_expr(node) {
                    vec![access_expr.expression, access_expr.name_or_argument]
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access_expr) = self.arena.get_access_expr(node) {
                    vec![access_expr.expression, access_expr.name_or_argument]
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(query) = self.arena.get_type_query(node) {
                    vec![query.expr_name]
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    vec![unary.expression]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn print_type_id_with_policy(
        &self,
        type_id: tsz_solver::types::TypeId,
        preserve_named_application: fn(
            &Self,
            tsz_solver::types::TypeId,
            &tsz_solver::TypeInterner,
        ) -> bool,
    ) -> String {
        if let Some(interner) = self.type_interner {
            let type_id =
                self.display_alias_for_policy(type_id, interner, preserve_named_application);
            // Evaluate the type before printing to expand mapped types over
            // literal union constraints (e.g., `{[k in "ar"|"bg"]?: T}` becomes
            // `{ar?: T; bg?: T}`).  This matches tsc's behavior in declaration
            // emit where mapped types are fully resolved.
            let type_id = if preserve_named_application(self, type_id, interner) {
                let evaluated = if let Some(cache) = &self.type_cache {
                    let resolver = DtsCacheResolver { cache };
                    let mut evaluator =
                        tsz_solver::TypeEvaluator::with_resolver(interner, &resolver)
                            .with_expanded_application_display_alias_args();
                    evaluator.set_max_mapped_keys(1_024);
                    evaluator.evaluate(type_id)
                } else {
                    let mut evaluator = tsz_solver::TypeEvaluator::new(interner)
                        .with_expanded_application_display_alias_args();
                    evaluator.set_max_mapped_keys(1_024);
                    evaluator.evaluate(type_id)
                };
                let alias =
                    self.display_alias_for_policy(evaluated, interner, preserve_named_application);
                let alias = if preserve_named_application(self, alias, interner) {
                    alias
                } else {
                    type_id
                };
                self.apply_display_aliases_to_preserved_application_args(
                    alias,
                    interner,
                    preserve_named_application,
                )
            } else if let Some(cache) = &self.type_cache {
                let resolver = DtsCacheResolver { cache };
                let mut evaluator = tsz_solver::TypeEvaluator::with_resolver(interner, &resolver);
                evaluator.set_max_mapped_keys(1_024);
                let evaluated = evaluator.evaluate(type_id);
                self.display_alias_for_policy(evaluated, interner, preserve_named_application)
            } else {
                let mut evaluator = tsz_solver::TypeEvaluator::new(interner);
                evaluator.set_max_mapped_keys(1_024);
                let evaluated = evaluator.evaluate(type_id);
                self.display_alias_for_policy(evaluated, interner, preserve_named_application)
            };

            let module_path_resolver = |sym_id| self.resolve_symbol_module_path(sym_id);
            let namespace_alias_resolver = |sym_id| self.resolve_namespace_import_alias(sym_id);
            let local_import_alias_name_resolver =
                |sym_id| self.can_reference_local_import_alias_by_name(sym_id);
            let has_local_import_alias_resolver = |sym_id| {
                if let Some(binder) = self.binder {
                    self.symbol_has_local_import_alias(binder, sym_id)
                } else {
                    false
                }
            };
            let mut printer = TypePrinter::new(interner)
                .with_indent_level(self.indent_level)
                .with_node_arena(self.arena)
                .with_module_path_resolver(&module_path_resolver)
                .with_namespace_alias_resolver(&namespace_alias_resolver)
                .with_local_import_alias_name_resolver(&local_import_alias_name_resolver)
                .with_has_local_import_alias_resolver(&has_local_import_alias_resolver)
                .with_strict_null_checks(self.strict_null_checks);

            // Add symbol arena if available for visibility checking
            if let Some(binder) = self.binder {
                printer = printer.with_symbols(&binder.symbols);
            }

            // Add type cache if available for resolving Lazy(DefId) types
            if let Some(cache) = &self.type_cache {
                printer = printer.with_type_cache(cache);
            }

            // Set enclosing namespace for context-relative qualified names
            if let Some(enc_sym) = self.enclosing_namespace_symbol {
                printer = printer.with_enclosing_symbol(enc_sym);
            }

            printer.print_type(type_id)
        } else {
            // Fallback if no interner available
            "any".to_string()
        }
    }

    /// Print a `TypeId` as TypeScript syntax using `TypePrinter`.
    pub(crate) fn print_type_id(&self, type_id: tsz_solver::types::TypeId) -> String {
        let printed = self
            .print_type_id_with_policy(type_id, Self::should_preserve_named_application_for_emit);
        self.expand_imported_indexed_access_type_text(&printed)
            .unwrap_or(printed)
    }

    pub(crate) fn print_type_id_for_inferred_declaration(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> String {
        let elided_alias_names = self.function_local_type_alias_application_names(type_id);
        let type_id = if let Some(interner) = self.type_interner {
            self.display_alias_for_policy(
                type_id,
                interner,
                Self::should_preserve_named_application_for_inferred_emit,
            )
        } else {
            type_id
        };
        let type_id = self.reduce_conditional_aliases_for_inferred_emit(type_id, 0);
        let printed = self.print_type_id_with_policy(
            type_id,
            Self::should_preserve_named_application_for_inferred_emit,
        );
        let printed = if elided_alias_names.is_empty() {
            printed
        } else {
            Self::elide_type_reference_names(&printed, &elided_alias_names)
        };
        let printed =
            Self::simplify_inexact_optional_mapped_intersection_text(&printed).unwrap_or(printed);
        let printed = self
            .expand_imported_indexed_access_type_text(&printed)
            .unwrap_or(printed);
        if Self::contains_portable_mapped_object_text(&printed)
            && let Some(expanded) =
                self.expand_portable_intersection_type_text(self.arena, &printed)
        {
            expanded
        } else {
            printed
        }
    }

    pub(crate) fn print_type_id_expanded_for_inferred_declaration(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> String {
        let type_id = self.reduce_conditional_aliases_for_inferred_emit(type_id, 0);
        self.print_type_id_with_policy(type_id, |_, _, _| false)
    }

    pub(in crate::declaration_emitter) fn qualify_current_namespace_self_type_text(
        &self,
        type_text: &str,
    ) -> String {
        let (alias, default_name, export_names);
        let (alias_ref, default_name_ref, export_names_ref) =
            if let (Some(alias), Some(default_name)) = (
                self.current_namespace_self_import_alias.as_deref(),
                self.current_namespace_shadowed_default_name.as_deref(),
            ) {
                (
                    alias,
                    default_name,
                    &self.current_namespace_self_export_names,
                )
            } else if self.inside_declare_namespace {
                let Some(computed_alias) = self.self_namespace_import_alias() else {
                    return type_text.to_string();
                };
                let Some(computed_default_name) = self.default_exported_local_name() else {
                    return type_text.to_string();
                };
                if !self.source_has_namespace_shadowing_name(&computed_default_name) {
                    return type_text.to_string();
                }
                let mut computed_export_names = self.top_level_self_exported_names();
                computed_export_names.insert(computed_default_name.clone());
                alias = computed_alias;
                default_name = computed_default_name;
                export_names = computed_export_names;
                (alias.as_str(), default_name.as_str(), &export_names)
            } else {
                return type_text.to_string();
            };
        if export_names_ref.is_empty() {
            return type_text.to_string();
        }

        let bytes = type_text.as_bytes();
        let mut out = String::with_capacity(type_text.len() + alias_ref.len());
        let mut i = 0;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if ch == '"' || ch == '\'' {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let current = bytes[i] as char;
                    if current == '\\' {
                        i = (i + 2).min(bytes.len());
                        continue;
                    }
                    i += 1;
                    if current == ch {
                        break;
                    }
                }
                out.push_str(&type_text[start..i]);
                continue;
            }

            if !Self::is_type_reference_identifier_start(ch) {
                out.push(ch);
                i += 1;
                continue;
            }

            let start = i;
            i += 1;
            while i < bytes.len() && Self::is_type_reference_identifier_continue(bytes[i] as char) {
                i += 1;
            }
            let ident = &type_text[start..i];
            let already_qualified = start > 0 && bytes[start - 1] == b'.';
            if already_qualified || !export_names_ref.contains(ident) {
                out.push_str(ident);
                continue;
            }

            out.push_str(alias_ref);
            out.push('.');
            if ident == default_name_ref {
                out.push_str("default");
            } else {
                out.push_str(ident);
            }
        }
        out
    }

    fn function_local_type_alias_application_names(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        self.collect_function_local_type_alias_application_names(type_id, &mut names, 0);
        names
    }

    fn collect_function_local_type_alias_application_names(
        &self,
        type_id: tsz_solver::types::TypeId,
        names: &mut FxHashSet<String>,
        depth: usize,
    ) {
        if depth > 16 {
            return;
        }
        let (Some(interner), Some(cache)) = (self.type_interner, self.type_cache.as_ref()) else {
            return;
        };
        let Some(type_data) = interner.lookup(type_id) else {
            return;
        };
        match type_data {
            tsz_solver::types::TypeData::Application(app_id) => {
                let app = interner.type_application(app_id);
                if let Some(def_id) = tsz_solver::visitor::lazy_def_id(interner, app.base)
                    && let Some(sym_id) = cache.def_to_symbol.get(&def_id).copied()
                    && let Some(symbol) = self.binder.and_then(|binder| binder.symbols.get(sym_id))
                    && symbol.flags & symbol_flags::TYPE_ALIAS != 0
                    && self.symbol_is_function_local_type_alias(symbol)
                    && let Some(name) = cache.def_to_name.get(&def_id)
                {
                    names.insert(name.clone());
                }
                self.collect_function_local_type_alias_application_names(
                    app.base,
                    names,
                    depth + 1,
                );
                for arg in app.args.iter().copied() {
                    self.collect_function_local_type_alias_application_names(arg, names, depth + 1);
                }
            }
            tsz_solver::types::TypeData::Union(members)
            | tsz_solver::types::TypeData::Intersection(members) => {
                for member in interner.type_list(members).iter().copied() {
                    self.collect_function_local_type_alias_application_names(
                        member,
                        names,
                        depth + 1,
                    );
                }
            }
            _ => {}
        }
    }

    fn elide_type_reference_names(type_text: &str, names: &FxHashSet<String>) -> String {
        let bytes = type_text.as_bytes();
        let mut out = String::with_capacity(type_text.len());
        let mut i = 0;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if ch == '"' || ch == '\'' {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let current = bytes[i] as char;
                    if current == '\\' {
                        i = (i + 2).min(bytes.len());
                        continue;
                    }
                    i += 1;
                    if current == ch {
                        break;
                    }
                }
                out.push_str(&type_text[start..i]);
                continue;
            }
            if !Self::is_type_reference_identifier_start(ch) {
                out.push(ch);
                i += 1;
                continue;
            }

            let start = i;
            i += 1;
            while i < bytes.len() && Self::is_type_reference_identifier_continue(bytes[i] as char) {
                i += 1;
            }
            let ident = &type_text[start..i];
            let prev_non_ws = type_text[..start]
                .chars()
                .rev()
                .find(|c| !c.is_ascii_whitespace());
            if prev_non_ws == Some('.') || !names.contains(ident) {
                out.push_str(ident);
                continue;
            }

            let mut end = i;
            while end < bytes.len() && (bytes[end] as char).is_ascii_whitespace() {
                end += 1;
            }
            if end < bytes.len()
                && bytes[end] as char == '<'
                && let Some(type_arg_end) = Self::type_reference_type_argument_end(type_text, end)
            {
                if let Some(type_arg) =
                    Self::single_type_argument_text(type_text, end, type_arg_end)
                {
                    out.push_str(type_arg);
                    out.push_str(" | /*elided*/ any");
                } else {
                    out.push_str("/*elided*/ any");
                }
                i = type_arg_end;
                continue;
            }
            out.push_str("/*elided*/ any");
        }
        out
    }

    pub(in crate::declaration_emitter) fn simplify_inexact_optional_mapped_intersection_text(
        type_text: &str,
    ) -> Option<String> {
        let start = type_text.find("{} & {")?;
        let first_start = start + "{} & ".len();
        let first_end = Self::balanced_brace_end(type_text, first_start)?;
        let mut next = Self::skip_ascii_whitespace(type_text, first_end)?;
        if !type_text.get(next..)?.starts_with('&') {
            return None;
        }
        next += 1;
        next = Self::skip_ascii_whitespace(type_text, next)?;
        if !type_text.get(next..)?.starts_with('{') {
            return None;
        }
        let second_end = Self::balanced_brace_end(type_text, next)?;
        let candidate = type_text.get(start..second_end)?;
        if !candidate.contains("as undefined extends")
            || !candidate.contains("[keyof unknown]")
            || !candidate.contains("? keyof unknown : never")
            || !candidate.contains("? never : keyof unknown")
        {
            return None;
        }

        let source_object = Self::inexact_optional_source_object_text(candidate)?;
        let simplified = Self::inexact_optional_object_intersection_text(&source_object)?;
        let mut output =
            String::with_capacity(type_text.len() - candidate.len() + simplified.len());
        output.push_str(type_text.get(..start)?);
        output.push_str(&simplified);
        output.push_str(type_text.get(second_end..)?);
        Some(output)
    }

    fn inexact_optional_source_object_text(candidate: &str) -> Option<String> {
        let marker = "undefined extends";
        let marker_start = candidate.find(marker)? + marker.len();
        let object_start = Self::skip_ascii_whitespace(candidate, marker_start)?;
        if !candidate.get(object_start..)?.starts_with('{') {
            return None;
        }
        let object_end = Self::balanced_brace_end(candidate, object_start)?;
        candidate.get(object_start..object_end).map(str::to_string)
    }

    pub(in crate::declaration_emitter) fn inexact_optional_object_intersection_text(
        source_object: &str,
    ) -> Option<String> {
        let inner = source_object.trim().strip_prefix('{')?.strip_suffix('}')?;
        let members = Self::split_object_members(inner);
        if members.is_empty() {
            return None;
        }

        let mut optional_members = Vec::new();
        let mut required_members = Vec::new();
        for member in members {
            let (name, explicit_optional, type_text) = Self::parse_object_property_member(&member)?;
            let type_includes_undefined = Self::type_text_contains_undefined(type_text);
            if explicit_optional || type_includes_undefined {
                let optional_name = name.strip_suffix('?').unwrap_or(name).trim();
                let optional_type = if type_includes_undefined {
                    type_text.to_string()
                } else {
                    format!("{type_text} | undefined")
                };
                optional_members.push(format!("    {optional_name}?: {optional_type};"));
            } else {
                required_members.push(format!("    {name}: {type_text};"));
            }
        }

        if optional_members.is_empty() || required_members.is_empty() {
            return None;
        }

        Some(format!(
            "{{\n{}\n}} & {{\n{}\n}}",
            optional_members.join("\n"),
            required_members.join("\n")
        ))
    }

    fn split_object_members(inner: &str) -> Vec<String> {
        let mut members = Vec::new();
        let mut start = 0usize;
        for idx in Self::top_level_byte_indices(inner, b';') {
            let member = inner.get(start..idx).map(str::trim).unwrap_or_default();
            if !member.is_empty() {
                members.push(member.to_string());
            }
            start = idx + 1;
        }
        let tail = inner.get(start..).map(str::trim).unwrap_or_default();
        if !tail.is_empty() {
            members.push(tail.to_string());
        }
        members
    }

    fn parse_object_property_member(member: &str) -> Option<(&str, bool, &str)> {
        let colon = Self::top_level_byte_indices(member, b':')
            .into_iter()
            .next()?;
        let name = member.get(..colon)?.trim();
        let type_text = member.get(colon + 1..)?.trim();
        let explicit_optional = name.ends_with('?');
        Some((name, explicit_optional, type_text))
    }

    fn type_text_contains_undefined(type_text: &str) -> bool {
        let bytes = type_text.as_bytes();
        let needle = b"undefined";
        let mut i = 0usize;
        while i + needle.len() <= bytes.len() {
            if &bytes[i..i + needle.len()] == needle {
                let before_ok = i == 0 || !Self::is_ident_char(bytes[i - 1]);
                let after = i + needle.len();
                let after_ok = after == bytes.len() || !Self::is_ident_char(bytes[after]);
                if before_ok && after_ok {
                    return true;
                }
                i += needle.len();
            } else {
                i += 1;
            }
        }
        false
    }

    pub(in crate::declaration_emitter) fn type_text_has_undefined_branch(type_text: &str) -> bool {
        let mut text = type_text.trim();
        while let Some(inner) = Self::strip_balanced_outer_parens(text) {
            text = inner.trim();
        }

        if text == "undefined" {
            return true;
        }

        let union_indices = Self::top_level_byte_indices(text, b'|');
        if union_indices.is_empty() {
            return false;
        }

        let mut start = 0usize;
        for index in union_indices {
            if Self::type_text_has_undefined_branch(&text[start..index]) {
                return true;
            }
            start = index + 1;
        }
        Self::type_text_has_undefined_branch(&text[start..])
    }

    pub(in crate::declaration_emitter) fn type_annotation_semantically_includes_undefined(
        &self,
        type_annotation: NodeIndex,
    ) -> bool {
        type_annotation.is_some()
            && (self
                .emit_type_node_text(type_annotation)
                .is_some_and(|type_text| self.type_text_or_alias_includes_undefined(&type_text, 0))
                || self.type_node_semantically_includes_undefined(type_annotation, 0))
    }

    pub(in crate::declaration_emitter) fn emitted_type_text_semantically_includes_undefined(
        &self,
        type_text: &str,
    ) -> bool {
        self.type_text_or_alias_includes_undefined(type_text, 0)
    }

    fn type_text_or_alias_includes_undefined(&self, type_text: &str, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        let mut text = type_text.trim();
        while let Some(inner) = Self::strip_balanced_outer_parens(text) {
            text = inner.trim();
        }

        if text == "undefined" {
            return true;
        }

        let union_indices = Self::top_level_byte_indices(text, b'|');
        if !union_indices.is_empty() {
            let mut start = 0usize;
            for index in union_indices {
                if self.type_text_or_alias_includes_undefined(&text[start..index], depth + 1) {
                    return true;
                }
                start = index + 1;
            }
            return self.type_text_or_alias_includes_undefined(&text[start..], depth + 1);
        }

        if let Some((name, args)) = Self::parse_utility_type_text(text) {
            return match name {
                "Exclude" => {
                    let first_includes_undefined = args.first().is_some_and(|arg| {
                        self.type_text_or_alias_includes_undefined(arg, depth + 1)
                    });
                    let excluded_includes_undefined = args.get(1).is_some_and(|arg| {
                        self.type_text_or_alias_includes_undefined(arg, depth + 1)
                    });
                    first_includes_undefined && !excluded_includes_undefined
                }
                "Extract" => {
                    args.first().is_some_and(|arg| {
                        self.type_text_or_alias_includes_undefined(arg, depth + 1)
                    }) && args.get(1).is_some_and(|arg| {
                        self.type_text_or_alias_includes_undefined(arg, depth + 1)
                    })
                }
                _ => false,
            };
        }
        if Self::is_simple_identifier_text(text) {
            return self
                .find_local_type_alias_type_node(text)
                .or_else(|| self.current_file_type_alias_type_node_by_name(text))
                .and_then(|alias_type| self.emit_type_node_text(alias_type))
                .is_some_and(|alias_text| {
                    self.type_text_or_alias_includes_undefined(&alias_text, depth + 1)
                });
        }
        false
    }

    fn parse_utility_type_text(text: &str) -> Option<(&str, Vec<&str>)> {
        let lt = text.find('<')?;
        if !text.ends_with('>') {
            return None;
        }
        let name = text[..lt].trim();
        if !matches!(name, "Exclude" | "Extract") {
            return None;
        }
        let inner = &text[lt + 1..text.len() - 1];
        Some((name, Self::split_top_level_commas(inner)))
    }

    pub(in crate::declaration_emitter) fn type_node_semantically_includes_undefined(
        &self,
        type_idx: NodeIndex,
        depth: usize,
    ) -> bool {
        if depth > 8 {
            return false;
        }

        let Some(type_node) = self.arena.get(type_idx) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.arena.get_type_ref(type_node) else {
            return false;
        };
        let Some(name) = self.identifier_text_from_arena(self.arena, type_ref.type_name) else {
            return false;
        };

        match name.as_str() {
            "NonNullable" => false,
            "Exclude" => {
                let Some(args) = type_ref.type_arguments.as_ref() else {
                    return false;
                };
                let first_includes_undefined =
                    args.nodes.first().copied().is_some_and(|arg| {
                        self.type_node_or_alias_includes_undefined(arg, depth + 1)
                    });
                let excluded_includes_undefined =
                    args.nodes.get(1).copied().is_some_and(|arg| {
                        self.type_node_or_alias_includes_undefined(arg, depth + 1)
                    });
                first_includes_undefined && !excluded_includes_undefined
            }
            "Extract" => {
                let Some(args) = type_ref.type_arguments.as_ref() else {
                    return false;
                };
                args.nodes
                    .first()
                    .copied()
                    .is_some_and(|arg| self.type_node_or_alias_includes_undefined(arg, depth + 1))
                    && args.nodes.get(1).copied().is_some_and(|arg| {
                        self.type_node_or_alias_includes_undefined(arg, depth + 1)
                    })
            }
            _ => self
                .find_local_type_alias_type_node(&name)
                .or_else(|| self.current_file_type_alias_type_node_by_name(&name))
                .is_some_and(|alias_type| {
                    self.type_node_or_alias_includes_undefined(alias_type, depth + 1)
                }),
        }
    }

    fn current_file_type_alias_type_node_by_name(&self, name: &str) -> Option<NodeIndex> {
        let source_file = self
            .current_source_file_idx
            .and_then(|idx| self.arena.get(idx))
            .and_then(|node| self.arena.get_source_file(node))?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            let alias = self.arena.get_type_alias(stmt_node)?;
            if self.get_identifier_text(alias.name).as_deref() == Some(name) {
                return Some(alias.type_node);
            }
        }
        None
    }

    fn type_node_or_alias_includes_undefined(&self, type_idx: NodeIndex, depth: usize) -> bool {
        self.emit_type_node_text(type_idx)
            .is_some_and(|type_text| Self::type_text_has_undefined_branch(&type_text))
            || self.type_node_semantically_includes_undefined(type_idx, depth)
    }

    fn strip_balanced_outer_parens(text: &str) -> Option<&str> {
        let bytes = text.as_bytes();
        if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
            return None;
        }

        let mut depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut escaped = false;

        for (i, &byte) in bytes.iter().enumerate() {
            if let Some(q) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == q {
                    quote = None;
                }
                continue;
            }

            match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'(' => depth += 1,
                b')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 && i != bytes.len() - 1 {
                        return None;
                    }
                }
                _ => {}
            }
        }

        (depth == 0).then_some(&text[1..text.len() - 1])
    }

    fn skip_ascii_whitespace(text: &str, start: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut i = start;
        while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        Some(i)
    }

    fn top_level_byte_indices(text: &str, target: u8) -> Vec<usize> {
        let bytes = text.as_bytes();
        let mut indices = Vec::new();
        let mut brace_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut i = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            if let Some(q) = quote {
                if b == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if b == q {
                    quote = None;
                }
                i += 1;
                continue;
            }

            match b {
                b'\'' | b'"' | b'`' => quote = Some(b),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                _ if b == target
                    && brace_depth == 0
                    && bracket_depth == 0
                    && paren_depth == 0
                    && angle_depth == 0 =>
                {
                    indices.push(i);
                }
                _ => {}
            }
            i += 1;
        }
        indices
    }

    fn balanced_brace_end(text: &str, start: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        if bytes.get(start).copied() != Some(b'{') {
            return None;
        }
        let mut depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut i = start;
        while i < bytes.len() {
            let b = bytes[i];
            if let Some(q) = quote {
                if b == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if b == q {
                    quote = None;
                }
                i += 1;
                continue;
            }

            match b {
                b'\'' | b'"' | b'`' => quote = Some(b),
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn type_reference_type_argument_end(type_text: &str, start: usize) -> Option<usize> {
        let bytes = type_text.as_bytes();
        let mut depth = 0usize;
        let mut i = start;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            match ch {
                '"' | '\'' => {
                    i += 1;
                    while i < bytes.len() {
                        let current = bytes[i] as char;
                        if current == '\\' {
                            i = (i + 2).min(bytes.len());
                            continue;
                        }
                        i += 1;
                        if current == ch {
                            break;
                        }
                    }
                }
                '<' => {
                    depth += 1;
                    i += 1;
                }
                '>' if i == 0 || bytes[i - 1] != b'=' => {
                    depth = depth.checked_sub(1)?;
                    i += 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        None
    }

    fn single_type_argument_text(type_text: &str, start: usize, end: usize) -> Option<&str> {
        let inner = type_text.get(start + 1..end.checked_sub(1)?)?.trim();
        if inner.is_empty() {
            return None;
        }
        let bytes = inner.as_bytes();
        let mut depth = 0usize;
        let mut i = 0;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            match ch {
                '"' | '\'' => {
                    i += 1;
                    while i < bytes.len() {
                        let current = bytes[i] as char;
                        if current == '\\' {
                            i = (i + 2).min(bytes.len());
                            continue;
                        }
                        i += 1;
                        if current == ch {
                            break;
                        }
                    }
                }
                '<' => {
                    depth += 1;
                    i += 1;
                }
                '>' if i == 0 || bytes[i - 1] != b'=' => {
                    depth = depth.checked_sub(1)?;
                    i += 1;
                }
                ',' if depth == 0 => return None,
                _ => i += 1,
            }
        }
        Some(inner)
    }

    pub(in crate::declaration_emitter) const fn is_type_reference_identifier_start(
        ch: char,
    ) -> bool {
        ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
    }

    pub(in crate::declaration_emitter) const fn is_type_reference_identifier_continue(
        ch: char,
    ) -> bool {
        ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
    }

    pub(crate) fn resolve_declaration_type_text(
        &self,
        related_nodes: &[NodeIndex],
        initializer: Option<NodeIndex>,
    ) -> Option<ResolvedDeclarationTypeText> {
        let type_id = self.get_node_type_or_names(related_nodes)?;
        let canonical_type_text = self.print_type_id_for_inferred_declaration(type_id);
        let emitted_type_text = initializer
            .map(|initializer| {
                self.declaration_emittable_type_text(initializer, type_id, &canonical_type_text)
            })
            .unwrap_or_else(|| canonical_type_text.clone());
        Some(ResolvedDeclarationTypeText {
            type_id,
            canonical_type_text,
            emitted_type_text,
        })
    }

    pub(crate) fn allowlisted_initializer_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        self.explicit_asserted_type_text(initializer)
            .or_else(|| self.preferred_expression_type_text(initializer))
            .or_else(|| self.infer_fallback_type_text(initializer))
    }

    pub(crate) fn print_type_id_with_outer_type_params(
        &self,
        type_id: tsz_solver::types::TypeId,
        outer_type_params: &NodeList,
    ) -> String {
        let elided_alias_names = self.function_local_type_alias_application_names(type_id);
        let Some(interner) = self.type_interner else {
            return "any".to_string();
        };
        let type_id = self.display_alias_for_declaration_emit(type_id, interner);
        let type_id = if self.should_preserve_named_application_for_emit(type_id, interner) {
            type_id
        } else if let Some(cache) = &self.type_cache {
            let resolver = DtsCacheResolver { cache };
            let mut evaluator = tsz_solver::TypeEvaluator::with_resolver(interner, &resolver);
            evaluator.set_max_mapped_keys(1_024);
            let evaluated = evaluator.evaluate(type_id);
            self.display_alias_for_declaration_emit(evaluated, interner)
        } else {
            let mut evaluator = tsz_solver::TypeEvaluator::new(interner);
            evaluator.set_max_mapped_keys(1_024);
            let evaluated = evaluator.evaluate(type_id);
            self.display_alias_for_declaration_emit(evaluated, interner)
        };
        let mut outer_names = Vec::new();
        for &param_idx in &outer_type_params.nodes {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_type_parameter(param_node)
                && let Some(name_text) = self.get_identifier_text(param.name)
            {
                let atom = interner.intern_string(&name_text);
                outer_names.push(atom);
            }
        }
        let module_path_resolver = |sym_id| self.resolve_symbol_module_path(sym_id);
        let namespace_alias_resolver = |sym_id| self.resolve_namespace_import_alias(sym_id);
        let local_import_alias_name_resolver =
            |sym_id| self.can_reference_local_import_alias_by_name(sym_id);
        let has_local_import_alias_resolver = |sym_id| {
            if let Some(binder) = self.binder {
                self.symbol_has_local_import_alias(binder, sym_id)
            } else {
                false
            }
        };
        let mut printer = TypePrinter::new(interner)
            .with_indent_level(self.indent_level)
            .with_node_arena(self.arena)
            .with_module_path_resolver(&module_path_resolver)
            .with_namespace_alias_resolver(&namespace_alias_resolver)
            .with_local_import_alias_name_resolver(&local_import_alias_name_resolver)
            .with_has_local_import_alias_resolver(&has_local_import_alias_resolver)
            .with_strict_null_checks(self.strict_null_checks)
            .with_outer_type_params(outer_names);
        if let Some(binder) = self.binder {
            printer = printer.with_symbols(&binder.symbols);
        }
        if let Some(cache) = &self.type_cache {
            printer = printer.with_type_cache(cache);
        }
        if let Some(enc_sym) = self.enclosing_namespace_symbol {
            printer = printer.with_enclosing_symbol(enc_sym);
        }
        let printed = printer.print_type(type_id);
        if elided_alias_names.is_empty() {
            printed
        } else {
            Self::elide_type_reference_names(&printed, &elided_alias_names)
        }
    }
    pub(crate) fn collect_type_param_names(&self, type_params: &NodeList) -> Vec<String> {
        let mut names = Vec::new();
        for &param_idx in &type_params.nodes {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_type_parameter(param_node)
                && let Some(name_text) = self.get_identifier_text(param.name)
            {
                names.push(name_text);
            }
        }
        names
    }
    pub(crate) fn rename_shadowed_type_params_in_text(
        text: &str,
        outer_names: &[String],
    ) -> String {
        if outer_names.is_empty() {
            return text.to_string();
        }
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut renames: Vec<(String, String)> = Vec::new();
        let mut i = 0;
        while i < len {
            if bytes[i] == b'<' {
                let mut depth = 1;
                let mut j = i + 1;
                let mut param_names: Vec<String> = Vec::new();
                let mut current_start = j;
                while j < len && depth > 0 {
                    match bytes[j] {
                        b'<' => depth += 1,
                        b'>' => {
                            depth -= 1;
                            if depth == 0
                                && let Some(name) =
                                    Self::extract_type_param_name(&text[current_start..j])
                            {
                                param_names.push(name);
                            }
                        }
                        b',' if depth == 1 => {
                            if let Some(name) =
                                Self::extract_type_param_name(&text[current_start..j])
                            {
                                param_names.push(name);
                            }
                            current_start = j + 1;
                        }
                        _ => {}
                    }
                    j += 1;
                }
                let is_func_type_params = j < len && bytes[j] == b'(';
                if is_func_type_params {
                    for name in &param_names {
                        let trimmed = name.trim();
                        if outer_names.iter().any(|o| o == trimmed)
                            && !renames.iter().any(|(o, _)| o == trimmed)
                        {
                            let mut s = 1u32;
                            loop {
                                let cand = format!("{trimmed}_{s}");
                                if !outer_names.contains(&cand)
                                    && !renames.iter().any(|(_, r)| *r == cand)
                                {
                                    renames.push((trimmed.to_string(), cand));
                                    break;
                                }
                                s += 1;
                            }
                        }
                    }
                }
                i = j;
            } else {
                i += 1;
            }
        }
        let mut result = text.to_string();
        for (original, renamed) in &renames {
            result = Self::replace_whole_word(&result, original, renamed);
        }
        result
    }

    fn extract_type_param_name(segment: &str) -> Option<String> {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            return None;
        }
        let trimmed = trimmed.strip_prefix("const ").unwrap_or(trimmed).trim();
        let trimmed = trimmed.strip_prefix("in ").unwrap_or(trimmed).trim();
        let trimmed = trimmed.strip_prefix("out ").unwrap_or(trimmed).trim();
        let name: String = trimmed
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() { None } else { Some(name) }
    }
    pub(in crate::declaration_emitter) fn replace_whole_word(
        text: &str,
        word: &str,
        replacement: &str,
    ) -> String {
        let mut result = String::with_capacity(text.len() + 16);
        let bytes = text.as_bytes();
        let word_bytes = word.as_bytes();
        let word_len = word_bytes.len();
        let text_len = bytes.len();
        let mut i = 0;
        while i < text_len {
            if i + word_len <= text_len && &bytes[i..i + word_len] == word_bytes {
                let before_ok = i == 0 || !Self::is_ident_char(bytes[i - 1]);
                let after_ok =
                    i + word_len >= text_len || !Self::is_ident_char(bytes[i + word_len]);
                if before_ok && after_ok {
                    result.push_str(replacement);
                    i += word_len;
                    continue;
                }
            }
            result.push(bytes[i] as char);
            i += 1;
        }
        result
    }
    pub(in crate::declaration_emitter) const fn is_ident_char(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
    }

    pub(in crate::declaration_emitter) fn print_synthetic_class_extends_alias_type(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> String {
        let Some(interner) = self.type_interner else {
            return self.print_type_id(type_id);
        };
        let Some(callable_id) = tsz_solver::visitor::callable_shape_id(interner, type_id) else {
            return self.print_type_id(type_id);
        };
        let callable = interner.callable_shape(callable_id);
        let has_properties = callable.properties.iter().any(|prop| {
            let name = interner.resolve_atom(prop.name);
            name != "prototype" && !name.starts_with("__private_brand_")
        });

        if callable.symbol.is_none()
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
            && callable.construct_signatures[0].type_predicate.is_none()
        {
            return self.print_construct_signature_arrow_text(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
        }

        self.print_type_id(type_id)
    }

    pub(in crate::declaration_emitter) fn print_construct_signature_arrow_text(
        &self,
        sig: &tsz_solver::types::CallSignature,
        is_abstract: bool,
    ) -> String {
        let Some(interner) = self.type_interner else {
            return self.print_type_id(sig.return_type);
        };

        let type_params = if sig.type_params.is_empty() {
            String::new()
        } else {
            let params = sig
                .type_params
                .iter()
                .map(|tp| {
                    let mut text = String::new();
                    if tp.is_const {
                        text.push_str("const ");
                    }
                    text.push_str(&interner.resolve_atom(tp.name));
                    if let Some(constraint) = tp.constraint {
                        text.push_str(" extends ");
                        text.push_str(&self.print_type_id(constraint));
                    }
                    if let Some(default) = tp.default {
                        text.push_str(" = ");
                        text.push_str(&self.print_type_id(default));
                    }
                    text
                })
                .collect::<Vec<_>>();
            format!("<{}>", params.join(", "))
        };

        let params = sig
            .params
            .iter()
            .map(|param| {
                let mut text = String::new();
                if param.rest {
                    text.push_str("...");
                }
                if let Some(name) = param.name {
                    text.push_str(&interner.resolve_atom(name));
                    if param.optional {
                        text.push('?');
                    }
                    text.push_str(": ");
                }
                text.push_str(&self.print_type_id(param.type_id));
                text
            })
            .collect::<Vec<_>>();

        let prefix = if is_abstract { "abstract new " } else { "new " };
        format!(
            "{prefix}{}({}) => {}",
            type_params,
            params.join(", "),
            self.print_type_id(sig.return_type)
        )
    }
}
