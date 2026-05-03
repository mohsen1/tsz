use rustc_hash::FxHashMap;
use std::sync::Arc;
use tracing::debug;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

use super::{SolverTypeId, UsageAnalyzer, UsageKind, visitor};

impl<'a> UsageAnalyzer<'a> {
    /// Walk an inferred type from the type cache.
    ///
    /// This is the semantic walk over inferred `TypeId`s.
    pub(super) fn walk_inferred_type(&mut self, node_idx: NodeIndex) {
        // Look up the inferred TypeId for this node
        debug!("[DEBUG] walk_inferred_type: node_idx={:?}", node_idx);
        if let Some(&type_id) = self.type_cache.node_types.get(&node_idx.0) {
            debug!("[DEBUG] walk_inferred_type: found type_id={:?}", type_id);
            self.walk_type_id(type_id);
        } else {
            debug!(
                "[DEBUG] walk_inferred_type: NO TYPE FOUND for node_idx={:?}",
                node_idx
            );
        }
    }

    fn walk_inferred_type_if_present(&mut self, node_idx: NodeIndex) -> bool {
        if let Some(&type_id) = self.type_cache.node_types.get(&node_idx.0) {
            self.walk_type_id(type_id);
            return true;
        }
        false
    }

    pub(super) fn walk_inferred_type_or_related(&mut self, node_ids: &[NodeIndex]) {
        for &node_idx in node_ids {
            if !node_idx.is_some() {
                continue;
            }

            if self.walk_inferred_type_if_present(node_idx) {
                return;
            }

            let Some(node) = self.arena.get(node_idx) else {
                continue;
            };

            for related_idx in self.get_node_type_related_nodes(node) {
                if related_idx.is_some() && self.walk_inferred_type_if_present(related_idx) {
                    return;
                }
            }
        }
    }

    pub(super) fn get_node_type_related_nodes(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Vec<NodeIndex> {
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    let mut related = Vec::with_capacity(2);
                    if decl.initializer.is_some() {
                        related.push(decl.initializer);
                    }
                    if decl.type_annotation.is_some() {
                        related.push(decl.type_annotation);
                    }
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
                    if decl.type_annotation.is_some() {
                        related.push(decl.type_annotation);
                    }
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
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(query) = self.arena.get_type_query(node) {
                    vec![query.expr_name]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn add_symbol_usage(
        usages: &mut FxHashMap<SymbolId, UsageKind>,
        sym_id: SymbolId,
        usage_kind: UsageKind,
    ) {
        usages
            .entry(sym_id)
            .and_modify(|kind| *kind |= usage_kind)
            .or_insert(usage_kind);
    }

    fn collect_direct_symbol_usages(
        &self,
        type_id: SolverTypeId,
        usages: &mut FxHashMap<SymbolId, UsageKind>,
    ) {
        if let Some(def_id) = visitor::lazy_def_id(self.type_interner, type_id)
            && let Some(&sym_id) = self.type_cache.def_to_symbol.get(&def_id)
        {
            Self::add_symbol_usage(usages, sym_id, UsageKind::TYPE);
        }

        if let Some((def_id, _)) = visitor::enum_components(self.type_interner, type_id)
            && let Some(&sym_id) = self.type_cache.def_to_symbol.get(&def_id)
        {
            Self::add_symbol_usage(usages, sym_id, UsageKind::TYPE);

            // Also add the parent enum symbol if this is an enum member
            if let Some(symbol) = self.binder.symbols.get(sym_id)
                && symbol.parent.is_some()
            {
                Self::add_symbol_usage(usages, symbol.parent, UsageKind::TYPE);
            }
        }

        if let Some(sym_ref) = visitor::type_query_symbol(self.type_interner, type_id)
            && let Some(sym_id) = self.type_query_value_dependency_symbol(SymbolId(sym_ref.0))
        {
            Self::add_symbol_usage(usages, sym_id, UsageKind::VALUE);
        }

        if let Some(sym_ref) = visitor::unique_symbol_ref(self.type_interner, type_id) {
            Self::add_symbol_usage(usages, SymbolId(sym_ref.0), UsageKind::TYPE);
        }

        if let Some(sym_ref) = visitor::module_namespace_symbol_ref(self.type_interner, type_id) {
            Self::add_symbol_usage(usages, SymbolId(sym_ref.0), UsageKind::TYPE);
        }

        if let Some(shape_id) = visitor::object_shape_id(self.type_interner, type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.type_interner, type_id))
        {
            let shape = self.type_interner.object_shape(shape_id);
            if let Some(sym_id) = shape.symbol {
                Self::add_symbol_usage(usages, sym_id, UsageKind::TYPE);
            }
        }

        if let Some(shape_id) = visitor::callable_shape_id(self.type_interner, type_id) {
            let shape = self.type_interner.callable_shape(shape_id);
            if let Some(sym_id) = shape.symbol {
                Self::add_symbol_usage(usages, sym_id, UsageKind::TYPE);
            }
        }
    }

    fn collect_symbol_usages_for_type(
        &mut self,
        type_id: SolverTypeId,
    ) -> Arc<[(SymbolId, UsageKind)]> {
        if let Some(cached) = self.type_symbol_cache.get(&type_id) {
            return cached.clone();
        }

        if !self.memoizing_types.insert(type_id) {
            return self
                .type_symbol_cache
                .get(&type_id)
                .cloned()
                .unwrap_or_else(|| Arc::from([]));
        }

        let mut usages = FxHashMap::default();
        self.collect_direct_symbol_usages(type_id, &mut usages);

        let mut result = Self::freeze_symbol_usages(&usages);
        self.type_symbol_cache.insert(type_id, result.clone());

        let mut children = Vec::new();
        visitor::for_each_child_by_id(self.type_interner, type_id, |child| {
            children.push(child);
        });

        for child in children {
            if child == type_id {
                continue;
            }
            for &(sym_id, usage_kind) in self.collect_symbol_usages_for_type(child).iter() {
                Self::add_symbol_usage(&mut usages, sym_id, usage_kind);
            }
        }

        self.memoizing_types.remove(&type_id);

        result = Self::freeze_symbol_usages(&usages);
        self.type_symbol_cache.insert(type_id, result.clone());
        result
    }

    fn freeze_symbol_usages(
        usages: &FxHashMap<SymbolId, UsageKind>,
    ) -> Arc<[(SymbolId, UsageKind)]> {
        let mut frozen: Vec<(SymbolId, UsageKind)> = usages
            .iter()
            .map(|(&sym_id, &usage_kind)| (sym_id, usage_kind))
            .collect();
        frozen.sort_unstable_by_key(|(sym_id, usage_kind)| (sym_id.0, usage_kind.bits));
        Arc::from(frozen)
    }

    /// Walk a `TypeId` to extract all referenced symbols.
    pub(super) fn walk_type_id(&mut self, type_id: SolverTypeId) {
        if !self.visited_types.insert(type_id) {
            return;
        }

        for &(sym_id, usage_kind) in self.collect_symbol_usages_for_type(type_id).iter() {
            self.mark_symbol_used(sym_id, usage_kind);
        }
    }
}
