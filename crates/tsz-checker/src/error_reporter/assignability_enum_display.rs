//! Enum-specific display helpers for assignability diagnostics.

use crate::query_boundaries::enum_analysis as enum_query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn format_union_with_collapsed_enum_display(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let members = crate::query_boundaries::diagnostics::union_members(self.ctx.types, ty)?;
        if members.len() < 2 {
            return None;
        }
        // Collapse a union of same-enum members to the bare enum name only
        // when the union covers EVERY member of the enum. tsc renders a
        // proper subset (e.g. `E.A | E.B` of a three-member enum) member by
        // member, falling through to the per-member rendering loop below.
        if let Some(enum_sym) =
            enum_query::full_enum_member_union_parent_symbol(&self.ctx, &members)
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
            .any(|&member| enum_query::enum_member_like_parent_symbol(&self.ctx, member).is_none());

        for &member in &members {
            if has_non_enum_member
                && let Some(enum_sym) =
                    enum_query::enum_member_like_parent_symbol(&self.ctx, member)
                && enum_query::union_contains_all_members_of_enum(&self.ctx, &members, enum_sym)
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
}
