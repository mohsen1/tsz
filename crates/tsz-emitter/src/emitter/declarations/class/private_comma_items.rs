use super::emit_es6_private_accessors::PrivateAutoAccessorInfo;
use crate::emitter::Printer;
use crate::emitter::core::{PrivateAccessorDef, PrivateMethodDef, StaticPrivateInit};

pub(super) struct PrivateCommaItems<'a> {
    pub(super) weakmap_inits: &'a [String],
    pub(super) instances_ws: Option<&'a str>,
    pub(super) private_auto_instance_storage_inits: &'a [String],
    pub(super) method_defs: &'a [PrivateMethodDef],
    pub(super) accessor_defs: &'a [PrivateAccessorDef],
    pub(super) private_member_def_needs_class_alias: bool,
    pub(super) class_value_alias: Option<&'a str>,
    pub(super) class_name: &'a str,
    pub(super) emitted_private_auto_accessors_pre_static: bool,
    pub(super) private_auto_accessors: &'a [PrivateAutoAccessorInfo],
    pub(super) private_class_alias_pair: Option<&'a (String, String)>,
    pub(super) set_function_name: Option<(&'a str, &'a str)>,
    pub(super) static_private_inits: &'a [StaticPrivateInit],
}

impl<'a> Printer<'a> {
    pub(super) fn emit_private_comma_items(&mut self, items: PrivateCommaItems<'_>) {
        for init in items.weakmap_inits {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(init);
            self.decrease_indent();
        }

        if let Some(ws_name) = items.instances_ws {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(ws_name);
            self.write(" = new WeakSet()");
            self.decrease_indent();
        }

        for init in items.private_auto_instance_storage_inits {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(init);
            self.decrease_indent();
        }

        for def in items.method_defs {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.emit_private_method_function_def(
                def,
                items.private_member_def_needs_class_alias,
                items.class_value_alias,
                items.class_name,
            );
            self.decrease_indent();
        }

        for def in items.accessor_defs {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.emit_private_accessor_function_def(
                def,
                items.private_member_def_needs_class_alias,
                items.class_value_alias,
                items.class_name,
            );
            self.decrease_indent();
        }

        if !items.emitted_private_auto_accessors_pre_static {
            for accessor in items.private_auto_accessors {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.emit_private_auto_accessor_function_def(
                    &accessor.get_var_name,
                    &accessor.storage_name,
                    accessor.is_static,
                    true,
                    items
                        .private_class_alias_pair
                        .map(|(alias, _)| alias.as_str())
                        .or(items.class_value_alias),
                );
                self.write(",");
                self.write_line();
                self.emit_private_auto_accessor_function_def(
                    &accessor.set_var_name,
                    &accessor.storage_name,
                    accessor.is_static,
                    false,
                    items
                        .private_class_alias_pair
                        .map(|(alias, _)| alias.as_str())
                        .or(items.class_value_alias),
                );
                self.decrease_indent();
            }
        }

        if let Some((temp, name)) = items.set_function_name {
            self.emit_class_expr_set_function_name_comma_item(temp, name);
        }

        for init in items.static_private_inits {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.emit_static_private_init(init, items.class_name, false);
            self.decrease_indent();
        }

        for accessor in items.private_auto_accessors.iter().filter(|a| a.is_static) {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(&accessor.storage_name);
            self.write(" = { value: ");
            if let Some(init) = accessor.initializer {
                self.emit_expression(init);
            } else {
                self.write("void 0");
            }
            self.write(" }");
            self.decrease_indent();
        }
    }
}
