//! Enum and enum-adjacent checker queries.
//!
//! These wrappers keep enum utility code off the broad `common` quarantine
//! barrel while the underlying solver queries remain the semantic owner.

use std::sync::Arc;

use crate::context::CheckerContext;
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::DefId;
use tsz_solver::{ObjectShape, TypeId};

pub(crate) fn enum_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    super::common::enum_def_id(db, type_id)
}

/// Whether `type_id` names a TypeScript enum symbol. This is a checker boundary
/// rather than a pure solver query because unresolved `Lazy(DefId)` aliases may
/// still require binder-backed symbol resolution.
pub(crate) fn is_enum_type(ctx: &CheckerContext<'_>, type_id: TypeId) -> bool {
    enum_symbol_with_flags(ctx, type_id, symbol_flags::ENUM).is_some()
}

/// Check if a type is an enum-family fallback operand for arithmetic
/// validation: either a direct enum type/member, or a union whose every member
/// resolves to an enum or enum member.
///
/// Callers use this only at the same fallback points where the binary operator
/// evaluator could not fully resolve the operand. Resolved operands still go
/// through the evaluator, which owns numeric-vs-string enum validation.
pub(crate) fn is_arithmetic_enum_like_type(ctx: &CheckerContext<'_>, type_id: TypeId) -> bool {
    if is_enum_type(ctx, type_id) {
        return true;
    }

    let Some(members) = super::common::union_list_id(ctx.types, type_id) else {
        return is_enum_or_enum_member_type(ctx, type_id);
    };

    let member_list = ctx.types.type_list(members);
    !member_list.is_empty()
        && member_list
            .iter()
            .all(|&member| is_enum_or_enum_member_type(ctx, member))
}

/// Check if the type is still an unresolved `Lazy(DefId)`.
pub(crate) fn is_unresolved_lazy_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    super::common::is_lazy_type(db, type_id)
}

/// DefId for the parent enum when `type_id` is an enum member type.
pub(crate) fn enum_member_parent_def_id(
    ctx: &CheckerContext<'_>,
    type_id: TypeId,
) -> Option<DefId> {
    let def_id = enum_def_id(ctx.types, type_id)?;
    ctx.type_env
        .try_borrow()
        .ok()
        .and_then(|env| env.get_enum_parent(def_id))
}

/// Binder symbol for the parent enum when a type should widen from an enum
/// member to its containing enum. Falls back to symbol flags for legacy lazy
/// member aliases that have not been materialized into enum `DefId` metadata.
pub(crate) fn enum_member_parent_symbol_for_widening(
    ctx: &CheckerContext<'_>,
    type_id: TypeId,
) -> Option<SymbolId> {
    if let Some(parent_def_id) = enum_member_parent_def_id(ctx, type_id)
        && let Some(parent_sym_id) = ctx.def_to_symbol_id(parent_def_id)
    {
        return Some(parent_sym_id);
    }

    let sym_id = enum_symbol_with_flags(ctx, type_id, symbol_flags::ENUM_MEMBER)?;
    ctx.binder.get_symbol(sym_id).map(|symbol| symbol.parent)
}

/// Whether `type_id` is an enum member that widens to its parent enum in mutable
/// binding and fresh-return positions.
pub(crate) fn is_enum_member_for_widening(ctx: &CheckerContext<'_>, type_id: TypeId) -> bool {
    enum_member_parent_symbol_for_widening(ctx, type_id).is_some()
}

/// Parent enum symbol when `type_id` is an enum member or an indexed access
/// that names a declared enum member (for example `(typeof E)["A"]`).
pub(crate) fn enum_member_like_parent_symbol(
    ctx: &CheckerContext<'_>,
    type_id: TypeId,
) -> Option<SymbolId> {
    enum_member_fact(ctx, type_id).map(|fact| fact.parent_symbol)
}

/// Whether `members` contains exactly every declared member of `enum_sym`,
/// allowing other non-enum members in the surrounding union.
pub(crate) fn union_contains_all_members_of_enum(
    ctx: &CheckerContext<'_>,
    members: &[TypeId],
    enum_sym: SymbolId,
) -> bool {
    let Some(expected_members) = enum_export_member_symbols(ctx, enum_sym) else {
        return false;
    };
    if expected_members.is_empty() {
        return false;
    }

    let actual_members = members
        .iter()
        .filter_map(|&member| enum_member_fact(ctx, member))
        .filter(|fact| fact.parent_symbol == enum_sym)
        .map(|fact| fact.member_symbol)
        .collect::<FxHashSet<_>>();
    actual_members == expected_members
}

/// Parent enum symbol when every member of `members` belongs to the same enum
/// and the union covers all declared members of that enum.
pub(crate) fn full_enum_member_union_parent_symbol(
    ctx: &CheckerContext<'_>,
    members: &[TypeId],
) -> Option<SymbolId> {
    if members.is_empty() {
        return None;
    }

    let mut parent = None;
    for &member in members {
        let member_parent = enum_member_like_parent_symbol(ctx, member)?;
        if let Some(existing_parent) = parent {
            if existing_parent != member_parent {
                return None;
            }
        } else {
            parent = Some(member_parent);
        }
    }

    let parent = parent?;
    union_contains_all_members_of_enum(ctx, members, parent).then_some(parent)
}

