use super::super::super::Printer;
use crate::emitter::core::{PrivateFieldStorageKind, StaticPrivateInit};
use crate::transforms::private_fields_es5::{
    get_private_field_name, is_private_identifier, make_unique_private_name,
};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ClassData;
use tsz_scanner::SyntaxKind;

#[derive(Debug, Clone)]
pub(super) struct PrivateAutoAccessorInfo {
    pub(super) member_idx: NodeIndex,
    pub(super) name: String,
    pub(super) get_var_name: String,
    pub(super) set_var_name: String,
    pub(super) storage_name: String,
    pub(super) initializer: Option<NodeIndex>,
    pub(super) is_static: bool,
}

pub(super) fn collect_private_auto_accessors_with_reserved(
    printer: &Printer<'_>,
    class: &ClassData,
    class_name: &str,
    used_names: &mut rustc_hash::FxHashSet<String>,
) -> Vec<PrivateAutoAccessorInfo> {
    if class_name.is_empty() {
        return Vec::new();
    }

    let mut accessors = Vec::new();
    for &member_idx in &class.members.nodes {
        let Some(member_node) = printer.arena.get(member_idx) else {
            continue;
        };
        let Some(prop) = printer.arena.get_property_decl(member_node) else {
            continue;
        };
        if !printer
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
        {
            continue;
        }
        if printer
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
            || printer
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
        {
            continue;
        }
        if !is_private_identifier(printer.arena, prop.name) {
            continue;
        }

        let Some(field_name) = get_private_field_name(printer.arena, prop.name) else {
            continue;
        };
        let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
        let base = format!("_{class_name}_{clean_name}");
        let get_var_name = make_unique_private_name(&format!("{base}_get"), used_names);
        let set_var_name = make_unique_private_name(&format!("{base}_set"), used_names);
        let storage_name = if printer.ctx.options.legacy_decorators {
            used_names.insert(base.clone());
            let storage_stem = make_unique_private_name(&base, used_names);
            make_unique_private_name(&format!("{storage_stem}_accessor_storage"), used_names)
        } else {
            make_unique_private_name(&format!("{base}_accessor_storage"), used_names)
        };
        accessors.push(PrivateAutoAccessorInfo {
            member_idx,
            name: clean_name.to_string(),
            get_var_name,
            set_var_name,
            storage_name,
            initializer: if prop.initializer.is_none() {
                None
            } else {
                Some(prop.initializer)
            },
            is_static: printer.has_effective_static_modifier_js(&prop.modifiers),
        });
    }
    accessors
}

impl<'a> Printer<'a> {
    pub(super) fn emit_static_private_init(
        &mut self,
        init: &StaticPrivateInit,
        class_name: &str,
        with_semicolon: bool,
    ) {
        self.write(&init.storage_name);
        match init.storage_kind {
            PrivateFieldStorageKind::WeakMap => {
                self.write(".set(");
                self.write(class_name);
                self.write(", ");
            }
            PrivateFieldStorageKind::Value => {
                self.write(" = { value: ");
            }
        }
        if init.initializer.is_some() {
            self.emit_expression(init.initializer);
        } else {
            self.write("void 0");
        }
        match init.storage_kind {
            PrivateFieldStorageKind::WeakMap => self.write(")"),
            PrivateFieldStorageKind::Value => self.write(" }"),
        }
        if with_semicolon {
            self.write(";");
        }
    }

    pub(in crate::emitter) fn emit_private_auto_accessor_function_def(
        &mut self,
        var_name: &str,
        storage_name: &str,
        is_static: bool,
        is_get: bool,
        class_alias: Option<&str>,
    ) {
        self.write(var_name);
        self.write(" = function ");
        self.write(var_name);
        self.write("(");
        if !is_get {
            self.write("value");
        }
        self.write(") { ");
        if is_get {
            self.write("return ");
            self.write_helper("__classPrivateFieldGet");
        } else {
            self.write_helper("__classPrivateFieldSet");
        }
        self.write("(");
        if is_static {
            let alias = class_alias.unwrap_or("this");
            self.write(alias);
            self.write(", ");
            self.write(alias);
            if is_get {
                self.write(", \"f\", ");
                self.write(storage_name);
            } else {
                self.write(", value, \"f\", ");
                self.write(storage_name);
            }
        } else {
            self.write("this, ");
            self.write(storage_name);
            if is_get {
                self.write(", \"f\"");
            } else {
                self.write(", value, \"f\"");
            }
        }
        self.write("); }");
    }
}
