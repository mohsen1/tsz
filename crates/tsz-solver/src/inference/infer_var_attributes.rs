//! Per-inference-variable attribute marks.
//!
//! Small recording/query helpers that tag an [`InferenceVar`] with a resolution
//! attribute (top-level-in-return-type, callback-parameter-typed, spread-rest
//! packed) consulted later by candidate resolution. Extracted from `infer.rs`
//! to keep that shard under the solver's per-file line cap; behavior unchanged.

use crate::inference::infer::{InferenceContext, InferenceVar};

impl<'a> InferenceContext<'a> {
    /// Mark an inference variable as representing a type parameter that
    /// occurs at the top level of the signature's return type and has not
    /// yet been fixed. Such variables suppress literal-type widening during
    /// covariant resolution, matching tsc's `getCovariantInference` gate.
    pub fn mark_top_level_in_return_type_unfixed(&mut self, var: InferenceVar) {
        let root = self.table.find(var);
        self.top_level_in_return_type_unfixed.insert(root);
    }

    /// Record that `var` is the type of a callback parameter position in the
    /// call's signature (`(x: T) => …`). Such variables disable the return-type
    /// "first wins" pin during covariant resolution (see
    /// [`Self::vars_typed_by_callback_parameter`], #17761).
    pub fn mark_vars_typed_by_callback_parameter(&mut self, var: InferenceVar) {
        let root = self.table.find(var);
        self.vars_typed_by_callback_parameter.insert(root);
    }

    /// Record that `var` is inferred from a tuple packed out of trailing
    /// rest arguments, so candidate resolution widens its literal elements
    /// per the declared constraint (tsc's `getSpreadArgumentType` rule)
    /// instead of blanket-widening the whole tuple.
    pub fn mark_spread_rest_var(
        &mut self,
        var: InferenceVar,
        mode: crate::inference::spread_rest_literals::SpreadRestLiteralMode,
    ) {
        let root = self.table.find(var);
        self.spread_rest_var_modes.insert(root, mode);
    }

    /// The spread-rest literal mode recorded for `var`, if its candidates
    /// come from a packed rest-argument tuple.
    pub fn spread_rest_mode_of(
        &mut self,
        var: InferenceVar,
    ) -> Option<crate::inference::spread_rest_literals::SpreadRestLiteralMode> {
        let root = self.table.find(var);
        self.spread_rest_var_modes.get(&root).copied()
    }
}