/// Parent enum type when `type_id` is a union of every member of one enum.
pub(crate) fn full_enum_member_union_parent_type(
    ctx: &CheckerContext<'_>,
    type_id: TypeId,
) -> Option<TypeId> {
    let list_id = super::common::union_list_id(ctx.types, type_id)?;
    let members = ctx.types.type_list(list_id);
    let parent = full_enum_member_union_parent_symbol(ctx, members.as_ref())?;

    if let Some(parent_type) = ctx.symbol_types.get(&parent) {
        return Some(parent_type);
    }

    ctx.definition_store
        .find_def_by_symbol(parent.0)
        .map(|parent_def_id| ctx.types.factory().enum_type(parent_def_id, type_id))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EnumMemberFact {
    pub(crate) member_symbol: SymbolId,
    pub(crate) parent_symbol: SymbolId,
}

pub(crate) fn enum_member_fact(
    ctx: &CheckerContext<'_>,
    type_id: TypeId,
) -> Option<EnumMemberFact> {
    if let Some((def_id, _)) = super::common::enum_components(ctx.types, type_id) {
        let member_symbol = ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = ctx.binder.get_symbol(member_symbol)?;
        return symbol
            .has_any_flags(symbol_flags::ENUM_MEMBER)
            .then_some(symbol.parent)
            .filter(|parent_symbol| parent_symbol.is_some())
            .map(|parent_symbol| EnumMemberFact {
                member_symbol,
                parent_symbol,
            });
    }

    let (object_type, index_type) = super::common::index_access_parts(ctx.types, type_id)?;
    let parent_symbol = super::common::type_shape_symbol(ctx.types, object_type).or_else(|| {
        super::common::enum_components(ctx.types, object_type)
            .and_then(|(def_id, _)| ctx.def_to_symbol_id_with_fallback(def_id))
    })?;
    let parent = ctx.binder.get_symbol(parent_symbol)?;
    if !parent.has_any_flags(symbol_flags::ENUM) {
        return None;
    }
    let member_name =
        super::type_computation::access::literal_property_name(ctx.types, index_type)?;
    let member_name_text = ctx.types.resolve_atom(member_name);
    let member_symbol = parent.exports.as_ref()?.get(member_name_text.as_ref())?;
    let symbol = ctx.binder.get_symbol(member_symbol)?;
    (symbol.has_any_flags(symbol_flags::ENUM_MEMBER) && symbol.parent == parent_symbol).then_some(
        EnumMemberFact {
            member_symbol,
            parent_symbol,
        },
    )
}

fn is_enum_or_enum_member_type(ctx: &CheckerContext<'_>, type_id: TypeId) -> bool {
    enum_symbol_with_flags(ctx, type_id, symbol_flags::ENUM | symbol_flags::ENUM_MEMBER).is_some()
}

fn enum_export_member_symbols(
    ctx: &CheckerContext<'_>,
    enum_sym: SymbolId,
) -> Option<FxHashSet<SymbolId>> {
    let enum_symbol = ctx.binder.get_symbol(enum_sym)?;
    let exports = enum_symbol.exports.as_ref()?;
    Some(
        exports
            .iter()
            .filter_map(|(_, sym_id)| {
                ctx.binder
                    .get_symbol(*sym_id)
                    .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::ENUM_MEMBER))
                    .then_some(*sym_id)
            })
            .collect(),
    )
}

fn enum_symbol_with_flags(
    ctx: &CheckerContext<'_>,
    type_id: TypeId,
    flags: u32,
) -> Option<SymbolId> {
    let sym_id = ctx.resolve_type_to_symbol_id(type_id)?;
    let symbol = ctx.binder.get_symbol(sym_id)?;
    symbol.has_any_flags(flags).then_some(sym_id)
}

/// The structural member-value union of an enum type (e.g. `"red" | "blue"` for
/// a string enum, `0 | 1` for a numeric enum). Returns `None` when `type_id` is
/// not an enum type. This is the enum's comparison/overlap value-set.
pub(crate) fn enum_member_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    super::common::enum_member_type(db, type_id)
}

/// When exactly one of `(left, right)` is an enum (and the other is not),
/// returns the operand pair with the enum side replaced by its member-value
/// union — the form an overlap/comparability check should relate. Returns `None`
/// otherwise (neither is an enum, or both are).
///
/// `tsc` relates an enum to a non-enum literal/primitive/union through this
/// member union, so `Color === "red"` overlaps (a member value) while
/// `Color === "green"` does not. Enum-vs-enum comparisons stay nominal — two
/// different enums never overlap even with equal member values, and two members
/// of the same enum compare by their (distinct) values — so the both-enum case
/// is left to the nominal path.
pub(crate) fn enum_comparison_operands(
    db: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> Option<(TypeId, TypeId)> {
    match (
        enum_def_id(db, left).is_some(),
        enum_def_id(db, right).is_some(),
    ) {
        (true, false) => enum_member_type(db, left).map(|members| (members, right)),
        (false, true) => enum_member_type(db, right).map(|members| (left, members)),
        _ => None,
    }
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    super::common::type_parameter_constraint(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Arc<ObjectShape>> {
    super::common::object_shape_for_type(db, type_id)
}
