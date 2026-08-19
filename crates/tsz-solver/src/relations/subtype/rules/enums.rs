//! Enum relation rules (nominal identity plus structural member values).
//!
//! Enums are nominal types: two different enums with the same member values
//! are NOT compatible. `Enum(DefId, MemberType)` preserves both facets:
//! - `DefId`: nominal identity (`E1 != E2`)
//! - `MemberType`: structural assignability to primitives (`E1 <: number`)

use crate::def::resolver::TypeResolver;
use crate::relations::subtype::{SubtypeChecker, SubtypeResult};
use crate::types::{LiteralValue, TypeData, TypeId};
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

        // Target is Enum, Source is not - Rule #7, tsc's numeric-member
        // admission, and the structural member fallthrough.
        if enum_components(self.interner, target).is_some() {
            return Some(self.check_non_enum_source_to_enum_target(source, target));
        }

        None
    }

    /// Relate a non-enum source against an `Enum(DefId, members)` target.
    ///
    /// tsc `isSimpleTypeRelatedTo`: `number` is assignable to any enum MEMBER
    /// whose value is numeric (`t & NumberLiteral && t & EnumLiteral`), and —
    /// through the union rule — to any enum type with at least one numeric
    /// member (`number -> H.A` admits `number -> H`). The registered
    /// numeric-enum fast path only covers homogeneous all-numeric enums, so
    /// member targets, heterogeneous enums, and computed-member enums must
    /// consult the member value domain itself. String-valued members stay
    /// rejected by the structural fallthrough (a raw string never converts to
    /// a string enum, not even with a matching value).
    ///
    /// The structural dispatcher's intrinsic-source arm delegates here too:
    /// its early `False` for non-object targets would otherwise reject
    /// `number` against every enum target before this rule could run.
    pub(crate) fn check_non_enum_source_to_enum_target(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> SubtypeResult {
        let Some((t_def_id, t_members)) = enum_components(self.interner, target) else {
            return SubtypeResult::False;
        };

        if source == TypeId::NUMBER
            && (self.resolver.is_numeric_enum(t_def_id)
                || self.enum_value_domain_admits_number(t_members))
        {
            return SubtypeResult::True;
        }

        // For number literals, fall through to structural check against
        // t_members so that only actual enum member values (e.g. 0|1|2) are
        // accepted.
        self.check_subtype(source, t_members)
    }

    /// Whether an enum's structural member value set contains a numeric
    /// constituent (`number` itself or a numeric literal).
    ///
    /// `members` is the value-domain side of an `Enum(DefId, members)`
    /// wrapper: a single literal for a member type, a union of bare literals
    /// for a declared enum type, or a union of `Enum`-wrapped member types
    /// for a reconstructed full-member union — so constituents unwrap one
    /// `Enum` layer before the numeric test. A computed member whose value
    /// could not be evaluated surfaces as `number` and also admits `number`
    /// (tsc gives such enums the `Enum` type flag, which admits `number`
    /// wholesale).
    fn enum_value_domain_admits_number(&self, members: TypeId) -> bool {
        let unwrap = |type_id: TypeId| -> TypeId {
            enum_components(self.interner, type_id).map_or(type_id, |(_, inner)| inner)
        };
        let is_number_like = |type_id: TypeId| -> bool {
            let unwrapped = unwrap(type_id);
            unwrapped == TypeId::NUMBER
                || matches!(
                    self.interner.lookup(unwrapped),
                    Some(TypeData::Literal(LiteralValue::Number(_)))
                )
        };

        match self.interner.lookup(unwrap(members)) {
            Some(TypeData::Union(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .any(|&member| is_number_like(member)),
            _ => is_number_like(members),
        }
    }
}
