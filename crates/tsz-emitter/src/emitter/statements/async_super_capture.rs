use super::super::Printer;
use std::sync::Arc;

pub(in crate::emitter) struct StaticSuperScopeSnapshot {
    base_alias: Option<Arc<str>>,
    direct_access: bool,
    index_alias: Option<Arc<str>>,
    index_value_access: bool,
}

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn enter_pending_lowered_async_arrow_super_capture_scope(
        &mut self,
        is_function_body_block: bool,
    ) -> StaticSuperScopeSnapshot {
        let pending_capture = if is_function_body_block {
            self.pending_lowered_async_arrow_super_capture.take()
        } else {
            None
        };
        let snapshot = StaticSuperScopeSnapshot {
            base_alias: self.scoped_static_super_base_alias.clone(),
            direct_access: self.scoped_static_super_direct_access,
            index_alias: self.scoped_static_super_index_alias.clone(),
            index_value_access: self.scoped_static_super_index_value_access,
        };

        if let Some((capture, super_alias_text, super_index_alias_text)) = pending_capture.as_ref()
        {
            if let Some(index_alias) = super_index_alias_text.as_deref() {
                self.write("const ");
                self.write(index_alias);
                if capture.needs_writable_element_index {
                    self.write(" = (function (geti, seti) {");
                    self.write_line();
                    self.increase_indent();
                    self.write("const cache = Object.create(null);");
                    self.write_line();
                    self.write("return name => cache[name] || (cache[name] = { get value() { return geti(name); }, set value(v) { seti(name, v); } });");
                    self.write_line();
                    self.decrease_indent();
                    self.write("})(name => super[name], (name, value) => super[name] = value);");
                } else {
                    self.write(" = name => super[name];");
                }
                self.write_line();
                self.scoped_static_super_index_alias = Some(Arc::<str>::from(index_alias));
                self.scoped_static_super_index_value_access = capture.needs_writable_element_index;
            }

            if let Some(super_alias) = super_alias_text.as_deref() {
                self.write("const ");
                self.write(super_alias);
                self.write(" = Object.create(null, {");
                self.write_line();
                self.increase_indent();
                for (i, name) in capture.property_names.iter().enumerate() {
                    self.write(name);
                    self.write(": { get: () => super.");
                    self.write(name);
                    if capture.writable_property_names.contains(name) {
                        self.write(", set: v => super.");
                        self.write(name);
                        self.write(" = v");
                    }
                    self.write(" }");
                    if i + 1 < capture.property_names.len() {
                        self.write(",");
                    }
                    self.write_line();
                }
                self.decrease_indent();
                self.write("});");
                self.write_line();
                self.scoped_static_super_base_alias = Some(Arc::<str>::from(super_alias));
                self.scoped_static_super_direct_access = true;
            }
        }

        snapshot
    }

    pub(in crate::emitter) fn restore_static_super_scope(
        &mut self,
        snapshot: StaticSuperScopeSnapshot,
    ) {
        self.scoped_static_super_base_alias = snapshot.base_alias;
        self.scoped_static_super_direct_access = snapshot.direct_access;
        self.scoped_static_super_index_alias = snapshot.index_alias;
        self.scoped_static_super_index_value_access = snapshot.index_value_access;
    }
}
