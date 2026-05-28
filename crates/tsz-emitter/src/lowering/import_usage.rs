//! CommonJS import-retention analysis for the lowering pass.

use super::*;
use crate::emitter::JsxEmit;
use crate::jsx_pragmas::JsxRuntimePragma;

impl<'a> LoweringPass<'a> {
    pub(super) fn visit_import_declaration(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(import_decl) = self.arena.get_import_decl(node) else {
            return;
        };

        // Detect CommonJS helpers needed for imports
        if self.is_commonjs()
            && let Some(clause_node) = self.arena.get(import_decl.import_clause)
            && let Some(clause) = self.arena.get_import_clause(clause_node)
            && !clause.is_type_only
        {
            if !self.ctx.options.verbatim_module_syntax
                && !self.is_classic_jsx_factory_import_clause(clause)
                && !self.import_has_value_usage_after_node(node, clause)
            {
                if import_decl.import_clause.is_some() {
                    self.visit(import_decl.import_clause);
                }
                return;
            }
            // __importDefault and __importStar helpers are only needed when
            // esModuleInterop is enabled. Without it, namespace imports
            // compile to plain `require()` calls and default imports use
            // direct property access on the module object.
            if self.ctx.options.es_module_interop {
                let has_default = clause.name.is_some();
                let has_named_bindings = clause.named_bindings.is_some()
                    && self.arena.get(clause.named_bindings).is_some_and(|n| {
                        // Check for true named bindings (not namespace import)
                        n.kind != syntax_kind_ext::NAMESPACE_IMPORT
                            && self
                                .arena
                                .get_named_imports(n)
                                .is_some_and(|ni| self.named_imports_have_value_usage(node, &ni))
                    });

                // Combined default + named import (e.g., `import foo, {bar} from "mod"`)
                // requires __importStar to wrap the require call so both .default
                // and named exports are accessible.
                if has_default && has_named_bindings {
                    let helpers = self.transforms.helpers_mut();
                    helpers.import_star = true;
                    helpers.create_binding = true;
                } else if has_default {
                    // Default-only import: import d from "mod" -> needs __importDefault
                    let helpers = self.transforms.helpers_mut();
                    helpers.mark_import_default();
                }

                // Namespace import: import * as ns from "mod" -> needs __importStar
                if let Some(bindings_node) = self.arena.get(clause.named_bindings) {
                    // NAMESPACE_IMPORT = 275
                    if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT
                        && self
                            .arena
                            .get_named_imports(bindings_node)
                            .and_then(|named_imports| {
                                self.get_identifier_text_ref(named_imports.name)
                            })
                            .is_some_and(|name| !name.is_empty())
                    {
                        let helpers = self.transforms.helpers_mut();
                        helpers.import_star = true;
                        helpers.create_binding = true;
                    } else if let Some(named_imports) = self.arena.get_named_imports(bindings_node)
                        && self
                            .get_identifier_text_ref(named_imports.name)
                            .is_some_and(|name| !name.is_empty())
                        && named_imports.elements.nodes.is_empty()
                    {
                        let helpers = self.transforms.helpers_mut();
                        helpers.import_star = true;
                        helpers.create_binding = true;
                    } else if let Some(named_imports) = self.arena.get_named_imports(bindings_node)
                    {
                        let has_default_named_import =
                            named_imports.elements.nodes.iter().any(|&spec_idx| {
                                self.arena.get(spec_idx).is_some_and(|spec_node| {
                                    self.arena.get_specifier(spec_node).is_some_and(|spec| {
                                        if spec.is_type_only {
                                            return false;
                                        }
                                        let import_name = if spec.property_name.is_some() {
                                            self.arena
                                                .get(spec.property_name)
                                                .and_then(|prop_node| {
                                                    self.arena.get_identifier(prop_node)
                                                })
                                                .map(|id| id.escaped_text.as_str())
                                        } else {
                                            self.arena
                                                .get(spec.name)
                                                .and_then(|name_node| {
                                                    self.arena.get_identifier(name_node)
                                                })
                                                .map(|id| id.escaped_text.as_str())
                                        };
                                        import_name == Some("default")
                                    })
                                })
                            });
                        if has_default_named_import {
                            let helpers = self.transforms.helpers_mut();
                            helpers.mark_import_default();
                        }
                    }
                }
            }
        }

        // Continue traversal
        if import_decl.import_clause.is_some() {
            self.visit(import_decl.import_clause);
        }
    }

