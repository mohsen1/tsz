//! In-progress analysis-state value types for `CheckerContext`.
//!
//! Deferred implicit-any diagnostics, circular return-site tracking, the
//! inferred-return-type memo key, and object-literal initializer tracking.
//! Extracted from `context/mod.rs` to hold it under the 2000-LOC checker cap;
//! re-exported from the module root (`pub use analysis_state_types::*`) so call
//! sites are unchanged.

use super::PendingImplicitAnyKind;
use rustc_hash::FxHashMap;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{PropertyInfo, TypeId};

/// Deferred implicit-any diagnostic state for a variable declaration.
#[derive(Clone, Copy, Debug)]
pub struct PendingImplicitAnyVar {
    /// Declaration name node used for the TS7034 anchor.
    pub name_node: NodeIndex,
    /// Which deferred implicit-any behavior applies to this declaration.
    pub kind: PendingImplicitAnyKind,
}

/// Closure/function-expression circular return-site state for the active file.
///
/// `sites` are deferred functions whose return expressions read a variable
/// symbol still being resolved; they centralize TS7022/TS7023/TS7024 emission
/// and suppress downstream relation noise. `lazy` is the subset whose
/// self-reference is benign (no contextual return type and no recursive callee
/// self-invocation): those sites still widen the variable to `any`, but their
/// diagnostic is suppressed to match `tsc`, which resolves such deferred
/// references on demand without an error (#10675).
#[derive(Clone, Debug, Default)]
pub struct PendingCircularReturnSites {
    pub sites: FxHashMap<SymbolId, Vec<NodeIndex>>,
    pub lazy: FxHashMap<SymbolId, Vec<NodeIndex>>,
}

impl PendingCircularReturnSites {
    pub fn clear(&mut self) {
        self.sites.clear();
        self.lazy.clear();
    }
}

/// Identity key for the inferred-body-return-type memo (`resolvedReturnType`
/// analog). Captures every ambient input that determines the result of pure
/// return-type inference for a given function/arrow/method body, so a cache hit
/// is byte-identical to recomputing the inference:
///
/// - `function_node`: the function/arrow/method/accessor node being inferred.
/// - `return_context`: the unwrapped contextual return type fed to inference
///   (`None` for context-free inference). Different contextual types can drive
///   different inferred results (e.g. `const` type-parameter context), so they
///   are distinct keys.
/// - `in_const_assertion` / `preserve_literal_types`: literal-preservation modes
///   that change widening of inferred literal returns.
/// - `this_type`: the active contextual `this` type. A method/function body that
///   returns or references `this` (e.g. `foo() { return this; }`, or a
///   `this`-polymorphic predicate) infers a `this`-dependent return type, so the
///   same node inferred under different `this` bindings must not collide.
/// - `scope_fingerprint`: a stable hash of the active `type_parameter_scope`
///   bindings, so a generic body inferred under one ambient type-parameter
///   binding is never reused under a different binding.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct InferredReturnTypeKey {
    pub function_node: NodeIndex,
    pub return_context: Option<TypeId>,
    pub in_const_assertion: bool,
    pub preserve_literal_types: bool,
    pub this_type: Option<TypeId>,
    pub scope_fingerprint: u64,
}

/// In-progress object literal initializer for a variable declaration.
///
/// TypeScript allows later property initializers to reference earlier properties
/// through the variable being initialized, e.g.
/// `const keys = { all: ["x"] as const, list: () => [...keys.all] }`.
/// The full variable type is not available while the object literal is being
/// checked, so this stack exposes only properties that have already been
/// processed for the exact active literal.
#[derive(Clone, Debug)]
pub struct PartialObjectLiteralInitializer {
    pub variable_symbol: SymbolId,
    pub object_literal: NodeIndex,
    pub properties: FxHashMap<Atom, PropertyInfo>,
}

impl PartialObjectLiteralInitializer {
    #[must_use]
    pub fn new(variable_symbol: SymbolId, object_literal: NodeIndex) -> Self {
        Self {
            variable_symbol,
            object_literal,
            properties: FxHashMap::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ObjectLiteralTracking {
    /// Raw object-literal property diagnostic target, keyed by property element for TS2322/TS2345 display recovery.
    pub property_diag_targets: FxHashMap<NodeIndex, TypeId>,
    /// Contextual target type for an object literal, keyed by literal node for per-property diagnostic recovery.
    pub contextual_targets: FxHashMap<NodeIndex, TypeId>,
    /// Stack of in-progress object literal variable initializers.
    pub partial_initializers: Vec<PartialObjectLiteralInitializer>,
    /// Property-level type of a computed-name method/accessor that routed
    /// into an index-signature bucket (wide `string`/`number`/`symbol` key),
    /// keyed by the member element node. Captured at object-literal
    /// computation time — when the member's type is already safely inferred —
    /// so `object_literal_source_type_display` can re-spell the member
    /// (`[ws]: () => number`) without re-running function inference at
    /// display time. #16662.
    pub computed_index_member_display_types: FxHashMap<NodeIndex, TypeId>,
}
