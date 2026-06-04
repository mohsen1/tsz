use crate::construction::TypeDatabase;

#[cfg(test)]
use crate::types::*;

use crate::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, IntrinsicKind,
    LiteralValue, MappedType, ObjectShape, ParamInfo, TemplateSpan, TupleElement, TypeData, TypeId,
    TypeParamInfo, TypePredicate,
};

use rustc_hash::FxHashMap;

use tsz_common::interner::Atom;

/// Maximum depth for recursive type instantiation.
pub const MAX_INSTANTIATION_DEPTH: u32 = 50;

const MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS: usize = 8192;

/// Instantiator for applying type substitutions.
pub struct TypeInstantiator<'a> {
    interner: &'a dyn TypeDatabase,
    substitution: &'a TypeSubstitution,
    /// Track visited types to handle cycles
    visiting: FxHashMap<TypeId, TypeId>,
    /// Type parameter names that are shadowed in the current scope.
    shadowed: Vec<Atom>,
    /// Freshly-instantiated local type parameters for the current nested generic scope.
    local_type_params: Vec<(Atom, TypeId)>,
    substitute_infer: bool,
    preserve_meta_types: bool,
    preserve_unsubstituted_type_params: bool,
    /// When set, substitutes `ThisType` with this concrete type.
    pub this_type: Option<TypeId>,
    /// When set with `this_type`, ONLY substitute `ThisType` references at
    /// type-combinator positions (Intersection / Union / `IndexAccess` / `KeyOf` /
    /// Conditional, etc.). Skip recursion into Object, Function, and Callable
    /// internals so their stored method bodies' `this` references remain
    /// polymorphic for property-access-time rebinding.
    ///
    /// Required for `apply_this_substitution_to_call_return`: when a method
    /// returns `this & T` and the receiver is `Label`, we want
    /// `Label & T_inferred`, NOT a re-baked `Label_obj_with_this_substituted`.
    /// Re-baking poisons subsequent intersection wrapping (the chained
    /// `extend({a}).extend({b})` pattern in `intersectionThisTypes.ts`).
    ///
    /// MUST stay false for class-specialization paths (heritage merge,
    /// `instantiate_type_with_this`) where the substitution legitimately
    /// means "specialize this method body for this class".
    pub shallow_this_only: bool,
    /// When `Some((source, iter_var, declared_type))`, any `IndexAccess(source, K)` where
    /// `K` is a `TypeParameter` with name == `iter_var` is replaced with `declared_type`
    /// instead of being evaluated. Used in homomorphic `-?` mapped type evaluation to feed
    /// the declared (non-optional) property type into the template, matching tsc behavior.
    pub declared_index_type: Option<(TypeId, Atom, TypeId)>,
    depth: u32,
    max_depth: u32,
    depth_exceeded: bool,
    /// Cached: `true` when every key in `substitution.map` is a solver
    /// inference variable (`__infer_*`). The substitution is immutable for the
    /// lifetime of the instantiator, so this is computed once at construction.
    substitution_is_inference_only: bool,
}

include!("instantiate_parts/part1.rs");
include!("instantiate_parts/part2.rs");

mod api;

mod display_properties;

mod homomorphic;

mod substitution;

pub use self::api::*;

use self::api::{
    index_access_operand_needs_resolver, mapped_constraint_needs_resolver,
    template_has_lazy_application_in_composite, type_contains_lazy_application,
};

pub use self::substitution::TypeSubstitution;

#[cfg(test)]
#[path = "../../tests/instantiate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/instantiate_readonly_mapped_tests.rs"]
mod readonly_mapped_tests;
