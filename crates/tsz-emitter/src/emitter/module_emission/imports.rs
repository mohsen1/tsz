use super::super::{ModuleKind, Printer};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_import_declaration(&mut self, node: &Node) {
        if let Some(import) = self.arena.get_import_decl(node)
            && let Some(clause_node) = self.arena.get(import.import_clause)
            && clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE
        {
            self.emit_import_equals_declaration(node);
            return;
        }

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
            if self
                .arena
                .has_modifier(&import.modifiers, SyntaxKind::AccessorKeyword)
                || self.has_recovered_accessor_modifier(node)
            {
                self.write("accessor ");
            }
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

        let preserve_invalid_module_syntax = self.recovered_module_syntax_block_depth > 0;

        if self.import_clause_is_empty_named_import(clause) {
            if !(self.ctx.options.verbatim_module_syntax || self.source_is_js_file) {
                return;
            }

            self.write("import {} from ");
            self.emit_module_specifier(import.module_specifier);
            self.emit_import_attributes(import.attributes);
            self.write_semicolon();
            return;
        }

        let mut has_default = false;
        let mut namespace_name = None;
        let mut value_specs = Vec::new();
        let mut raw_named_bindings = None;
        let mut trailing_comma = false;

        if clause.name.is_some() {
            has_default = if preserve_invalid_module_syntax {
                true
            } else if self.ctx.options.type_only_nodes.is_empty()
                && !self.source_is_js_file
                && !self.ctx.options.verbatim_module_syntax
                && !self.is_jsx_factory_import_clause(clause)
            {
                self.default_import_has_value_usage_after_node(node, import, clause.name)
            } else {
                true
            };
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
                        && !preserve_invalid_module_syntax
                    {
                        value_specs = self.filter_value_specs_by_usage(node, &value_specs);
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

        // Elide an unused default binding when another binding survives in the
        // same clause. Mirrors the named-specifier filter above and matches
        // tsc's behavior for `import Foo, { bar } from "x"; bar();` -> emits
        // only `import { bar } from "x";`. JSX-factory defaults are exempt
        // because their name is referenced implicitly by JSX elements.
        if has_default
            && has_named
            && self.ctx.options.type_only_nodes.is_empty()
            && !self.source_is_js_file
            && !self.ctx.options.verbatim_module_syntax
            && !self.is_jsx_factory_import_clause(clause)
            && !self.default_binding_has_value_usage(node, clause.name)
        {
            has_default = false;
        }

        if !has_default && !has_named {
            return;
        }

        if self
            .arena
            .has_modifier(&import.modifiers, SyntaxKind::AccessorKeyword)
            || self.has_recovered_accessor_modifier(node)
        {
            self.write("accessor ");
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
            // AMD and System bind imports via wrapper parameters/setters.
            // UMD uses require() in the body, so don't suppress.
            if matches!(
                self.ctx.original_module_kind,
                Some(ModuleKind::AMD | ModuleKind::System)
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

        let empty_named_import = self.import_clause_is_empty_named_import(clause);
        if empty_named_import
            && !(self.ctx.options.verbatim_module_syntax || self.source_is_js_file)
        {
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

        // AMD and System bind imports via wrapper parameters/setters.
        // Suppress per-statement CommonJS `require(...)` emission in the body.
        // UMD uses require() in the body, so don't suppress.
        if matches!(
            self.ctx.original_module_kind,
            Some(ModuleKind::AMD | ModuleKind::System)
        ) {
            return;
        }

        if empty_named_import {
            self.write("require(\"");
            self.write(&module_spec);
            self.write("\");");
            self.write_line();
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
                    if prop_name_node.is_string_literal() {
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
        self.emit_import_equals_declaration_inner(node, false);
        if self.writer.len() > before_len {
            self.write_semicolon();
        }
    }

    pub(in crate::emitter) fn emit_exported_import_equals_declaration(&mut self, node: &Node) {
        let before_len = self.writer.len();
        self.emit_import_equals_declaration_inner(node, true);
        if self.writer.len() > before_len {
            self.write_semicolon();
        }
    }

    pub(in crate::emitter) fn emit_import_equals_declaration_inner(
        &mut self,
        node: &Node,
        force_exported: bool,
    ) {
        let Some(import) = self.arena.get_import_decl(node) else {
            return;
        };

        if import.import_clause.is_none() {
            return;
        }

        // Check if this import alias is a CJS exported name.
        // In that case, tsc emits `exports.b = a.foo;` directly (no `var`).
        let alias_name = self
            .arena
            .get(import.import_clause)
            .and_then(|n| self.arena.get_identifier(n))
            .map(|id| id.escaped_text.clone());
        let has_export_modifier = self
            .arena
            .has_modifier(&import.modifiers, SyntaxKind::ExportKeyword);
        let is_exported_var = force_exported
            || has_export_modifier
            || alias_name
                .as_ref()
                .is_some_and(|name| self.commonjs_exported_var_names.contains(name.as_str()));

        let Some(module_node) = self.arena.get(import.module_specifier) else {
            return;
        };
        let is_external = module_node.is_string_literal()
            || module_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE;
        let is_node_esm_external =
            is_external && self.ctx.options.resolved_node_module_to_esm && !self.in_namespace_iife;

        if self.recovered_module_syntax_block_depth > 0 && is_external && !is_exported_var {
            self.write("import ");
            self.emit(import.import_clause);
            self.write(" = require(");
            self.emit_module_specifier(import.module_specifier);
            self.write(")");
            return;
        }

        let has_runtime_value = self.import_decl_has_runtime_value(import);
        // Script-mode preservation: when the file is not a module and the
        // alias targets a top-level *interface or type alias* identifier,
        // tsc preserves `var x = T;` (broken-at-runtime) instead of
        // eliding. Top-level type-only declarations create a global
        // identifier that the alias references, so tsc emits the
        // assignment as written. Non-instantiated namespaces are
        // different — tsc still elides them to avoid duplicate-`var`
        // conflicts when the alias name shadows an existing binding
        // (`var a; namespace M {} import a = M;` elides the alias).
        let is_simple_identifier_target = module_node.is_identifier();
        let is_script_mode = !self.ctx.file_is_module
            && self.ctx.original_module_kind.is_none()
            && !self.ctx.options.module_detection_force;
        let target_is_interface_or_type_alias = is_simple_identifier_target
            && self.identifier_target_is_interface_or_type_alias(import.module_specifier);
        let script_mode_preserves_alias = is_script_mode && target_is_interface_or_type_alias;
        let is_namespace_alias =
            module_node.is_identifier() || module_node.kind == syntax_kind_ext::QUALIFIED_NAME;
        if !(has_runtime_value
            || script_mode_preserves_alias
            || is_exported_var && module_node.kind != SyntaxKind::Identifier as u16)
        {
            return;
        }
        // Even when the alias has the `export` modifier, skip the runtime
        // assignment when the qualified target chain resolves to an
        // *exported* interface or type alias (e.g. `export import b = a.I`
        // where namespace `a` exports `interface I`). tsc emits neither the
        // void-0 preamble nor `exports.b = a.I;` in that case. Non-exported
        // inner members are unreachable from outside the namespace and tsc
        // preserves the (broken) runtime emit, so we must not elide there.
        if is_exported_var {
            let stmts = self.scope_statements_for_runtime_lookup(None);
            if !stmts.is_empty()
                && crate::transforms::module_commonjs::import_alias_resolves_to_exported_type_only(
                    self.arena,
                    import.module_specifier,
                    &stmts,
                    self.ctx.options.preserve_const_enums,
                )
            {
                return;
            }
        }

        // Inside namespace IIFEs, elide namespace aliases (`import X = Y;`)
        // when X is never referenced in the remaining source.  tsc uses the
        // checker's symbol reference tracking; we use a text-based heuristic.
        //
        // This is restricted to namespace scope because top-level import
        // aliases in scripts create global variables that may be consumed
        // externally, and tsc preserves those even when unreferenced locally.
        if is_namespace_alias
            && self.in_namespace_iife
            && !is_exported_var
            && !self.import_alias_is_referenced_after_node(node, import)
        {
            return;
        }
        if is_namespace_alias
            && self.ctx.file_is_module
            && !is_exported_var
            && !self.ctx.options.verbatim_module_syntax
            && !self.source_is_js_file
            && !self.import_equals_has_value_usage_after_node(node, import)
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

        // AMD and System bind external imports via wrapper parameters/setters,
        // so we must not emit a duplicate runtime require here.
        // UMD is NOT included because UMD's factory body uses require() calls
        // just like CJS — the define() deps list is only for the AMD branch.
        if is_external
            && matches!(
                self.ctx.original_module_kind,
                Some(ModuleKind::AMD | ModuleKind::System)
            )
        {
            return;
        }

        if self.in_namespace_iife
            && alias_name
                .as_deref()
                .is_some_and(|name| self.namespace_has_prior_import_equals_alias(node, name))
        {
            return;
        }

        if is_node_esm_external && is_exported_var {
            self.write_var_or_const();
            self.emit_decl_name(import.import_clause);
            self.write(" = ");
            self.emit_node_esm_import_equals_require(module_node);
            self.write_semicolon();
            self.write_line();
            self.write("export { ");
            self.emit_decl_name(import.import_clause);
            self.write(" }");
            return;
        }

        if is_exported_var {
            // Emit directly as `exports.b = ...;` — the identifier substitution
            // in emit() will produce `exports.b`.
            self.emit(import.import_clause);
            self.write(" = ");
        } else if is_external {
            // `import X = require("module")` uses const/var based on target.
            self.write_var_or_const();
            self.emit_decl_name(import.import_clause);
            self.write(" = ");
        } else {
            // `import X = Y` (entity name) always uses `var` per TSC behavior.
            self.write("var ");
            self.emit_decl_name(import.import_clause);
            self.write(" = ");
        }

        if module_node.is_string_literal() {
            self.emit_import_equals_require_call(module_node, is_node_esm_external);
            return;
        }

        self.emit_entity_name(import.module_specifier);
    }

    fn emit_node_esm_import_equals_require(&mut self, module_node: &Node) {
        self.emit_import_equals_require_call(module_node, true);
    }

    fn emit_import_equals_require_call(&mut self, module_node: &Node, use_node_esm_require: bool) {
        if let Some(lit) = self.arena.get_literal(module_node) {
            let spec = self.rewrite_module_spec(&lit.text);
            let require_name = if use_node_esm_require {
                self.node_esm_require_name()
            } else {
                "require".to_string()
            };
            self.write(&require_name);
            self.write("(\"");
            self.write(&spec);
            self.write("\")");
        }
    }

    pub(in crate::emitter) fn source_needs_node_esm_create_require(
        &self,
        statements: &tsz_parser::parser::NodeList,
    ) -> bool {
        self.ctx.options.resolved_node_module_to_esm
            && statements.nodes.iter().any(|&stmt_idx| {
                self.arena.get(stmt_idx).is_some_and(|stmt| {
                    if stmt.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                        return self.import_equals_declaration_is_external(stmt);
                    }
                    if let Some(export) = self.arena.get_export_decl(stmt)
                        && let Some(clause_node) = self.arena.get(export.export_clause)
                        && clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    {
                        return self.import_equals_declaration_is_external(clause_node);
                    }
                    false
                })
            })
    }

    pub(in crate::emitter) fn import_equals_declaration_is_external(&self, node: &Node) -> bool {
        self.arena.get_import_decl(node).is_some_and(|import| {
            !import.is_type_only
                && self
                    .arena
                    .get(import.module_specifier)
                    .is_some_and(|module_node| {
                        module_node.is_string_literal()
                            || module_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                    })
        })
    }

    pub(in crate::emitter) fn emit_node_esm_create_require_preamble(&mut self) {
        let (create_require_name, require_name) = self.node_esm_create_require_names();
        self.write("import { createRequire as ");
        self.write(&create_require_name);
        self.write(" } from \"module\";");
        self.write_line();
        self.write_var_or_const();
        self.write(&require_name);
        self.write(" = ");
        self.write(&create_require_name);
        self.write("(import.meta.url);");
        self.write_line();
    }

    fn node_esm_require_name(&mut self) -> String {
        self.node_esm_create_require_names().1
    }

    fn node_esm_create_require_names(&mut self) -> (String, String) {
        if let Some(names) = &self.node_esm_create_require_names {
            return names.clone();
        }
        let create_require_name = self.make_unique_exact_or_numbered_name("_createRequire");
        let require_name = self.make_unique_exact_or_numbered_name("__require");
        let names = (create_require_name, require_name);
        self.node_esm_create_require_names = Some(names.clone());
        names
    }

    fn make_unique_exact_or_numbered_name(&mut self, base: &str) -> String {
        if !self.file_identifiers.contains(base) && !self.generated_temp_names.contains(base) {
            let name = base.to_string();
            self.generated_temp_names.insert(name.clone());
            return name;
        }
        for suffix in 1..=1000 {
            let candidate = format!("{base}_{suffix}");
            if !self.file_identifiers.contains(&candidate)
                && !self.generated_temp_names.contains(&candidate)
            {
                self.generated_temp_names.insert(candidate.clone());
                return candidate;
            }
        }
        self.make_unique_name_fresh()
    }

    fn namespace_has_prior_import_equals_alias(&self, node: &Node, alias_name: &str) -> bool {
        let Some(source_text) = self.source_text else {
            return false;
        };
        let end = (node.pos as usize).min(source_text.len());
        let prefix = &source_text[..end];
        let last_open = prefix.rfind('{').map_or(0, |pos| pos + 1);
        let last_close = prefix.rfind('}').map_or(0, |pos| pos + 1);
        let scope_start = last_open.max(last_close);
        let prior = &source_text[scope_start..end];
        prior.lines().any(|line| {
            let trimmed = line.trim_start();
            let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);
            let Some(rest) = trimmed.strip_prefix("import ") else {
                return false;
            };
            let rest = rest.trim_start();
            let Some(after_name) = rest.strip_prefix(alias_name) else {
                return false;
            };
            let next = after_name.as_bytes().first().copied();
            let boundary =
                next.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_' && b != b'$');
            boundary && after_name.trim_start().starts_with('=')
        })
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

            if !self.ctx.options.verbatim_module_syntax
                && !self.source_is_js_file
                && !self.is_jsx_factory_import_clause(clause)
                && !self.import_has_value_usage_after_node(stmt_node, clause)
            {
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

#[cfg(test)]
mod tests {
    use super::Printer;

    #[test]
    fn import_alias_redeclaration_requires_import_equals() {
        assert!(Printer::contains_alias_value_reference_before_shadow(
            "import M = Z.I;\nM.bar();",
            "M",
        ));
        assert!(!Printer::contains_alias_value_reference_before_shadow(
            "import M from \"pkg\";\nM.bar();",
            "M",
        ));
    }
}
