//! Static-side class diagnostics for namespace-merged declarations.

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn report_namespace_merged_static_side_mismatch(
        &mut self,
        class_idx: NodeIndex,
        class_name: NodeIndex,
        derived_class_name: &str,
        base_class_name: &str,
        base_sym: tsz_binder::SymbolId,
    ) {
        let Some(derived_sym) = self.ctx.binder.get_node_symbol(class_idx) else {
            return;
        };
        let derived_symbol_flags = self
            .ctx
            .binder
            .get_symbol(derived_sym)
            .map_or(0, |s| s.flags);
        let derived_has_namespace = derived_symbol_flags
            & (tsz_binder::symbol_flags::NAMESPACE_MODULE | tsz_binder::symbol_flags::VALUE_MODULE)
            != 0;
        if !derived_has_namespace {
            return;
        }

        let derived_ctor_type = self.get_type_of_symbol(derived_sym);
        let base_ctor_type = self.get_type_of_symbol(base_sym);
        let derived_ns_names = self.collect_namespace_export_names_for_symbol(derived_sym);
        let base_static_names = self.collect_property_names_from_type(base_ctor_type);
        let has_name_overlap = derived_ns_names
            .iter()
            .any(|n| base_static_names.contains(n));
        if has_name_overlap
            && derived_ctor_type != TypeId::UNKNOWN
            && derived_ctor_type != TypeId::ERROR
            && base_ctor_type != TypeId::UNKNOWN
            && base_ctor_type != TypeId::ERROR
            && !self.diagnostic_relation_boolean_guard(derived_ctor_type, base_ctor_type)
        {
            self.error_at_node(
                class_name,
                &format!(
                    "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side 'typeof {base_class_name}'."
                ),
                diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
            );
        }
    }

    /// Collect the names of a symbol's namespace exports (merged `namespace X { ... }`
    /// declarations). Used to decide whether a class-namespace merge could
    /// shadow or conflict with a base class's static members for the TS2417
    /// static-side compatibility check.
    fn collect_namespace_export_names_for_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && let Some(exports) = symbol.exports.as_ref()
        {
            for (name, _sym_id) in exports.iter() {
                if !name.is_empty() {
                    names.insert(name.clone());
                }
            }
        }
        names
    }

    /// Collect all property names from a type via the solver query boundary API.
    /// Used for type-level override checking when the base class is a complex
    /// expression (function call, intersection constructor).
    pub(super) fn collect_property_names_from_type(
        &mut self,
        type_id: TypeId,
    ) -> rustc_hash::FxHashSet<String> {
        let resolved = self.resolve_lazy_type(type_id);
        let atoms =
            crate::query_boundaries::diagnostics::collect_property_name_atoms_for_diagnostics(
                self.ctx.types,
                resolved,
                5,
            );
        atoms
            .into_iter()
            .map(|atom| self.ctx.types.resolve_atom_ref(atom).to_string())
            .collect()
    }
}
