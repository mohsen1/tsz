use super::super::{Printer, get_operator_text};
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_system_live_export_assignment_expression(
        &mut self,
        left: NodeIndex,
        operator: u16,
        right: NodeIndex,
    ) -> bool {
        if !self.is_system_live_export_context() || !self.is_assignment_operator(operator) {
            return false;
        }
        let Some(left_node) = self.arena.get(left) else {
            return false;
        };
        if left_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let local_name = self.get_identifier_text_idx(left);
        let export_names = self.system_export_names_for_local(&local_name);
        if export_names.is_empty() {
            return false;
        }
        self.write_system_export_call_chain_start(&export_names);
        self.write_identifier(&local_name);
        self.write(" ");
        self.write(get_operator_text(operator));
        self.write_space();
        self.emit(right);
        self.write_system_export_call_chain_end(&export_names);
        true
    }

    /// Open the System live named-export call-wrap around a down-leveled
    /// compound-assignment write whose target is a System-exported binding.
    ///
    /// The operator-lowering paths (`**=` for targets `< ES2016`, and
    /// `&&=`/`||=`/`??=` for targets `< ES2021`) expand to an inner
    /// value-producing assignment `x = <value>` that returns before the normal
    /// assignment dispatch and so bypasses
    /// [`Self::emit_system_live_export_assignment_expression`]. To preserve the
    /// live named-export update `tsc` threads through every write, the lowered
    /// write is wrapped `exports_1("x", x = <value>)` (nested for multi-alias
    /// re-exports), exactly as the non-lowered assignment path does.
    ///
    /// When `target` is a bare identifier bound to one or more System exports in
    /// a System live-export context this writes the opening call chain
    /// (`exports_1("alias", exports_1("x", `) and returns the chain depth so the
    /// caller closes it with [`Self::write_system_export_call_chain_close`] after
    /// emitting `x = <value>`. Returns `None` (writing nothing) for non-identifier
    /// / non-exported targets or non-System output, in which case the caller emits
    /// the bare write unchanged. The decision keys on the binding's import/export
    /// origin, never on rendered text.
    pub(in crate::emitter) fn system_live_export_downlevel_write_wrap_open(
        &mut self,
        target: NodeIndex,
    ) -> Option<usize> {
        if !self.is_system_live_export_context() {
            return None;
        }
        let node = self.arena.get(target)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let local_name = self.get_identifier_text_idx(target);
        let export_names = self.system_export_names_for_local(&local_name);
        if export_names.is_empty() {
            return None;
        }
        self.write_system_export_call_chain_start(&export_names);
        Some(export_names.len())
    }

    pub(in crate::emitter) fn emit_system_live_export_prefix_unary(
        &mut self,
        local_name: &str,
        operator: u16,
    ) -> bool {
        if !self.is_system_live_export_context() {
            return false;
        }
        let export_names = self.system_export_names_for_local(local_name);
        if export_names.is_empty() {
            return false;
        }
        self.write_system_export_call_chain_start(&export_names);
        self.write(get_operator_text(operator));
        self.write(local_name);
        self.write_system_export_call_chain_end(&export_names);
        true
    }

    pub(in crate::emitter) fn emit_system_live_export_postfix_unary(
        &mut self,
        local_name: &str,
        operator: u16,
        is_statement: bool,
    ) -> bool {
        if !self.is_system_live_export_context() {
            return false;
        }
        let export_names = self.system_export_names_for_local(local_name);
        if export_names.is_empty() {
            return false;
        }
        if is_statement {
            self.write_system_export_call_chain_start(&export_names);
            self.write("(");
            self.write_identifier(local_name);
            self.write(get_operator_text(operator));
            self.write(", ");
            self.write_identifier(local_name);
            self.write(")");
            self.write_system_export_call_chain_end(&export_names);
            return true;
        }

        let temp = self.make_unique_name_file_hoisted();
        self.write("(");
        self.write(&temp);
        self.write(" = ");
        self.write_identifier(local_name);
        self.write(get_operator_text(operator));
        self.write(", ");
        self.write_system_export_call_chain_start(&export_names);
        self.write_identifier(local_name);
        self.write_system_export_call_chain_end(&export_names);
        self.write(", ");
        self.write(&temp);
        self.write(")");
        true
    }
}
