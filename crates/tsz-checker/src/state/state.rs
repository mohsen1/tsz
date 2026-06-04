use crate::CheckerContext;

use crate::context::{CheckerOptions, TypingRequest};

use crate::control_flow::type_guards::reference_uses_outer_class_property_initializer_binding;

use crate::query_boundaries::common::QueryDatabase;

use tsz_binder::BinderState;

use tsz_binder::SymbolId;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeArena;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

use tsz_solver::computation::TypeEnvironment;

thread_local! {
    /// Shared depth counter for all cross-arena delegation points.
    /// Prevents stack overflow from deeply nested CheckerState creation.
    static CROSS_ARENA_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Reset the cross-arena delegation depth counter to zero.
///
/// `enter_cross_arena_delegation` / `leave_cross_arena_delegation` are a manual
/// (non-RAII) enter/leave pair, so a compilation that bails out between them
/// without unwinding — e.g. the stack-overflow breaker tripping or resolution
/// fuel running out — can leave the counter non-zero. A leftover depth would
/// then make `enter_cross_arena_delegation` refuse delegation in an unrelated
/// later compilation. Reset between independent compilations (batch mode) so a
/// pathological project cannot poison the next one.
pub(crate) fn reset_cross_arena_depth() {
    CROSS_ARENA_DEPTH.with(|c| c.set(0));
}

#[cfg(test)]
pub(crate) fn set_cross_arena_depth_for_test(value: u32) {
    CROSS_ARENA_DEPTH.with(|c| c.set(value));
}

#[cfg(test)]
pub(crate) fn cross_arena_depth_for_test() -> u32 {
    CROSS_ARENA_DEPTH.with(std::cell::Cell::get)
}

/// Type checker state using `NodeArena` and Solver type system.
///
/// This is a performance-optimized checker that works directly with the
/// cache-friendly Node architecture and uses the solver's `TypeInterner`
/// for structural type equality.
///
/// The state is stored in a `CheckerContext` which can be shared with
/// specialized checker modules (expressions, statements, declarations).
pub struct CheckerState<'a> {
    /// Shared checker context containing all state.
    pub ctx: CheckerContext<'a>,
}

pub use tsz_common::limits::MAX_CALL_DEPTH;

pub use tsz_common::limits::MAX_INSTANTIATION_DEPTH;

pub use tsz_common::limits::MAX_TREE_WALK_ITERATIONS;

pub use tsz_common::limits::MAX_TYPE_RESOLUTION_OPS;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum EnumKind {
    Numeric,
    String,
    Mixed,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum MemberAccessLevel {
    Private,
    Protected,
}

#[derive(Clone, Debug)]
pub(crate) struct MemberAccessInfo {
    pub(crate) level: MemberAccessLevel,
    pub(crate) declaring_class_idx: NodeIndex,
    pub(crate) declaring_class_name: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum MemberLookup {
    NotFound,
    Public,
    Restricted(MemberAccessLevel),
}

pub(crate) use crate::flow_analysis::{ComputedKey, PropertyKey};

/// Mode for resolving parameter types during extraction.
/// Used to consolidate duplicate parameter extraction functions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParamTypeResolutionMode {
    /// Use `get_type_from_type_node_in_type_literal` - for type literal contexts
    InTypeLiteral,
    /// Use `get_type_from_type_node` - for declaration contexts
    FromTypeNode,
}

/// Helper struct that implements `AssignabilityOverrideProvider` by delegating
/// to `CheckerState` methods. Captures the `TypeEnvironment` reference.
pub(crate) struct CheckerOverrideProvider<'a, 'b> {
    checker: &'a CheckerState<'b>,
    env: Option<&'a TypeEnvironment>,
}

impl<'a, 'b> CheckerOverrideProvider<'a, 'b> {
    pub(crate) const fn new(
        checker: &'a CheckerState<'b>,
        env: Option<&'a TypeEnvironment>,
    ) -> Self {
        Self { checker, env }
    }
}

impl<'a, 'b> tsz_solver::relations::compat::AssignabilityOverrideProvider
    for CheckerOverrideProvider<'a, 'b>
{
    fn enum_assignability_override(&self, source: TypeId, target: TypeId) -> Option<bool> {
        self.checker.enum_assignability_override(source, target)
    }

    fn abstract_constructor_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        self.checker
            .abstract_constructor_assignability_override(source, target, self.env)
    }

    fn constructor_accessibility_override(&self, source: TypeId, target: TypeId) -> Option<bool> {
        self.checker
            .constructor_accessibility_override(source, target, self.env)
    }
}

include!("state_parts/part1.rs");
include!("state_parts/part2.rs");
