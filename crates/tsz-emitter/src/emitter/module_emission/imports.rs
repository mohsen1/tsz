use super::super::{ModuleKind, Printer};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
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
            self.emit(import.module_specifier);
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
        self.emit(import.module_specifier);
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

        // Wrapped module formats bind imports via wrapper parameters/setters.
        // Suppress per-statement CommonJS `require(...)` emission in the body.
        if matches!(
            self.ctx.original_module_kind,
            Some(ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System)
        ) {
            return;
        }

        let mut has_value_binding = clause.name.is_some();
        let mut named_bindings_all_type_only = false;
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
                    } else if !named_imports.elements.nodes.is_empty() {
                        // `import { type Foo } from "x"` has no runtime impact in CommonJS.
                        named_bindings_all_type_only = true;
                    }
                }
            } else {
                has_value_binding = true;
            }
        }

        if !has_value_binding {
            if named_bindings_all_type_only {
                return;
            }
            // `import {} from "x"` has no local value bindings but is still a runtime side effect.
            self.write("require(\"");
            self.write(&module_spec);
            self.write("\");");
            self.write_line();
            return;
        }

        // Generate module var name: "./foo" -> "foo_1"
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
                self.write(" = __importDefault(require(\"");
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

        // Check if this is a namespace-only import (import * as ns from "mod")
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
                        self.write(" = __importStar(require(\"");
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

        let es_module_interop = self.ctx.options.es_module_interop;

        // Detect combined default + named import: `import foo, {bar} from "mod"`
        // With esModuleInterop, this requires __importStar to wrap the require call
        // so both .default and named exports are accessible.
        let has_default = clause.name.is_some();
        let has_named_bindings = clause.named_bindings.is_some()
            && self
                .arena
                .get(clause.named_bindings)
                .and_then(|n| self.arena.get_named_imports(n))
                .is_some_and(|ni| {
                    // True named imports (not namespace import)
                    ni.name.is_none() || !ni.elements.nodes.is_empty()
                });
        let use_import_star = es_module_interop && has_default && has_named_bindings;

        // Emit: const module_1 = __importStar(require("module"));
        // OR:   const module_1 = require("module");
        self.write_var_or_const();
        self.write(&module_var);
        if use_import_star {
            self.write(" = __importStar(require(\"");
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
            let import_name = if spec.property_name.is_some() {
                if let Some(prop_name_node) = self.arena.get(spec.property_name) {
                    if let Some(prop_ident) = self.arena.get_identifier(prop_name_node) {
                        prop_ident.escaped_text.as_str()
                    } else {
                        local_ident.escaped_text.as_str()
                    }
                } else {
                    local_ident.escaped_text.as_str()
                }
            } else {
                local_ident.escaped_text.as_str()
            };
            self.commonjs_named_import_substitutions.insert(
                local_ident.escaped_text.to_string(),
                format!("{module_var}.{import_name}"),
            );
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

        // Parser recovery can produce missing/invalid module references for
        // malformed `import x = ...;` declarations. TSC skips JS alias emission
        // in that case and preserves only trailing recovered expressions.
        if !self.is_valid_import_equals_reference(import.module_specifier) {
            if self.is_recovered_import_equals_expression(module_node) {
                self.emit(import.module_specifier);
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
                self.write("require(\"");
                self.write(&lit.text);
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
                    self.write(" = __importDefault(");
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
                    self.write(" = __importStar(");
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
}
