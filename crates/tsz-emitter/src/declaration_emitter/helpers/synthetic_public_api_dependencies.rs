//! Public API dependency retention helpers for synthetic declaration surfaces

use super::super::{DeclarationEmitter, usage_analyzer::UsageKind};
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn retain_local_type_names_for_public_api(
        &mut self,
        type_text: &str,
    ) {
        let Some(binder) = self.binder else {
            return;
        };
        let Some(used_symbols) = self.used_symbols.as_mut() else {
            return;
        };

        for name in Self::type_reference_identifier_names(type_text) {
            let Some(sym_id) = binder.file_locals.get(name.as_str()) else {
                continue;
            };
            let Some(symbol) = binder.symbols.get(sym_id) else {
                continue;
            };
            if symbol.flags
                & (symbol_flags::CLASS
                    | symbol_flags::INTERFACE
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::ENUM
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::ALIAS)
                == 0
            {
                continue;
            }
            used_symbols
                .entry(sym_id)
                .and_modify(|kind| *kind |= UsageKind::TYPE)
                .or_insert(UsageKind::TYPE);
        }
    }

    fn type_reference_identifier_names(type_text: &str) -> Vec<String> {
        let bytes = type_text.as_bytes();
        let mut names = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if ch == '"' || ch == '\'' {
                i += 1;
                while i < bytes.len() {
                    let current = bytes[i] as char;
                    if current == '\\' {
                        i = (i + 2).min(bytes.len());
                        continue;
                    }
                    i += 1;
                    if current == ch {
                        break;
                    }
                }
                continue;
            }
            if !Self::is_type_reference_identifier_start(ch) {
                i += 1;
                continue;
            }
            let start = i;
            i += 1;
            while i < bytes.len() && Self::is_type_reference_identifier_continue(bytes[i] as char) {
                i += 1;
            }
            if start > 0 && bytes[start - 1] == b'.' {
                continue;
            }
            let name = &type_text[start..i];
            if !Self::is_type_reference_keyword(name) {
                names.push(name.to_string());
            }
        }
        names
    }

    fn is_type_reference_keyword(name: &str) -> bool {
        matches!(
            name,
            "any"
                | "as"
                | "bigint"
                | "boolean"
                | "const"
                | "declare"
                | "default"
                | "export"
                | "extends"
                | "false"
                | "function"
                | "get"
                | "import"
                | "infer"
                | "keyof"
                | "new"
                | "never"
                | "null"
                | "number"
                | "object"
                | "readonly"
                | "set"
                | "string"
                | "symbol"
                | "this"
                | "true"
                | "typeof"
                | "undefined"
                | "unknown"
                | "void"
        )
    }

    pub(in crate::declaration_emitter) fn retain_direct_type_symbols_for_public_api(
        &mut self,
        type_id: tsz_solver::TypeId,
    ) {
        let (Some(type_cache), Some(interner)) = (self.type_cache.as_ref(), self.type_interner)
        else {
            return;
        };

        let mut retained_symbols = Vec::new();
        if let Some(def_id) = tsz_solver::visitor::lazy_def_id(interner, type_id)
            && let Some(&sym_id) = type_cache.def_to_symbol.get(&def_id)
        {
            retained_symbols.push(sym_id);
        }

        if let Some((def_id, _)) = tsz_solver::visitor::enum_components(interner, type_id)
            && let Some(&sym_id) = type_cache.def_to_symbol.get(&def_id)
        {
            retained_symbols.push(sym_id);
        }

        if let Some(shape_id) = tsz_solver::visitor::object_shape_id(interner, type_id)
            .or_else(|| tsz_solver::visitor::object_with_index_shape_id(interner, type_id))
            && let Some(sym_id) = interner.object_shape(shape_id).symbol
        {
            retained_symbols.push(sym_id);
        }

        if let Some(shape_id) = tsz_solver::visitor::callable_shape_id(interner, type_id)
            && let Some(sym_id) = interner.callable_shape(shape_id).symbol
        {
            retained_symbols.push(sym_id);
        }

        let Some(used_symbols) = self.used_symbols.as_mut() else {
            return;
        };
        for &sym_id in &retained_symbols {
            used_symbols
                .entry(sym_id)
                .and_modify(|kind| *kind |= UsageKind::TYPE)
                .or_insert(UsageKind::TYPE);
        }

        for sym_id in retained_symbols {
            self.retain_declaration_type_names_for_public_api(sym_id);
        }
    }

    fn retain_declaration_type_names_for_public_api(&mut self, sym_id: SymbolId) {
        let Some(binder) = self.binder else {
            return;
        };
        let decls = binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.all_declarations())
            .unwrap_or_default();

        for decl_idx in decls {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            match decl_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    self.retain_interface_type_names_for_public_api(decl_idx);
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(alias) = self.arena.get_type_alias(decl_node) {
                        self.retain_type_node_names_for_public_api(alias.type_node);
                    }
                }
                _ => {}
            }
        }
    }

    fn retain_interface_type_names_for_public_api(&mut self, iface_idx: NodeIndex) {
        let Some(iface_node) = self.arena.get(iface_idx) else {
            return;
        };
        let Some(iface) = self.arena.get_interface(iface_node) else {
            return;
        };
        for &member_idx in &iface.members.nodes {
            self.retain_interface_member_type_names_for_public_api(member_idx);
        }
    }

    fn retain_interface_member_type_names_for_public_api(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };
        if let Some(sig) = self.arena.get_signature(member_node) {
            if sig.type_annotation.is_some() {
                self.retain_type_node_names_for_public_api(sig.type_annotation);
            }
            if let Some(params) = sig.parameters.as_ref() {
                for &param_idx in &params.nodes {
                    self.retain_parameter_type_names_for_public_api(param_idx);
                }
            }
        }
    }

    fn retain_parameter_type_names_for_public_api(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return;
        };
        if param.type_annotation.is_some() {
            self.retain_type_node_names_for_public_api(param.type_annotation);
        }
    }

    fn retain_type_node_names_for_public_api(&mut self, type_idx: NodeIndex) {
        if let Some(type_text) = self.emit_type_node_text_normalized(type_idx) {
            self.retain_local_type_names_for_public_api(&type_text);
        }
    }
}
