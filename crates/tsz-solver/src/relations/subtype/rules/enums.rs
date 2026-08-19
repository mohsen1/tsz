//! Enum relation rules (nominal identity plus structural member values).
//!
//! Enums are nominal types: two different enums with the same member values
//! are NOT compatible. `Enum(DefId, MemberType)` preserves both facets:
//! - `DefId`: nominal identity (`E1 != E2`)
//! - `MemberType`: structural assignability to primitives (`E1 <: number`)

use crate::def::resolver::TypeResolver;
use crate::relations::subtype::{SubtypeChecker, SubtypeResult};
use crate::types::TypeId;
use crate::visitor::enum_components;

impl<R: TypeResolver> SubtypeChecker<'_, R> {
    /// Apply the enum relation rules when either side is an `Enum` type.
    ///
    /// Returns `None` when neither side is an enum so the dispatcher falls
    /// through to the remaining rules.
    pub(crate) fn check_enum_relations(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeResult> {
        if let (Some((s_def_id, s_members)), Some((t_def_id, _t_members))) = (
            enum_components(self.interner, source),
            enum_components(self.interner, target),
        ) {
            // Cross-module import barrels can give the same enum (or member)
            // declaration two distinct `DefId`s (the declaring file's key and an
            // import-alias key reached via a re-export). They denote the same
            // nominal enum, so compare through `defs_are_equivalent` (which
            // canonicalizes alias-forwarding and falls back to `SymbolId`)
            // instead of raw `DefId` equality. Raw `==` here makes the narrowing
            // subtype check (`E.MEMBER <: E`) fail whenever the discriminant
            // property type and the literal member were reached through
            // different module paths, collapsing the receiver to `never` (the
            // mobx `IDerivationState_` cross-file enum cascade).
            let same_def = self.resolver.defs_are_equivalent(s_def_id, t_def_id);

            if same_def
                && source != target
                && crate::type_queries::is_literal_enum_member(self.interner, source)
                && crate::type_queries::is_literal_enum_member(self.interner, target)
            {
                return Some(SubtypeResult::False);
            }

            // Enum to Enum: Nominal check - definitions must match
            if same_def {
                return Some(SubtypeResult::True);
            }

            // Check for member-to-parent relationship (e.g., E.A -> E)
            // If source is a member of the target enum, it is a subtype
            if self
                .resolver
                .get_enum_parent_def_id(s_def_id)
                .is_some_and(|parent| self.resolver.defs_are_equivalent(parent, t_def_id))
            {
                // Source is a member of target enum
                // Only allow if target is the full enum type (not a different member)
                if self.resolver.is_enum_type(target, self.interner) {
                    return Some(SubtypeResult::True);
                }
            }

            // Whole-enum source vs a member of the SAME enum: tsc models an
            // enum type as the union of its member types, so the relation
            // reduces to the value domains — a single-member enum IS its
            // member type (`One` relates to `One.Only`), while a multi-member
            // enum's value union fails against any single member's value.
            // Nominality is already satisfied (same enum), so compare the
            // structural member values.
            if self
                .resolver
                .get_enum_parent_def_id(t_def_id)
                .is_some_and(|parent| self.resolver.defs_are_equivalent(parent, s_def_id))
            {
                return Some(self.check_subtype(s_members, target));
            }

            // Different enums are NOT compatible (nominal typing)
            return Some(SubtypeResult::False);
        }

        // Source is Enum, Target is not - check structural member type
        if let Some((_s_def_id, s_members)) = enum_components(self.interner, source) {
            return Some(self.check_subtype(s_members, target));
        }

        // Target is Enum, Source is not - check Rule #7 first, then structural member type
        if let Some((t_def_id, t_members)) = enum_components(self.interner, target) {
            // Rule #7: number is assignable to numeric enums
            if source == TypeId::NUMBER && self.resolver.is_numeric_enum(t_def_id) {
                return Some(SubtypeResult::True);
            }
            // For number literals, fall through to structural check against t_members
            // so that only actual enum member values (e.g., 0|1|2) are accepted
            return Some(self.check_subtype(source, t_members));
        }

        None
    }
}
