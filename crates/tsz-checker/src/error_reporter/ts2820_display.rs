//! Display helpers for TS2820 suggestion diagnostics.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn widen_numeric_member_literals_in_display_text(display: &str) -> String {
        let bytes = display.as_bytes();
        let mut out = String::with_capacity(display.len());
        let mut i = 0usize;
        let is_boundary = |b: u8| {
            matches!(
                b,
                b';' | b',' | b'}' | b'>' | b')' | b'|' | b'&' | b']' | b' '
            )
        };
        while i < bytes.len() {
            if i + 2 < bytes.len() && bytes[i] == b':' && bytes[i + 1] == b' ' {
                out.push(':');
                out.push(' ');
                i += 2;

                let mut j = i;
                if j < bytes.len() && bytes[j] == b'-' {
                    j += 1;
                }
                let mut saw_digit = false;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                    saw_digit = true;
                }
                if j < bytes.len() && bytes[j] == b'.' {
                    j += 1;
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                        saw_digit = true;
                    }
                }
                if saw_digit && (j >= bytes.len() || is_boundary(bytes[j])) {
                    out.push_str("number");
                    i = j;
                    continue;
                }
            }

            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    pub(super) fn ts2820_target_contains_alias_surface(&self, target: TypeId) -> bool {
        self.ts2820_any_in_members(target, &|s, t| {
            s.ctx.types.get_display_alias(t).is_some()
                || s.lookup_type_alias_name_for_display(t).is_some()
        })
    }

    pub(super) fn ts2820_target_contains_application_surface(&self, target: TypeId) -> bool {
        self.ts2820_any_in_members(target, &|s, t| {
            s.ts2820_is_named_application_surface(t)
                || s.ctx
                    .types
                    .get_display_alias(t)
                    .is_some_and(|alias| s.ts2820_is_named_application_surface(alias))
        })
    }

    fn ts2820_any_in_members(
        &self,
        target: TypeId,
        predicate: &dyn Fn(&Self, TypeId) -> bool,
    ) -> bool {
        if predicate(self, target) {
            return true;
        }
        crate::query_boundaries::diagnostics::union_members(self.ctx.types, target)
            .or_else(|| {
                crate::query_boundaries::diagnostics::intersection_members(self.ctx.types, target)
            })
            .is_some_and(|members| {
                members
                    .iter()
                    .any(|&member| self.ts2820_any_in_members(member, predicate))
            })
    }

    fn ts2820_is_named_application_surface(&self, target: TypeId) -> bool {
        let Some((base, args)) =
            crate::query_boundaries::diagnostics::application_info(self.ctx.types, target)
        else {
            return false;
        };
        !args.is_empty() && self.ts2820_application_base_has_named_surface(base)
    }

    fn ts2820_application_base_has_named_surface(&self, base: TypeId) -> bool {
        crate::query_boundaries::diagnostics::lazy_def_id(self.ctx.types, base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(base))
            .is_some()
            || self.ctx.types.get_display_alias(base).is_some()
            || self.lookup_type_alias_name_for_display(base).is_some()
    }
}