    fn is_classic_jsx_factory_import_clause(
        &self,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        let roots = self.classic_jsx_factory_roots();
        if roots.is_empty() {
            return false;
        }

        if clause.name.is_some() {
            let name = emit_utils::identifier_text_or_empty(self.arena, clause.name);
            if roots.iter().any(|root| root == &name) {
                return true;
            }
        }

        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return false;
        };
        let Some(named_imports) = self.arena.get_named_imports(bindings_node) else {
            return false;
        };

        if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
            let ns_name = emit_utils::identifier_text_or_empty(self.arena, named_imports.name);
            if roots.iter().any(|root| root == &ns_name) {
                return true;
            }
        }

        named_imports.elements.nodes.iter().any(|&spec_idx| {
            self.arena
                .get(spec_idx)
                .and_then(|spec_node| self.arena.get_specifier(spec_node))
                .is_some_and(|spec| {
                    if spec.is_type_only {
                        return false;
                    }
                    let local_name = emit_utils::identifier_text_or_empty(self.arena, spec.name);
                    roots.iter().any(|root| root == &local_name)
                })
        })
    }

    fn classic_jsx_factory_roots(&self) -> Vec<String> {
        let uses_classic_factory = match self.current_jsx_pragmas.runtime {
            Some(JsxRuntimePragma::Classic) => true,
            Some(JsxRuntimePragma::Automatic) => false,
            _ => matches!(
                self.ctx.options.jsx,
                JsxEmit::Preserve | JsxEmit::React | JsxEmit::ReactNative
            ),
        };
        if !uses_classic_factory {
            return Vec::new();
        }
        if !self.has_jsx_syntax() {
            return Vec::new();
        }

        self.current_jsx_pragmas.classic_factory_roots(
            self.ctx.options.jsx_factory.as_deref(),
            self.ctx.options.jsx_fragment_factory.as_deref(),
        )
    }

    fn has_jsx_syntax(&self) -> bool {
        (0..self.arena.len()).any(|idx| {
            self.arena.get(NodeIndex(idx as u32)).is_some_and(|node| {
                node.kind == syntax_kind_ext::JSX_ELEMENT
                    || node.kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                    || node.kind == syntax_kind_ext::JSX_FRAGMENT
            })
        })
    }

    pub(super) fn import_has_value_usage_after_node(
        &self,
        node: &Node,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        let mut names = Vec::new();
        if clause.name.is_some() {
            let default_name = emit_utils::identifier_text_or_empty(self.arena, clause.name);
            if !default_name.is_empty() {
                names.push(default_name);
            }
        }
        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
            && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
        {
            if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                let ns_name = emit_utils::identifier_text_or_empty(self.arena, named_imports.name);
                if !ns_name.is_empty() {
                    names.push(ns_name);
                }
            } else {
                for &spec_idx in &named_imports.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(spec) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if spec.is_type_only {
                        continue;
                    }
                    let local_name = emit_utils::identifier_text_or_empty(self.arena, spec.name);
                    if !local_name.is_empty() {
                        names.push(local_name);
                    }
                }
            }
        }
        if names.is_empty() {
            return true;
        }
        let Some(source_text) = self.current_source_text else {
            return true;
        };
        // Use the module specifier end as the base offset, since node.end may
        // include trailing trivia that extends into the next statement.
        let mut start = if let Some(import_decl) = self.arena.get_import_decl(node)
            && let Some(module_node) = self.arena.get(import_decl.module_specifier)
        {
            module_node.end as usize
        } else {
            node.end as usize
        };
        start = start.min(source_text.len());
        let bytes = source_text.as_bytes();
        // Skip past the entire import line (including trailing comments)
        // to avoid matching identifiers in trailing comments like
        // `import { yield } from "m"; // error to use default as binding name`
        while start < bytes.len() {
            match bytes[start] {
                b'\n' => {
                    start += 1;
                    break;
                }
                b'\r' => {
                    start += 1;
                    if start < bytes.len() && bytes[start] == b'\n' {
                        start += 1;
                    }
                    break;
                }
                _ => start += 1,
            }
        }
        let haystack = &source_text[start..];
        // Strip type-only content so identifiers in type positions
        // (type aliases, interfaces, type annotations, etc.) don't
        // cause unnecessary helper emission.
        let value_haystack = crate::import_usage::strip_type_only_content(haystack);
        let value_haystack = crate::import_usage::strip_qualified_accesses_for_names(
            &value_haystack,
            &self.ctx.options.external_const_enum_bindings,
        );
        if names
            .iter()
            .any(|name| crate::import_usage::contains_identifier_occurrence(&value_haystack, name))
        {
            return true;
        }
        // Under `--emitDecoratorMetadata`, type annotations on decorated class
        // members become *value* references via `__metadata("design:type", X)`.
        // Don't elide imports whose binding appears in such an annotation.
        if self.ctx.options.emit_decorator_metadata
            && names.iter().any(|name| {
                crate::import_usage::name_appears_in_decorator_metadata_type(haystack, name)
            })
        {
            return true;
        }
        self.ctx.target_es5 && self.async_return_type_uses_imported_promise_constructor(&names)
    }

    fn named_imports_have_value_usage(
        &self,
        node: &Node,
        named_imports: &tsz_parser::parser::node::NamedImportsData,
    ) -> bool {
        if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
            return true;
        }

        named_imports.elements.nodes.iter().any(|&spec_idx| {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                return true;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                return true;
            };
            if spec.is_type_only || self.ctx.options.type_only_nodes.contains(&spec_idx) {
                return false;
            }
            let local_name = emit_utils::identifier_text_or_empty(self.arena, spec.name);
            if local_name.is_empty() {
                return true;
            }
            if self
                .classic_jsx_factory_roots()
                .iter()
                .any(|root| root == &local_name)
            {
                return true;
            }
            let Some(source_text) = self.current_source_text else {
                return true;
            };
            let haystack = self.source_after_import_statement(node, source_text);
            let value_haystack = crate::import_usage::strip_type_only_content(haystack);
            let value_haystack = crate::import_usage::strip_qualified_accesses_for_names(
                &value_haystack,
                &self.ctx.options.external_const_enum_bindings,
            );
            if crate::import_usage::contains_identifier_occurrence(&value_haystack, &local_name) {
                return true;
            }
            if self.ctx.options.emit_decorator_metadata
                && crate::import_usage::name_appears_in_decorator_metadata_type(
                    haystack,
                    &local_name,
                )
            {
                return true;
            }
            self.ctx.target_es5
                && self.async_return_type_uses_imported_promise_constructor(&[local_name])
        })
    }

    fn source_after_import_statement<'b>(&self, node: &Node, source_text: &'b str) -> &'b str {
        let mut start = if let Some(import_decl) = self.arena.get_import_decl(node)
            && let Some(module_node) = self.arena.get(import_decl.module_specifier)
        {
            module_node.end as usize
        } else {
            node.end as usize
        };
        start = start.min(source_text.len());
        let bytes = source_text.as_bytes();
        while start < bytes.len() {
            match bytes[start] {
                b'\n' => {
                    start += 1;
                    break;
                }
                b'\r' => {
                    start += 1;
                    if start < bytes.len() && bytes[start] == b'\n' {
                        start += 1;
                    }
                    break;
                }
                _ => start += 1,
            }
        }
        &source_text[start..]
    }

    fn async_return_type_uses_imported_promise_constructor(&self, names: &[String]) -> bool {
        self.arena.nodes.iter().any(|node| match node.kind {
            kind if kind == syntax_kind_ext::FUNCTION_DECLARATION
                || kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || kind == syntax_kind_ext::ARROW_FUNCTION =>
            {
                self.arena.get_function(node).is_some_and(|func| {
                    func.is_async
                        && self
                            .promise_constructor_type_name(func.type_annotation)
                            .is_some_and(|name| names.iter().any(|import| import == &name))
                })
            }
            kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(node).is_some_and(|method| {
                    self.arena
                        .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword)
                        && self
                            .promise_constructor_type_name(method.type_annotation)
                            .is_some_and(|name| names.iter().any(|import| import == &name))
                })
            }
            _ => false,
        })
    }

    fn promise_constructor_type_name(&self, type_annotation: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.arena.get_type_ref(type_node)?;
        let type_name_node = self.arena.get(type_ref.type_name)?;
        if type_name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let name = emit_utils::identifier_text_or_empty(self.arena, type_ref.type_name);
        if name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
            && name != "Promise"
            && name != "PromiseLike"
            && !self.is_type_only_declaration_name(&name)
        {
            Some(name)
        } else {
            None
        }
    }

    /// Returns true when `name` matches a top-level type alias or interface
    /// declaration in this source. Used to avoid mistakenly treating a
    /// `PascalCase` user-defined type for as a runtime promise constructor in
    /// async return-type analysis. Mirrors `is_type_only_declaration_name`
    /// in `crates/tsz-emitter/src/emitter/es5/helpers_async.rs`.
    fn is_type_only_declaration_name(&self, name: &str) -> bool {
        self.arena.nodes.iter().any(|node| {
            if node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                self.arena.get_type_alias(node).is_some_and(|alias| {
                    emit_utils::identifier_text_or_empty(self.arena, alias.name) == name
                })
            } else if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                self.arena.get_interface(node).is_some_and(|interface| {
                    emit_utils::identifier_text_or_empty(self.arena, interface.name) == name
                })
            } else {
                false
            }
        })
    }
}
