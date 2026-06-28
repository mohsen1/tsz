//! ES5 class emitter helpers split out of `transform_dispatch.rs`.

use super::declarations::class::class_has_self_references;
use super::*;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_es5_class_output(
        &mut self,
        es5_emitter: &mut ClassES5Emitter<'a>,
        class_node: NodeIndex,
        binding_name: Option<&str>,
    ) -> String {
        if let Some(binding_name) = binding_name {
            es5_emitter.emit_class_with_binding_name(class_node, binding_name)
        } else {
            es5_emitter.emit_class(class_node)
        }
    }

    /// Create an ES5 class emitter pre-configured with decorator info for the given class.
    pub(in crate::emitter) fn create_es5_class_emitter_with_decorators(
        &mut self,
        class_node: NodeIndex,
    ) -> ClassES5Emitter<'a> {
        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
        es5_emitter
            .set_async_generator_inner_name_counts(self.async_generator_inner_name_counts.clone());
        self.configure_es5_class_emitter_disposable_context(&mut es5_emitter);
        if let Some(class_node_ref) = self.arena.get(class_node)
            && let Some(class_data) = self.arena.get_class(class_node_ref)
        {
            let class_name = self.get_identifier_text_idx(class_data.name);
            self.configure_es5_class_external_hoists(&mut es5_emitter, class_node, &class_name);
        }
        es5_emitter.set_indent_level(self.writer.indent_level());
        es5_emitter.set_transforms(self.transforms.clone());
        es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
        es5_emitter.set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            es5_emitter.set_tslib_prefix(true);
            es5_emitter.set_tslib_import_binding(self.commonjs_tslib_import_binding.clone());
        }
        es5_emitter.set_printer_options(self.ctx.options.clone());
        es5_emitter.set_module_kind(self.ctx.outer_module_kind());
        es5_emitter.set_es_module_interop(self.ctx.options.es_module_interop);
        if let Some(text) = self.source_text_for_map() {
            if self.writer.has_source_map() {
                es5_emitter.set_source_map_context(text, self.writer.current_source_index());
            } else {
                es5_emitter.set_source_text(text);
            }
        }
        if !self.commonjs_named_import_substitutions.is_empty() {
            es5_emitter.set_commonjs_import_substitutions(
                self.commonjs_named_import_substitutions.clone(),
            );
        }

        if self.ctx.target_es5
            && !self.ctx.options.legacy_decorators
            && let Some(class_node_ref) = self.arena.get(class_node)
            && let Some(class_data) = self.arena.get_class(class_node_ref)
            && self.class_has_tc39_decorator_nodes(class_data)
        {
            es5_emitter.set_tc39_decorators(true);
        }

        if self.ctx.options.legacy_decorators
            && let Some(class_node_ref) = self.arena.get(class_node)
            && let Some(class_data) = self.arena.get_class(class_node_ref)
        {
            let class_decorators = self.collect_class_decorators(&class_data.modifiers);
            let class_self_alias = if !class_decorators.is_empty() {
                self.get_identifier_text_opt(class_data.name)
                    .filter(|class_name| {
                        class_has_self_references(
                            self.arena,
                            self.source_text_for_map(),
                            class_name,
                            &class_data.members.nodes,
                        )
                    })
                    .map(|class_name| self.make_unique_name_from_base(&class_name))
            } else {
                None
            };
            let has_member_decorators = class_data.members.nodes.iter().any(|&m_idx| {
                let Some(m_node) = self.arena.get(m_idx) else {
                    return false;
                };
                let mods = match m_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(m_node)
                        .and_then(|m| m.modifiers.as_ref()),
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(m_node)
                        .and_then(|p| p.modifiers.as_ref()),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(m_node)
                            .and_then(|a| a.modifiers.as_ref())
                    }
                    _ => None,
                };
                let has_member_dec = mods.is_some_and(|m| {
                    m.nodes.iter().any(|&mod_idx| {
                        self.arena
                            .get(mod_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    })
                });
                if has_member_dec {
                    return true;
                }

                let params = match m_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        self.arena.get_method_decl(m_node).map(|m| &m.parameters)
                    }
                    k if k == syntax_kind_ext::CONSTRUCTOR => {
                        self.arena.get_constructor(m_node).map(|c| &c.parameters)
                    }
                    _ => None,
                };
                params.is_some_and(|p| {
                    p.nodes.iter().any(|&param_idx| {
                        let Some(param_node) = self.arena.get(param_idx) else {
                            return false;
                        };
                        let Some(param) = self.arena.get_parameter(param_node) else {
                            return false;
                        };
                        param.modifiers.as_ref().is_some_and(|m| {
                            m.nodes.iter().any(|&mod_idx| {
                                self.arena
                                    .get(mod_idx)
                                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                            })
                        })
                    })
                })
            });
            if !class_decorators.is_empty() || has_member_decorators {
                es5_emitter.set_decorator_info(ClassDecoratorInfo {
                    class_decorators,
                    has_member_decorators,
                    emit_decorator_metadata: self.ctx.options.emit_decorator_metadata,
                });
                if let Some(alias) = class_self_alias {
                    es5_emitter.set_class_self_reference_alias(alias);
                }
            }
        }

        es5_emitter
    }
}
