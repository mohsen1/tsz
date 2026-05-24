//! Enum-specific display helpers for assignability diagnostics.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn format_union_with_collapsed_enum_display(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, ty)?;
        if members.len() < 2 {
            return None;
        }
        let enum_member_symbols: Vec<_> = members
            .iter()
            .filter_map(|&member| self.enum_member_symbol_for_type(member))
            .collect();
        // Collapse a union of same-enum members to the bare enum name only
        // when the union covers EVERY member of the enum. tsc renders a
        // proper subset (e.g. `E.A | E.B` of a three-member enum) member by
        // member, falling through to the per-member rendering loop below.
        if enum_member_symbols.len() == members.len()
            && let Some((_, enum_sym)) = enum_member_symbols.first().copied()
            && enum_member_symbols
                .iter()
                .all(|(_, candidate)| *candidate == enum_sym)
            && self.union_contains_all_enum_members(&members, enum_sym)
        {
            let widened = self.widen_enum_member_type(members[0]);
            return self
                .format_qualified_enum_name_for_message(widened)
                .or_else(|| {
                    self.ctx
                        .binder
                        .get_symbol(enum_sym)
                        .map(|symbol| symbol.escaped_name.clone())
                });
        }
        let mut rendered = Vec::with_capacity(members.len());
        let mut collapsed_enum = None;
        let mut rendered_enum_member = false;
        let mut rendered_full_enums = Vec::new();
        let has_non_enum_member = members
            .iter()
            .any(|&member| self.enum_member_symbol_for_type(member).is_none());

        for &member in &members {
            if has_non_enum_member
                && let Some((_, enum_sym)) = self.enum_member_symbol_for_type(member)
                && self.union_contains_all_enum_members(&members, enum_sym)
            {
                if !rendered_full_enums.contains(&enum_sym) {
                    let widened = self.widen_enum_member_type(member);
                    rendered.push(
                        self.format_qualified_enum_name_for_message(widened)
                            .or_else(|| {
                                self.ctx
                                    .binder
                                    .get_symbol(enum_sym)
                                    .map(|symbol| symbol.escaped_name.clone())
                            })?,
                    );
                    rendered_full_enums.push(enum_sym);
                }
                continue;
            }
            if let Some(name) = self.format_enum_member_name_for_message(member) {
                rendered.push(name);
                rendered_enum_member = true;
                continue;
            }
            let widened = self.widen_enum_member_type(member);
            if let Some(enum_sym) = self.enum_symbol_from_enumish_type(widened)
                && let Some(symbol) = self.ctx.binder.get_symbol(enum_sym)
            {
                let name = symbol.escaped_name.clone();
                match collapsed_enum.as_ref() {
                    Some((existing_sym, _)) if *existing_sym == enum_sym => {}
                    None => {
                        collapsed_enum = Some((enum_sym, name.clone()));
                        rendered.push(name);
                    }
                    Some(_) => return None,
                }
            } else {
                rendered.push(self.format_type_for_assignability_message(member));
            }
        }

        if collapsed_enum.is_some() || rendered_enum_member {
            Some(rendered.join(" | "))
        } else {
            None
        }
    }

    fn enum_member_symbol_for_type(
        &mut self,
        ty: TypeId,
    ) -> Option<(tsz_binder::SymbolId, tsz_binder::SymbolId)> {
        let def_id = crate::query_boundaries::common::enum_def_id(self.ctx.types, ty)?;
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        symbol
            .has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
            .then_some((sym_id, symbol.parent))
    }

    fn union_contains_all_enum_members(
        &mut self,
        members: &[TypeId],
        enum_sym: tsz_binder::SymbolId,
    ) -> bool {
        let Some(enum_symbol) = self.ctx.binder.get_symbol(enum_sym) else {
            return false;
        };
        let Some(exports) = enum_symbol.exports.as_ref() else {
            return false;
        };
        let enum_member_count = exports
            .iter()
            .filter(|&(_, &sym_id)| {
                self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                    symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
                })
            })
            .count();
        enum_member_count > 0
            && members
                .iter()
                .filter_map(|&member| self.enum_member_symbol_for_type(member))
                .filter(|(_, parent)| *parent == enum_sym)
                .count()
                == enum_member_count
    }
}
