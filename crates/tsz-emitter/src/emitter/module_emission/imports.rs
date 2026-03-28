use super::super::{ModuleKind, Printer};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn import_has_value_usage_after_node(
        &self,
        node: &Node,
        clause: &tsz_parser::parser::node::ImportClauseData,
    ) -> bool {
        let mut names = Vec::new();
        if clause.name.is_some() {
            let default_name = self.get_identifier_text_idx(clause.name);
            if !default_name.is_empty() {
                names.push(default_name);
            }
        }
        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
            && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
        {
            if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                let ns_name = self.get_identifier_text_idx(named_imports.name);
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
                    let local_name = self.get_identifier_text_idx(spec.name);
                    if !local_name.is_empty() {
                        names.push(local_name);
                    }
                }
            }
        }
        if names.is_empty() {
            return true;
        }
        let Some(source_text) = self.source_text else {
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
        // Strip type-only content from the haystack so that identifiers
        // appearing only in type positions (type annotations, declare lines,
        // other import/export type statements, etc.) don't count as value usages.
        let value_haystack = crate::import_usage::strip_type_only_content(haystack);
        names
            .iter()
            .any(|name| crate::import_usage::contains_identifier_occurrence(&value_haystack, name))
    }

    /// Filter named import specifiers to only those with value-level usage
    /// in the rest of the file. Used in --noCheck mode.
    fn filter_value_specs_by_usage(
        &self,
        import_node: &Node,
        specs: &[NodeIndex],
    ) -> Vec<NodeIndex> {
        let Some(source_text) = self.source_text else {
            return specs.to_vec();
        };
        let Some(import_data) = self.arena.get_import_decl(import_node) else {
            return specs.to_vec();
        };
        let haystack =
            Self::source_after_import(source_text, import_node, import_data, self.arena);
        let value_haystack = crate::import_usage::strip_type_only_content(haystack);

        specs
            .iter()
            .copied()
            .filter(|&spec_idx| {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    return true;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    return true;
                };
                let local_name = self.get_identifier_text_idx(spec.name);
                if local_name.is_empty() {
                    return true;
                }
                crate::import_usage::contains_identifier_occurrence(
                    &value_haystack,
                    &local_name,
                )
            })
            .collect()
    }

    /// Check if an import-equals declaration's identifier is used after the import.
    pub(in crate::emitter) fn import_equals_has_value_usage_after_node(
        &self,
        node: &Node,
        import_data: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let name = self.get_identifier_text_idx(import_data.import_clause);
        if name.is_empty() {
            return true;
        }
        let Some(source_text) = self.source_text else {
            return true;
        };
        let haystack = Self::source_after_import(source_text, node, import_data, self.arena);
        let value_haystack = crate::import_usage::strip_type_only_content(haystack);
        crate::import_usage::contains_identifier_occurrence(&value_haystack, &name)
    }

    /// Check if an import alias name has value usage in the remaining source.
    /// Used for namespace-scoped import alias elision: tsc erases `import X = Y`
    /// inside namespaces when X is only used in type positions.
    /// The search is scope-limited via `namespace_scope_end` to prevent
    /// sibling namespace references from keeping an alias alive.
    fn import_alias_is_referenced_after_node(
        &self,
        node: &Node,
        import_data: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let name = self.get_identifier_text_idx(import_data.import_clause);
        if name.is_empty() {
            return true;
        }
        let Some(source_text) = self.source_text else {
            return true;
        };
        let full_haystack = Self::source_after_import(source_text, node, import_data, self.arena);
        // Limit the search to the current namespace body scope
        let haystack = if self.namespace_scope_end < u32::MAX {
            let full_start_in_source = source_text.len() - full_haystack.len();
            let scope_end_usize = self.namespace_scope_end as usize;
            if scope_end_usize <= full_start_in_source {
                ""
            } else {
                let end_in_full = scope_end_usize - full_start_in_source;
                &full_haystack[..end_in_full.min(full_haystack.len())]
            }
        } else {
            full_haystack
        };
        // Strip type-only content including inline type annotations so that
        // type-position references (e.g., `p1: modes.IMode`) don't count as
        // value usage. This matches tsc which erases namespace import aliases
        // when the alias is only referenced in type positions.
        let stripped = crate::import_usage::strip_type_only_content(haystack);
        crate::import_usage::contains_identifier_occurrence(&stripped, &name)
    }

    /// Get the source text after an import node (skipping to the next line).
    fn source_after_import<'b>(
        source_text: &'b str,
        node: &Node,
        import_data: &tsz_parser::parser::node::ImportDeclData,
        arena: &tsz_parser::parser::node::NodeArena,
    ) -> &'b str {
        let mut start = if let Some(module_node) = arena.get(import_data.module_specifier) {
            module_node.end as usize
        } else {
            node.end as usize
        };
        start = start.min(source_text.len());
        let bytes = source_text.as_bytes();
        // Skip past the entire import line (including trailing comments)
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

    pub(in crate::emitter) fn emit_import_declaration(&mut self, node: &Node) {
        if self.ctx.is_commonjs() {
            self.emit_import_declaration_commonjs(node);
        } else {
            self.emit_import_declaration_es6(node);
        }
    }

    pub(in crate::emitter) fn emit_import_declaration_es6(&mut self, node: &Node) {
        let Some(import) = self.arena.get_import_decl(node) else {
            return;
        };

        if import.import_clause.is_none() {
            self.write("import ");
            self.emit_module_specifier(import.module_specifier);
            self.emit_import_attributes(import.attributes);
            self.write_semicolon();
            return;
        }

        let Some(clause_node) = self.arena.get(import.import_clause) else {
            return;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return;
        };

        if clause.is_type_only {
            return;
        }

        let mut has_default = false;
        let mut namespace_name = None;
        let mut value_specs = Vec::new();
        let mut raw_named_bindings = None;
        let mut trailing_comma = false;

        if clause.name.is_some() {
            has_default = true;
        }

        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
        {
            if let Some(named_imports) = self.arena.get_named_imports(bindings_node) {
                if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                    namespace_name = Some(named_imports.name);
                } else {
                    value_specs = self.collect_value_specifiers(&named_imports.elements);
                    // In --noCheck mode (type_only_nodes empty), apply text-based
                    // heuristic to elide individual named specifiers unused as values.
                    if self.ctx.options.type_only_nodes.is_empty()
                        && !self.source_is_js_file
                        && !self.ctx.options.verbatim_module_syntax
                    {
                        value_specs =
                            self.filter_value_specs_by_usage(node, &value_specs);
                    }
                    trailing_comma = self
                        .has_trailing_comma_in_source(bindings_node, &named_imports.elements.nodes);
                }
            } else {
                raw_named_bindings = Some(clause.named_bindings);
            }
        }

        let has_named =
            namespace_name.is_some() || !value_specs.is_empty() || raw_named_bindings.is_some();
        if !has_default && !has_named {
            return;
        }

        self.write("import ");
        if has_default {
            self.emit(clause.name);
        }

        if has_named {
            if has_default {
                self.write(", ");
            }
            if let Some(name) = namespace_name {
                self.write("* as ");
                self.emit(name);
            } else if !value_specs.is_empty() {
                self.write("{ ");
                self.emit_comma_separated(&value_specs);
                if trailing_comma {
                    self.write(",");
                }
                self.write(" }");
            } else if let Some(raw_node) = raw_named_bindings {
                self.emit(raw_node);
            }
        }

        self.write(" from ");
        self.emit_module_specifier(import.module_specifier);
        self.emit_import_attributes(import.attributes);
        self.write_semicolon();
    }

    pub(in crate::emitter) fn emit_import_declaration_commonjs(&mut self, node: &Node) {
        let Some(import) = self.arena.get_import_decl(node) else {
            return;
        };

        let Some(clause_node) = self.arena.get(import.import_clause) else {
            if matches!(
                self.ctx.original_module_kind,
                Some(ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System)
            ) {
                return;
            }
            // Side-effect import: import "module"; -> emit require
            let module_spec = if let Some(spec_node) = self.arena.get(import.module_specifier) {
                if let Some(lit) = self.arena.get_literal(spec_node) {
                    lit.text.clone()
                } else {
                    return;
                }
            } else {
                return;
            };

            self.write("require(\"");
            self.write(&module_spec);
            self.write("\");");
            self.write_line();
            return;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return;
        };

        if clause.is_type_only {
            return;
        }

        // Detect `import {} from "x"` early — it has no runtime bindings but
        // preserves side effects in CJS mode (same as bare `import "x"`).
        // Must check before the value-usage heuristic, which would elide it.
        let empty_named_import_preserves_side_effects = clause.name.is_none()
            && clause.named_bindings.is_some()
            && self
                .arena
                .get(clause.named_bindings)
                .and_then(|bindings_node| self.arena.get_named_imports(bindings_node))
                .is_some_and(|named_imports| {
                    named_imports.name.is_none() && named_imports.elements.nodes.is_empty()
                });

        if empty_named_import_preserves_side_effects {
            // Wrapped module formats handle imports via wrapper parameters/setters.
            if matches!(
                self.ctx.original_module_kind,
                Some(ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System)
            ) {
                return;
            }
            // `import {} from "x"` → `require("x");` for side effects
            let module_spec = if let Some(spec_node) = self.arena.get(import.module_specifier) {
                if let Some(lit) = self.arena.get_literal(spec_node) {
                    lit.text.clone()
                } else {
                    return;
                }
            } else {
                return;
            };
            let module_spec = self.rewrite_module_spec(&module_spec);
            self.write("require(\"");
            self.write(&module_spec);
            self.write("\");");
            self.write_line();
            return;
        }

        // With --verbatimModuleSyntax or in JS files, non-type-only imports are
        // always preserved (no heuristic elision). tsc's checker treats all
        // imports in JS files as value imports.
        if !self.ctx.options.verbatim_module_syntax
            && !self.source_is_js_file
            && !self.import_has_value_usage_after_node(node, clause)
        {
            return;
        }

        // Module specifier is needed for both binding and side-effect-only CommonJS emit.
        let module_spec = if let Some(spec_node) = self.arena.get(import.module_specifier) {
            if let Some(lit) = self.arena.get_literal(spec_node) {
                lit.text.clone()
            } else {
                return;
            }
        } else {
            return;
        };
        let module_spec = self.rewrite_module_spec(&module_spec);

        // Wrapped module formats bind imports via wrapper parameters/setters.
        // Suppress per-statement CommonJS `require(...)` emission in the body.
        if matches!(
            self.ctx.original_module_kind,
            Some(ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System)
        ) {
            return;
        }

        let mut has_value_binding = clause.name.is_some();
        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
        {
            if let Some(named_imports) = self.arena.get_named_imports(bindings_node) {
                if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                    has_value_binding = true;
                } else {
                    let value_specs = self.collect_value_specifiers(&named_imports.elements);
                    if !value_specs.is_empty() {
                        has_value_binding = true;
                    }
                }
            } else {
                has_value_binding = true;
            }
        }

        if !has_value_binding {
            // `import { type Foo } from "x"` has no runtime bindings and is elided.
            return;
        }

        // Check if this is a namespace-only import (import * as ns from "mod")
        // before allocating a module var, so the counter isn't wasted.
        // Detect from AST: named_bindings has a name but no elements
        let is_namespace_only_ast = clause.name.is_none()
            && clause.named_bindings.is_some()
            && self
                .arena
                .get(clause.named_bindings)
                .and_then(|n| self.arena.get_named_imports(n))
                .is_some_and(|ni| ni.name.is_some() && ni.elements.nodes.is_empty());

        if is_namespace_only_ast {
            // Get the namespace name from the AST
            if let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
            {
                let ns_name = self.get_identifier_text_idx(named_imports.name);
                if !ns_name.is_empty() {
                    self.write_var_or_const();
                    self.write(&ns_name);
                    if self.ctx.options.es_module_interop {
                        // `import * as ns from "mod"` -> `const ns = __importStar(require("mod"));`
                        self.write(" = ");
                        self.write_helper("__importStar");
                        self.write("(require(\"");
                        self.write(&module_spec);
                        self.write("\"));");
                    } else {
                        // `import * as ns from "mod"` -> `const ns = require("mod");`
                        self.write(" = require(\"");
                        self.write(&module_spec);
                        self.write("\");");
                    }
                    self.write_line();
                }
            }
            return;
        }

        // Generate module var name: "./foo" -> "foo_1"
        // This must come after the namespace-only check to avoid wasting
        // counter values on imports that use their own namespace name.
        let module_var = self.next_commonjs_module_var(&module_spec);
        self.register_commonjs_named_import_substitutions(node, &module_var);
        let is_default_only_ast = clause.name.is_some() && clause.named_bindings.is_none();
        let mut is_named_default_only_ast = false;
        if clause.name.is_none()
            && clause.named_bindings.is_some()
            && let Some(bindings_node) = self.arena.get(clause.named_bindings)
            && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
            && named_imports.name.is_none()
        {
            let value_specs = self.collect_value_specifiers(&named_imports.elements);
            is_named_default_only_ast = !value_specs.is_empty()
                && value_specs.iter().all(|&spec_idx| {
                    self.arena.get(spec_idx).is_some_and(|spec_node| {
                        self.arena.get_specifier(spec_node).is_some_and(|spec| {
                            let import_name = if spec.property_name.is_some() {
                                self.get_identifier_text_idx(spec.property_name)
                            } else {
                                self.get_identifier_text_idx(spec.name)
                            };
                            import_name == "default"
                        })
                    })
                });
        }

        if is_default_only_ast || is_named_default_only_ast {
            self.write_var_or_const();
            self.write(&module_var);
            if self.ctx.options.es_module_interop {
                // With esModuleInterop:
                // `import X from "m"` -> `const m_1 = __importDefault(require("m"));`
                self.write(" = ");
                self.write_helper("__importDefault");
                self.write("(require(\"");
                self.write(&module_spec);
                self.write("\"));");
            } else {
                // Without esModuleInterop:
                // `import X from "m"` -> `const m_1 = require("m");`
                self.write(" = require(\"");
                self.write(&module_spec);
                self.write("\");");
            }
            self.write_line();
            return;
        }

        let es_module_interop = self.ctx.options.es_module_interop;

        // Detect combined default + named import: `import foo, {bar} from "mod"`
        // With esModuleInterop, this requires __importStar to wrap the require call
        // so both .default and named exports are accessible.
        let has_default = clause.name.is_some();
        let has_named_bindings = clause.named_bindings.is_some()
            && self.arena.get(clause.named_bindings).is_some_and(|n| {
                n.kind != syntax_kind_ext::NAMESPACE_IMPORT
                    && self
                        .arena
                        .get_named_imports(n)
                        .is_some_and(|ni| ni.name.is_none() || !ni.elements.nodes.is_empty())
            });
        let use_import_star = es_module_interop && has_default && has_named_bindings;

        // Emit: const module_1 = __importStar(require("module"));
        // OR:   const module_1 = require("module");
        self.write_var_or_const();
        self.write(&module_var);
        if use_import_star {
            self.write(" = ");
            self.write_helper("__importStar");
            self.write("(require(\"");
            self.write(&module_spec);
            self.write("\"));");
        } else {
            self.write(" = require(\"");
            self.write(&module_spec);
            self.write("\");");
        }
        self.write_line();
    }

    fn register_commonjs_named_import_substitutions(&mut self, node: &Node, module_var: &str) {
        let Some(import) = self.arena.get_import_decl(node) else {
            return;
        };
        let Some(clause_node) = self.arena.get(import.import_clause) else {
            return;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return;
        };
        if clause.name.is_some()
            && let Some(default_name_node) = self.arena.get(clause.name)
            && let Some(default_ident) = self.arena.get_identifier(default_name_node)
        {
            self.commonjs_named_import_substitutions.insert(
                default_ident.escaped_text.to_string(),
                format!("{module_var}.default"),
            );
        }
        if !clause.named_bindings.is_some() {
            return;
        }
        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return;
        };
        let Some(named_imports) = self.arena.get_named_imports(bindings_node) else {
            return;
        };

        // Skip namespace imports (`import * as ns from "x"`).
        if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
            return;
        }

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
            let Some(local_name_node) = self.arena.get(spec.name) else {
                continue;
            };
            let Some(local_ident) = self.arena.get_identifier(local_name_node) else {
                continue;
            };
            // Get the import name (the original module export name).
            // For `import { "str" as local }`, property_name is the StringLiteral "str".
            // For `import { foo as local }`, property_name is the Identifier foo.
            // For `import { foo }`, there's no property_name and name is the Identifier foo.
            let (import_name, is_string_import) = if spec.property_name.is_some() {
                if let Some(prop_name_node) = self.arena.get(spec.property_name) {
                    if prop_name_node.kind == SyntaxKind::StringLiteral as u16 {
                        if let Some(lit) = self.arena.get_literal(prop_name_node) {
                            (lit.text.clone(), true)
                        } else {
                            (local_ident.escaped_text.to_string(), false)
                        }
                    } else if let Some(prop_ident) = self.arena.get_identifier(prop_name_node) {
                        (prop_ident.escaped_text.to_string(), false)
                    } else {
                        (local_ident.escaped_text.to_string(), false)
                    }
                } else {
                    (local_ident.escaped_text.to_string(), false)
                }
            } else {
                (local_ident.escaped_text.to_string(), false)
            };
            let substitution =
                if is_string_import || !super::super::is_valid_identifier_name(&import_name) {
                    format!("{module_var}[\"{import_name}\"]")
                } else {
                    format!("{module_var}.{import_name}")
                };
            self.commonjs_named_import_substitutions
                .insert(local_ident.escaped_text.to_string(), substitution);
        }
    }

    pub(in crate::emitter) fn emit_import_equals_declaration(&mut self, node: &Node) {
        let before_len = self.writer.len();
        self.emit_import_equals_declaration_inner(node);
        if self.writer.len() > before_len {
            self.write_semicolon();
        }
    }

    pub(in crate::emitter) fn emit_import_equals_declaration_inner(&mut self, node: &Node) {
        let Some(import) = self.arena.get_import_decl(node) else {
            return;
        };

        if !self.import_decl_has_runtime_value(import) {
            return;
        }

        if import.import_clause.is_none() {
            return;
        }

        let Some(module_node) = self.arena.get(import.module_specifier) else {
            return;
        };

        // Inside namespace IIFEs, elide namespace aliases (`import X = Y;`)
        // when X is never referenced in the remaining source.  tsc uses the
        // checker's symbol reference tracking; we use a text-based heuristic.
        //
        // This is restricted to namespace scope because top-level import
        // aliases in scripts create global variables that may be consumed
        // externally, and tsc preserves those even when unreferenced locally.
        let is_namespace_alias = module_node.kind == SyntaxKind::Identifier as u16
            || module_node.kind == syntax_kind_ext::QUALIFIED_NAME;
        if is_namespace_alias
            && self.in_namespace_iife
            && !self.import_alias_is_referenced_after_node(node, import)
        {
            return;
        }

        // Parser recovery can produce missing/invalid module references for
        // malformed `import x = ...;` declarations. TSC skips JS alias emission
        // in that case and preserves only trailing recovered expressions.
        if !self.is_valid_import_equals_reference(import.module_specifier) {
            if self.is_recovered_import_equals_expression(module_node) {
                self.emit_module_specifier(import.module_specifier);
            } else if self
                .recovered_import_equals_rhs_text(node)
                .is_some_and(|rhs| rhs == "null")
            {
                self.write("null");
            }
            return;
        }

        let is_external = module_node.kind == SyntaxKind::StringLiteral as u16
            || module_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE;

        // Wrapped module formats (AMD/UMD/System) bind external imports via wrapper
        // parameters/setters, so we must not emit a duplicate runtime require here.
        if is_external
            && matches!(
                self.ctx.original_module_kind,
                Some(ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System)
            )
        {
            return;
        }

        // `import X = require("module")` uses const/var based on target.
        // `import X = Y` (entity name) always uses `var` per TSC behavior.
        if is_external {
            self.write_var_or_const();
        } else {
            self.write("var ");
        }
        self.emit(import.import_clause);
        self.write(" = ");

        if module_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(module_node) {
                let spec = self.rewrite_module_spec(&lit.text);
                self.write("require(\"");
                self.write(&spec);
                self.write("\")");
            }
            return;
        }

        self.emit_entity_name(import.module_specifier);
    }

    fn is_valid_import_equals_reference(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => true,
            k if k == SyntaxKind::Identifier as u16 => self
                .arena
                .get_identifier(node)
                .is_some_and(|id| !id.escaped_text.is_empty()),
            k if k == SyntaxKind::ThisKeyword as u16 || k == SyntaxKind::SuperKeyword as u16 => {
                true
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                self.arena.get_qualified_name(node).is_some_and(|name| {
                    self.is_valid_import_equals_reference(name.left)
                        && self.is_valid_import_equals_reference(name.right)
                })
            }
            _ => false,
        }
    }

    const fn is_recovered_import_equals_expression(&self, node: &Node) -> bool {
        matches!(
            node.kind,
            k if k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    fn recovered_import_equals_rhs_text(&self, import_node: &Node) -> Option<&'a str> {
        let source = self.source_text_for_map()?;
        let start = import_node.pos as usize;
        let end = (import_node.end as usize).min(source.len());
        if start >= end {
            return None;
        }

        let declaration_text = &source[start..end];
        let equals_pos = declaration_text.find('=')?;
        let rhs_with_suffix = &declaration_text[equals_pos + 1..];
        let rhs = rhs_with_suffix
            .split_once(';')
            .map_or(rhs_with_suffix, |(before_semicolon, _)| before_semicolon)
            .trim();

        (!rhs.is_empty()).then_some(rhs)
    }

    pub(in crate::emitter) fn emit_import_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_import_clause(node) else {
            return;
        };

        let mut has_default = false;

        // Default import
        if clause.name.is_some() {
            self.emit(clause.name);
            has_default = true;
        }

        // Named bindings
        if clause.named_bindings.is_some() {
            if has_default {
                self.write(", ");
            }
            self.emit(clause.named_bindings);
        }
    }

    pub(in crate::emitter) fn emit_wrapped_import_interop_prologue(
        &mut self,
        statements: &NodeList,
    ) {
        if !matches!(
            self.ctx.original_module_kind,
            Some(ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System)
        ) {
            return;
        }

        for &stmt_idx in &statements.nodes {
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

            if clause.name.is_some() {
                let local_name = self.get_identifier_text_idx(clause.name);
                if !local_name.is_empty()
                    && let Some(subst) = self
                        .commonjs_named_import_substitutions
                        .get(local_name.as_str())
                    && let Some(dep_var) = subst.strip_suffix(".default")
                {
                    let dep_var = dep_var.to_string();
                    self.write(&dep_var);
                    self.write(" = ");
                    self.write_helper("__importDefault");
                    self.write("(");
                    self.write(&dep_var);
                    self.write(");");
                    self.write_line();
                }
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
                && named_imports.name.is_some()
                && named_imports.elements.nodes.is_empty()
            {
                let local_name = self.get_identifier_text_idx(named_imports.name);
                if !local_name.is_empty() {
                    self.write(&local_name);
                    self.write(" = ");
                    self.write_helper("__importStar");
                    self.write("(");
                    self.write(&local_name);
                    self.write(");");
                    self.write_line();
                }
            }
        }
    }

    pub(in crate::emitter) fn emit_named_imports(&mut self, node: &Node) {
        let Some(imports) = self.arena.get_named_imports(node) else {
            return;
        };

        // Filter out type-only import specifiers
        let value_imports: Vec<_> = imports
            .elements
            .nodes
            .iter()
            .filter(|&spec_idx| {
                if let Some(spec_node) = self.arena.get(*spec_idx) {
                    if let Some(spec) = self.arena.get_specifier(spec_node) {
                        !spec.is_type_only
                    } else {
                        true
                    }
                } else {
                    true
                }
            })
            .collect();

        // If all imports are type-only, don't emit the named bindings at all
        if value_imports.is_empty() {
            return;
        }

        if imports.name.is_some() && value_imports.is_empty() {
            self.write("* as ");
            self.emit(imports.name);
            return;
        }

        self.write("{ ");
        // Convert Vec<&NodeIndex> to Vec<NodeIndex> for emit_comma_separated
        let value_refs: Vec<NodeIndex> = value_imports.iter().map(|&&idx| idx).collect();
        self.emit_comma_separated(&value_refs);
        // Preserve trailing comma from source
        let has_trailing_comma = self.has_trailing_comma_in_source(node, &imports.elements.nodes);
        if has_trailing_comma {
            self.write(",");
        }
        self.write(" }");
    }

    /// Emit import attributes (e.g., `with { type: "json" }` or `assert { type: "json" }`)
    /// if the given `NodeIndex` points to an `IMPORT_ATTRIBUTES` node.
    pub(in crate::emitter) fn emit_import_attributes(&mut self, attributes: NodeIndex) {
        let Some(attr_node) = self.arena.get(attributes) else {
            return;
        };
        let Some(attrs) = self.arena.get_import_attributes_data(attr_node) else {
            return;
        };
        let keyword = if attrs.token == SyntaxKind::AssertKeyword as u16 {
            "assert"
        } else {
            "with"
        };
        self.write(" ");
        self.write(keyword);
        self.write(" { ");
        for (i, &elem_idx) in attrs.elements.nodes.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            if let Some(elem_node) = self.arena.get(elem_idx)
                && let Some(attr) = self.arena.get_import_attribute_data(elem_node)
            {
                self.emit(attr.name);
                self.write(": ");
                self.emit(attr.value);
            }
        }
        self.write(" }");
    }
}
