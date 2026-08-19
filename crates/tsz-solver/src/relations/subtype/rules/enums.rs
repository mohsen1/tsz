//! Enum-target subtype rules.
//!
//! Handles the `Target is Enum, Source is not an Enum` arm of the structural
//! dispatcher: tsc Rule #7 (open numeric enums), the `isSimpleTypeRelatedTo`
//! numeric-member admission, and the structural member-value fallthrough.

use crate::types::{LiteralValue, TypeData, TypeId};
use crate::visitor::enum_components;

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
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
