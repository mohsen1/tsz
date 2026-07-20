//! Declared-type lookups for assignment targets.
//!
//! Split from `assignment_ops.rs` (arch LOC cap).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn assignment_target_declared_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let value_decl = symbol.value_declaration;
        if !value_decl.is_some() {
            return None;
        }

        let node = self.ctx.arena.get(value_decl)?;
        if let Some(param) = self.ctx.arena.get_parameter(node)
            && param.type_annotation.is_some()
        {
            return Some(self.get_type_from_type_node(param.type_annotation));
        }

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
            && var_decl.type_annotation.is_some()
        {
            return Some(self.get_type_from_type_node(var_decl.type_annotation));
        }

        None
    }

    pub(super) fn assignment_identifier_declared_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.ctx.binder.resolve_identifier(self.ctx.arena, idx)?;
        self.assignment_target_declared_type(sym_id)
    }

    pub(super) fn recursive_tuple_declared_assignment_types(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> Option<(TypeId, TypeId)> {
        let target_declared = self.assignment_identifier_declared_type(left_idx)?;
        let source_declared = self.assignment_identifier_declared_type(right_idx)?;

        let (target_base, target_args) = self.application_info_or_display_alias(target_declared)?;
        let (source_base, source_args) = self.application_info_or_display_alias(source_declared)?;
        if target_base != source_base || target_args.len() != source_args.len() {
            return None;
        }

        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, target_base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        let name = self.ctx.types.resolve_atom_ref(def.name);
        if def.kind != tsz_solver::def::DefKind::TypeAlias || name.as_ref() != "TupleOf" {
            return None;
        }

        let has_type_parameter_arg = target_args.iter().chain(source_args.iter()).any(|arg| {
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, *arg)
        });
        if !has_type_parameter_arg || target_args == source_args {
            return None;
        }

        Some((source_declared, target_declared))
    }

    pub(super) fn declared_same_application_assignment_types(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> Option<(TypeId, TypeId)> {
        let target_declared = self.assignment_identifier_declared_type(left_idx)?;
        let source_declared = self.assignment_identifier_declared_type(right_idx)?;

        let (target_base, target_args) = self.application_info_or_display_alias(target_declared)?;
        let (source_base, source_args) = self.application_info_or_display_alias(source_declared)?;
        if target_base != source_base || target_args.len() != source_args.len() {
            return None;
        }

        Some((source_declared, target_declared))
    }

    pub(super) fn declared_application_any_target_accepts(
        &self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        let Some((source_base, source_args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, source_type)
        else {
            return false;
        };
        let Some((target_base, target_args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, target_type)
        else {
            return false;
        };
        if source_args
            .iter()
            .zip(target_args.iter())
            .any(|(&source_arg, &target_arg)| {
                (source_arg.is_any() && target_arg == TypeId::NEVER)
                    || (source_arg == TypeId::NEVER && target_arg.is_any())
            })
        {
            // Directional any/never application compatibility is owned by the
            // option-aware solver classifier, not this blanket target-any
            // assignment shortcut.
            return false;
        }
        source_base == target_base
            && source_args.len() == target_args.len()
            && target_args.iter().any(|arg| arg.is_any())
            && source_args
                .iter()
                .zip(target_args.iter())
                .all(|(source_arg, target_arg)| target_arg.is_any() || source_arg == target_arg)
    }
}
