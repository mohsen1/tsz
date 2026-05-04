use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn enclosing_interface_type_param_names(&self, idx: NodeIndex) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let mut current = idx;
        while !current.is_none() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                if let Some(interface) = self.ctx.arena.get_interface(node)
                    && let Some(type_params) = &interface.type_parameters
                {
                    for &param_idx in &type_params.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                            && let Some(name_node) = self.ctx.arena.get(param.name)
                            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        {
                            names.insert(ident.escaped_text.clone());
                        }
                    }
                }
                break;
            }
        }
        names
    }

    pub(super) fn type_args_reference_type_params(
        &self,
        args: &tsz_parser::parser::NodeList,
        type_param_names: &FxHashSet<String>,
    ) -> bool {
        if type_param_names.is_empty() {
            return false;
        }
        let mut stack: Vec<NodeIndex> = args.nodes.to_vec();
        while let Some(idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                && let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && type_param_names.contains(ident.escaped_text.as_str())
            {
                return true;
            }
            stack.extend(self.ctx.arena.get_children(idx));
        }
        false
    }

    pub(super) fn symbol_is_uninstantiated_namespace(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_binder::SymbolId> {
        use tsz_binder::symbol_flags;

        let mut visited_aliases = AliasCycleTracker::new();
        let sym_to_check = self
            .resolve_alias_symbol(sym_id, &mut visited_aliases)
            .unwrap_or(sym_id);
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(sym_to_check, &lib_binders)?;

        let is_namespace = symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE);
        let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
        let has_other_value = symbol.has_any_flags(value_flags_except_module);
        if !is_namespace || has_other_value {
            return None;
        }

        let is_instantiated = symbol
            .declarations
            .iter()
            .any(|&decl_idx| self.is_namespace_declaration_instantiated(decl_idx));
        if is_instantiated {
            None
        } else {
            Some(sym_to_check)
        }
    }

    pub(super) fn skip_ts2314_for_heritage_symbol(
        &self,
        heritage_sym: tsz_binder::SymbolId,
        is_class_declaration: bool,
        is_extends_clause: bool,
    ) -> bool {
        use tsz_binder::symbol_flags;

        self.is_js_file()
            || (is_class_declaration
                && is_extends_clause
                && self.get_cross_file_symbol(heritage_sym).is_some_and(|s| {
                    s.has_any_flags(symbol_flags::VARIABLE)
                        || (s.has_any_flags(symbol_flags::INTERFACE)
                            && !s.has_any_flags(symbol_flags::CLASS))
                }))
    }

    pub(super) fn heritage_ts2314_display_name(
        &self,
        heritage_sym: tsz_binder::SymbolId,
        fallback: &str,
    ) -> String {
        let mut visited_aliases = AliasCycleTracker::new();
        self.resolve_alias_symbol(heritage_sym, &mut visited_aliases)
            .and_then(|target| {
                self.get_symbol_globally(target)
                    .map(|s| s.escaped_name.clone())
            })
            .unwrap_or_else(|| fallback.to_string())
    }

    pub(super) fn report_unresolved_qualified_heritage_member(
        &mut self,
        expr_idx: NodeIndex,
        is_class_declaration: bool,
        is_extends_clause: bool,
    ) -> bool {
        use crate::query_boundaries::name_resolution::{NameLookupKind, NameResolutionRequest};

        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        let (left_idx, right_idx) = if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                return false;
            };
            (access.expression, access.name_or_argument)
        } else {
            return false;
        };

        let Some(right_name) = self
            .ctx
            .arena
            .get_identifier_at(right_idx)
            .map(|ident| ident.escaped_text.clone())
        else {
            return false;
        };

        let Some(left_sym) = self.resolve_heritage_symbol(left_idx) else {
            return false;
        };
        let Some(namespace_sym) = self.symbol_is_uninstantiated_namespace(left_sym) else {
            return false;
        };

        if is_class_declaration && is_extends_clause {
            let ns_name = self
                .entity_name_text(left_idx)
                .unwrap_or_else(|| right_name.clone());
            self.report_wrong_meaning_diagnostic(&ns_name, left_idx, NameLookupKind::Namespace);
            return true;
        }

        if is_extends_clause && !is_class_declaration {
            let lib_binders = self.get_lib_binders();
            let Some(left_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(namespace_sym, &lib_binders)
            else {
                return false;
            };
            let export_names: Vec<String> = left_symbol
                .exports
                .as_ref()
                .map(|exports| exports.iter().map(|(name, _)| name.clone()).collect())
                .unwrap_or_default();
            let req = NameResolutionRequest::exported_member(
                &right_name,
                right_idx,
                namespace_sym,
                export_names,
            );
            if let Err(failure) = self.resolve_name_structured(&req) {
                self.report_name_resolution_failure(&req, &failure);
                return true;
            }
        }

        false
    }
}
