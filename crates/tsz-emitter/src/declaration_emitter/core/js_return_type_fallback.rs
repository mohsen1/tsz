use tsz_parser::parser::NodeIndex;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_js_body_return_annotation(
        &mut self,
        body: NodeIndex,
    ) -> bool {
        if !body.is_some() {
            return false;
        }
        if let Some(type_text) = self.function_body_preferred_return_type_text(body) {
            self.write(": ");
            self.write(&type_text);
            return true;
        }
        if self.body_returns_void(body) {
            self.write(": void");
            return true;
        }
        false
    }
}
