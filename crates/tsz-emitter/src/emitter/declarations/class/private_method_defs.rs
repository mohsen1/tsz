use super::super::super::Printer;
use std::sync::Arc;
use tsz_parser::parser::NodeIndex;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_private_method_function_def(
        &mut self,
        var_name: &str,
        body_idx: NodeIndex,
        params: &[NodeIndex],
        private_member_def_needs_class_alias: bool,
        class_value_alias: Option<&str>,
        class_name: &str,
    ) {
        self.write(var_name);
        self.write(" = function ");
        self.write(var_name);
        self.write("(");
        self.emit_function_parameters_js(params);
        self.write(") ");

        let prev_self_alias = self.scoped_class_expression_self_alias.clone();
        if private_member_def_needs_class_alias
            && let Some(alias) = class_value_alias
            && !class_name.is_empty()
        {
            self.scoped_class_expression_self_alias =
                Some((Arc::<str>::from(class_name), Arc::<str>::from(alias)));
        }
        self.emit_single_line_block(body_idx);
        self.scoped_class_expression_self_alias = prev_self_alias;
    }
}
