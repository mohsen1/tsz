use super::super::super::Printer;
use crate::emitter::core::PrivateMethodDef;
use std::sync::Arc;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_private_method_function_def(
        &mut self,
        def: &PrivateMethodDef,
        private_member_def_needs_class_alias: bool,
        class_value_alias: Option<&str>,
        class_name: &str,
    ) {
        self.write(&def.var_name);
        self.write(" = ");
        if def.is_async {
            self.write("async ");
        }
        self.write("function");
        if def.is_generator {
            self.write("*");
        }
        self.write(" ");
        self.write(&def.var_name);
        self.write("(");
        self.function_scope_depth += 1;
        self.emit_function_parameters_js(&def.params);
        self.write(") ");

        let prev_self_alias = self.scoped_class_expression_self_alias.clone();
        if private_member_def_needs_class_alias
            && let Some(alias) = class_value_alias
            && !class_name.is_empty()
        {
            self.scoped_class_expression_self_alias =
                Some((Arc::<str>::from(class_name), Arc::<str>::from(alias)));
        }
        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        let prev_pending_function_body_parameters = std::mem::replace(
            &mut self.pending_function_body_parameters,
            def.params.clone(),
        );
        self.ctx.block_scope_state.enter_scope();
        self.push_temp_scope();
        let prev_declared = std::mem::take(&mut self.declared_namespace_names);
        self.prepare_logical_assignment_value_temps(def.body);
        let prev_in_generator = self.ctx.flags.in_generator;
        self.ctx.flags.in_generator = def.is_generator;
        self.emit(def.body);
        self.ctx.flags.in_generator = prev_in_generator;
        self.declared_namespace_names = prev_declared;
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
        self.pending_function_body_parameters = prev_pending_function_body_parameters;
        self.emitting_function_body_block = prev_emitting_function_body_block;
        self.function_scope_depth -= 1;
        self.scoped_class_expression_self_alias = prev_self_alias;
    }
}
