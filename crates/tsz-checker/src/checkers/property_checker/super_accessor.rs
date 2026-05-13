use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(super) const fn super_accessor_error(
        &mut self,
        _object_expr: NodeIndex,
        _property_name: &str,
        _error_node: NodeIndex,
        _class_idx: NodeIndex,
        _is_static: bool,
    ) -> bool {
        // Public and protected accessors are valid `super` property targets in
        // TypeScript. Keep this hook as a no-op compatibility shim so the
        // property checker falls through to the normal visibility, missing
        // property, and TS2855 checks instead of emitting the old accessor-only
        // TS2340 false positive.
        false
    }
}
