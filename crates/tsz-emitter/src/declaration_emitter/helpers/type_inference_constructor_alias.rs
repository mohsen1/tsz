use super::super::DeclarationEmitter;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn new_expression_variable_constructor_alias_instance_type_text(
        &self,
        expr_idx: NodeIndex,
        inferred_type_text: &str,
    ) -> Option<String> {
        let initializer = self.variable_constructor_alias_initializer(expr_idx)?;
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(init_node)?;
        let args = call.arguments.as_ref()?;
        let base_arg = args.nodes.iter().copied().find(|&arg_idx| {
            self.declaration_constructor_expression_text(arg_idx)
                .is_some()
        })?;
        let base_text = self.declaration_constructor_expression_text(base_arg)?;
        let sym_id = self.value_reference_symbol(base_arg)?;
        let defaults =
            self.constructor_symbol_default_type_args_text(sym_id, inferred_type_text)?;
        if defaults.is_empty() {
            return None;
        }
        Some(format!("{base_text}<{}>", defaults.join(", ")))
    }

    fn variable_constructor_alias_initializer(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let symbol = self.binder.and_then(|binder| binder.symbols.get(sym_id))?;
        if !symbol.has_any_flags(symbol_flags::VARIABLE) {
            return None;
        }
        symbol.declarations.iter().copied().find_map(|decl_idx| {
            self.arena
                .get(decl_idx)
                .and_then(|decl_node| self.arena.get_variable_declaration(decl_node))
                .and_then(|decl| decl.initializer.is_some().then_some(decl.initializer))
        })
    }

    fn constructor_symbol_default_type_args_text(
        &self,
        sym_id: SymbolId,
        inferred_type_text: &str,
    ) -> Option<Vec<String>> {
        let binder = self.binder?;
        let resolved_sym_id = self
            .resolve_alias_in_source_context(sym_id, binder)
            .unwrap_or(sym_id);
        let symbol = binder.symbols.get(resolved_sym_id)?;
        let module_specifier = Self::single_import_type_reference_module(inferred_type_text);

        let mut arenas: Vec<&NodeArena> = Vec::new();
        arenas.push(self.arena);
        if let Some(source_arena) = binder.symbol_arenas.get(&resolved_sym_id) {
            arenas.push(source_arena.as_ref());
        }
        if let Some(source_arena) = self.global_symbol_arenas.get(&resolved_sym_id) {
            arenas.push(source_arena.as_ref());
        }

        for &decl_idx in &symbol.declarations {
            for source_arena in &arenas {
                let Some(decl_node) = source_arena.get(decl_idx) else {
                    continue;
                };
                let Some(class_data) = source_arena.get_class(decl_node) else {
                    continue;
                };
                let Some(type_parameters) = class_data.type_parameters.as_ref() else {
                    return Some(Vec::new());
                };
                let defaults = type_parameters
                    .nodes
                    .iter()
                    .map(|&param_idx| {
                        let param_node = source_arena.get(param_idx)?;
                        let param = source_arena.get_type_parameter(param_node)?;
                        let raw = self.source_slice_from_arena(source_arena, param.default)?;
                        Some(self.qualify_constructor_alias_default_type_arg(
                            raw.trim(),
                            module_specifier.as_deref(),
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?;
                return Some(defaults);
            }
        }

        None
    }

    fn qualify_constructor_alias_default_type_arg(
        &self,
        type_text: &str,
        module_specifier: Option<&str>,
    ) -> String {
        let Some(module_specifier) = module_specifier else {
            return type_text.to_string();
        };
        let Some(name) = Self::simple_type_reference_name(type_text) else {
            return type_text.to_string();
        };
        if name != type_text {
            return type_text.to_string();
        }
        format!("import(\"{module_specifier}\").{name}")
    }

    fn single_import_type_reference_module(type_text: &str) -> Option<String> {
        let (start, module_specifier, tail) = Self::next_import_type_text(type_text)?;
        if start != 0 || Self::next_import_type_text(tail).is_some() {
            return None;
        }
        let tail = tail.trim_start();
        if !tail.starts_with('.') {
            return None;
        }
        Some(module_specifier)
    }
}
