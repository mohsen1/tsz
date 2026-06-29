//! Numeric-literal-union source-display helpers.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn source_type_contains_number_literal_only_union(
        &self,
        ty: TypeId,
    ) -> bool {
        let mut stack = vec![ty];
        let mut seen = FxHashSet::default();

        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }

            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, current)
            {
                if self.union_members_are_number_literals_or_common_intersections(&members) {
                    return true;
                }
                stack.extend(members);
                continue;
            }

            if let Some(members) =
                crate::query_boundaries::common::intersection_members(self.ctx.types, current)
            {
                stack.extend(members);
            }
        }

        false
    }

    fn union_members_are_number_literals_or_common_intersections(
        &self,
        members: &[TypeId],
    ) -> bool {
        if members.len() < 2 {
            return false;
        }

        let mut expected_non_numeric_parts: Option<FxHashSet<TypeId>> = None;
        for &member in members {
            let Some(non_numeric_parts) =
                self.number_literal_union_member_non_numeric_intersection_parts(member)
            else {
                return false;
            };

            if let Some(expected) = &expected_non_numeric_parts {
                if *expected != non_numeric_parts {
                    return false;
                }
            } else {
                expected_non_numeric_parts = Some(non_numeric_parts);
            }
        }

        true
    }

    fn number_literal_union_member_non_numeric_intersection_parts(
        &self,
        member: TypeId,
    ) -> Option<FxHashSet<TypeId>> {
        if matches!(
            crate::query_boundaries::common::literal_value(self.ctx.types, member),
            Some(crate::query_boundaries::common::LiteralValue::Number(_))
        ) {
            return Some(FxHashSet::default());
        }

        let intersection_members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, member)?;
        let mut saw_number_literal = false;
        let mut non_numeric_parts = FxHashSet::default();
        for part in intersection_members {
            if matches!(
                crate::query_boundaries::common::literal_value(self.ctx.types, part),
                Some(crate::query_boundaries::common::LiteralValue::Number(_))
            ) {
                if saw_number_literal {
                    return None;
                }
                saw_number_literal = true;
            } else {
                non_numeric_parts.insert(part);
            }
        }

        saw_number_literal.then_some(non_numeric_parts)
    }
}
