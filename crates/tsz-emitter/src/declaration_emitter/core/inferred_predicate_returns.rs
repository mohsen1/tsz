use super::DeclarationEmitter;
use crate::declaration_emitter::helpers::LateBoundAssignmentMember;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::FunctionData;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_inferred_predicate_function_return(
        &mut self,
        func_idx: NodeIndex,
        predicate_text: &str,
        func: &FunctionData,
        is_exported: bool,
        should_emit_late_bound_namespace: bool,
        late_bound_members: &[LateBoundAssignmentMember],
    ) {
        self.write(": ");
        self.write(predicate_text);
        self.write(";");
        self.write_line();
        if should_emit_late_bound_namespace {
            self.emit_ts_late_bound_function_namespace_from_members(
                func.name,
                is_exported,
                late_bound_members,
            );
        }
        if !self.emit_js_function_like_class_if_needed(
            func.name,
            &func.parameters,
            func.body,
            is_exported,
            func_idx,
        ) {
            self.emit_js_synthetic_prototype_class_if_needed(func.name, is_exported);
        }
        self.emit_js_namespace_export_aliases_for_name(func.name, is_exported);
        if let Some(body_node) = self.arena.get(func.body) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
    }
}
