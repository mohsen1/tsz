use super::super::Printer;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;

#[derive(Default)]
struct LegacySystemDecoratorHelpersNeeded {
    decorate: bool,
    metadata: bool,
    param: bool,
}

impl<'a> Printer<'a> {
    pub(super) fn emit_system_decorate_helper_if_needed(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) {
        if self.ctx.options.no_emit_helpers
            || self.ctx.options.import_helpers
            || !self.ctx.options.legacy_decorators
        {
            return;
        }

        let needed = self.system_source_legacy_decorator_helpers(source);
        if !needed.decorate {
            return;
        }

        for line in crate::transforms::helpers::DECORATE_HELPER.lines() {
            self.write(line);
            self.write_line();
        }

        if needed.metadata {
            for line in crate::transforms::helpers::METADATA_HELPER.lines() {
                self.write(line);
                self.write_line();
            }
        }

        if needed.param {
            for line in crate::transforms::helpers::PARAM_HELPER.lines() {
                self.write(line);
                self.write_line();
            }
        }
    }

    // NOTE: Only scans top-level statements; nested decorated classes in namespaces/blocks are not handled.
    fn system_source_legacy_decorator_helpers(
        &self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> LegacySystemDecoratorHelpersNeeded {
        let mut needed = LegacySystemDecoratorHelpersNeeded::default();
        let mut stack = source.statements.nodes.clone();
        while let Some(stmt_idx) = stack.pop() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if (stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || stmt_node.kind == syntax_kind_ext::CLASS_EXPRESSION)
                && let Some(class_decl) = self.arena.get_class(stmt_node)
            {
                self.accumulate_system_class_legacy_decorator_helpers(class_decl, &mut needed);
            }
            stack.extend(self.arena.get_children(stmt_idx));
        }
        needed
    }

    fn accumulate_system_class_legacy_decorator_helpers(
        &self,
        class_decl: &tsz_parser::parser::node::ClassData,
        needed: &mut LegacySystemDecoratorHelpersNeeded,
    ) {
        let has_class_decorators = !self
            .collect_class_decorators(&class_decl.modifiers)
            .is_empty();
        let ctor_param_decorators =
            self.collect_constructor_param_decorators(&class_decl.members.nodes);
        let has_ctor_param_decorators = !ctor_param_decorators.is_empty();
        let mut has_ctor = false;
        let mut has_decorated_member_call = false;
        let mut has_method_param_decorators = false;
        let mut member_requires_metadata = false;

        for &member_idx in &class_decl.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                has_ctor = true;
                continue;
            }

            match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    let member_decorators =
                        !self.collect_class_decorators(&method.modifiers).is_empty();
                    let has_param_decorators = method.parameters.nodes.iter().any(|&param_idx| {
                        self.arena
                            .get(param_idx)
                            .and_then(|param_node| self.arena.get_parameter(param_node))
                            .is_some_and(|param| {
                                !self.collect_class_decorators(&param.modifiers).is_empty()
                            })
                    });
                    if member_decorators || has_param_decorators {
                        has_decorated_member_call = true;
                        if self.ctx.options.emit_decorator_metadata {
                            member_requires_metadata = true;
                        }
                    }
                    if has_param_decorators {
                        has_method_param_decorators = true;
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let member_decorators =
                        !self.collect_class_decorators(&prop.modifiers).is_empty();
                    if member_decorators {
                        has_decorated_member_call = true;
                        if self.ctx.options.emit_decorator_metadata {
                            member_requires_metadata = true;
                        }
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if !self
                        .collect_class_decorators(&accessor.modifiers)
                        .is_empty()
                    {
                        has_decorated_member_call = true;
                        if self.ctx.options.emit_decorator_metadata {
                            member_requires_metadata = true;
                        }
                    }
                }
                _ => {}
            }
        }

        if has_class_decorators || has_ctor_param_decorators || has_decorated_member_call {
            needed.decorate = true;
        }

        if has_ctor_param_decorators || has_method_param_decorators {
            needed.param = true;
        }

        if self.ctx.options.emit_decorator_metadata {
            let class_assignment_emits_metadata =
                (has_class_decorators || has_ctor_param_decorators) && has_ctor;
            if class_assignment_emits_metadata || member_requires_metadata {
                needed.metadata = true;
            }
        }
    }

    pub(super) fn emit_wrapped_import_helpers(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) {
        if self.ctx.options.no_emit_helpers || self.ctx.options.import_helpers {
            return;
        }

        let mut needs_import_default = false;
        let mut needs_import_star = false;

        // Check if lowering pass detected dynamic import() calls needing __importStar
        if self.transforms.helpers_populated() && self.transforms.helpers().import_star {
            needs_import_star = true;
        }

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            if !self.import_decl_has_runtime_value(import_decl) {
                continue;
            }
            let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only {
                continue;
            }
            if !self.ctx.options.verbatim_module_syntax
                && !self.source_is_js_file
                && !self.is_jsx_factory_import_clause(clause)
                && !self.import_has_value_usage_after_node(stmt_node, clause)
            {
                continue;
            }
            if clause.name.is_some() {
                needs_import_default = true;
            }
            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
                && named_imports.name.is_some()
                && named_imports.elements.nodes.is_empty()
            {
                needs_import_star = true;
            }
        }

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.is_type_only || export_decl.module_specifier.is_none() {
                continue;
            }
            if let Some(clause_node) = self.arena.get(export_decl.export_clause)
                && clause_node.kind != syntax_kind_ext::NAMED_EXPORTS
            {
                needs_import_star = true;
            }
        }

        if needs_import_star {
            self.write(crate::transforms::helpers::CREATE_BINDING_HELPER);
            self.write_line();
            self.write(crate::transforms::helpers::SET_MODULE_DEFAULT_HELPER);
            self.write_line();
            self.write(crate::transforms::helpers::IMPORT_STAR_HELPER);
            self.write_line();
        }
        if needs_import_default {
            self.write(crate::transforms::helpers::IMPORT_DEFAULT_HELPER);
            self.write_line();
        }
    }
}
