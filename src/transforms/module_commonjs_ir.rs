//! CommonJS Module Transform (IR-based)
//!
//! Transforms ES modules to CommonJS format, producing IR nodes instead of strings.
//!
//! ```typescript
//! import { foo } from "./module";
//! export const bar = 42;
//! export default myFunc;
//! ```
//!
//! Becomes IR that prints as:
//!
//! ```javascript
//! "use strict";
//! Object.defineProperty(exports, "__esModule", { value: true });
//! exports.bar = void 0;
//! var module_1 = require("./module");
//! var foo = module_1.foo;
//! exports.bar = 42;
//! exports.default = myFunc;
//! ```

use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;

/// Context for CommonJS transformation
pub struct CommonJsTransformContext<'a> {
    arena: &'a NodeArena,
    module_counter: u32,
}

impl<'a> CommonJsTransformContext<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            module_counter: 0,
        }
    }

    /// Transform a source file's statements to CommonJS IR
    pub fn transform_source_file(&mut self, statements: &[NodeIndex]) -> Vec<IRNode> {
        let mut result = Vec::new();

        // Add preamble
        result.push(IRNode::UseStrict);
        result.push(IRNode::EsesModuleMarker);

        // Collect export names for initialization
        let mut exports =
            crate::transforms::module_commonjs::collect_export_names(self.arena, statements);

        // TypeScript emits void 0 initialization in reverse declaration order
        exports.reverse();

        // Initialize exports
        if !exports.is_empty() {
            // Combined export initialization: exports.a = exports.b = ... = void 0;
            result.push(IRNode::Raw(format!(
                "exports.{} = void 0;",
                exports.join(" = exports.")
            )));
        }

        // Transform statements
        for &stmt_idx in statements {
            if let Some(ir) = self.transform_statement(stmt_idx) {
                result.push(ir);
            }
        }

        result
    }

    /// Transform a single statement to IR
    fn transform_statement(&mut self, stmt_idx: NodeIndex) -> Option<IRNode> {
        let stmt_node = self.arena.get(stmt_idx)?;

        match stmt_node.kind {
            k if k == syntax_kind_ext::IMPORT_DECLARATION => self.transform_import(stmt_idx),
            k if k == syntax_kind_ext::EXPORT_DECLARATION => self.transform_export(stmt_idx),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_statement(stmt_idx)
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_statement(stmt_idx)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_statement(stmt_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => self.transform_enum_statement(stmt_idx),
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_namespace_statement(stmt_idx)
            }
            _ => {
                // Pass through as AST reference
                Some(IRNode::ASTRef(stmt_idx))
            }
        }
    }

    /// Transform an import declaration
    fn transform_import(&mut self, import_idx: NodeIndex) -> Option<IRNode> {
        let import = self.arena.get_import_decl_at(import_idx)?;

        // Get module specifier
        let module_spec = get_string_literal_text(self.arena, import.module_specifier)?;
        let module_var = sanitize_module_name(&module_spec);

        // Generate module variable name
        self.module_counter += 1;
        let var_name = format!("{}_1", module_var);

        let mut statements = Vec::new();

        // var module_1 = require("./module");
        statements.push(IRNode::RequireStatement {
            var_name: var_name.clone(),
            module_spec,
        });

        // Process import bindings
        let Some(clause_node) = self.arena.get(import.import_clause) else {
            return None;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return None;
        };

        if clause.is_type_only {
            return None;
        }

        // Default import
        if !clause.name.is_none()
            && let Some(name) = get_identifier_text(self.arena, clause.name)
        {
            statements.push(IRNode::DefaultImport {
                var_name: name,
                module_var: var_name.clone(),
            });
        }

        // Named bindings
        if !clause.named_bindings.is_none()
            && let Some(named_node) = self.arena.get(clause.named_bindings)
            && let Some(named_imports) = self.arena.get_named_imports(named_node)
        {
            // Namespace import: import * as ns from "..."
            if !named_imports.name.is_none() && named_imports.elements.nodes.is_empty() {
                if let Some(name) = get_identifier_text(self.arena, named_imports.name) {
                    statements.push(IRNode::NamespaceImport {
                        var_name: name,
                        module_var: var_name.clone(),
                    });
                }
            } else {
                // Named imports: import { a, b } from "..."
                for &spec_idx in &named_imports.elements.nodes {
                    if let Some(spec_node) = self.arena.get(spec_idx)
                        && let Some(spec) = self.arena.get_specifier(spec_node)
                    {
                        if spec.is_type_only {
                            continue;
                        }
                        let local_name =
                            get_identifier_text(self.arena, spec.name).unwrap_or_default();
                        let import_name = if !spec.property_name.is_none() {
                            get_identifier_text(self.arena, spec.property_name)
                                .unwrap_or(local_name.clone())
                        } else {
                            local_name.clone()
                        };
                        statements.push(IRNode::NamedImport {
                            var_name: local_name,
                            module_var: var_name.clone(),
                            import_name,
                        });
                    }
                }
            }
        }

        Some(IRNode::Block(statements))
    }

    /// Transform an export declaration
    fn transform_export(&mut self, export_idx: NodeIndex) -> Option<IRNode> {
        let export_data = self.arena.get_export_decl_at(export_idx)?;

        if export_data.is_type_only {
            return None;
        }

        // Default export
        if export_data.is_default_export {
            // export default expr;
            let _inner_idx = export_data.export_clause;
            // For now, emit as AST reference
            return Some(IRNode::ASTRef(export_idx));
        }

        // Check for re-exports (export { x } from "./module")
        if !export_data.module_specifier.is_none() {
            return self.transform_re_export(export_data);
        }

        // Regular export - get inner declaration
        let Some(_inner_node) = self.arena.get(export_data.export_clause) else {
            return None;
        };

        // Transform the inner declaration
        self.transform_statement(export_data.export_clause)
    }

    /// Transform a re-export (export { x } from "./module")
    fn transform_re_export(
        &mut self,
        export_data: &crate::parser::node::ExportDeclData,
    ) -> Option<IRNode> {
        let module_spec = get_string_literal_text(self.arena, export_data.module_specifier)?;
        let module_var = sanitize_module_name(&module_spec);

        self.module_counter += 1;
        let var_name = format!("{}_1", module_var);

        let mut statements = Vec::new();

        // var module_1 = require("./module");
        statements.push(IRNode::RequireStatement {
            var_name: var_name.clone(),
            module_spec,
        });

        // Get exported names
        let Some(clause_node) = self.arena.get(export_data.export_clause) else {
            return None;
        };

        if let Some(named_exports) = self.arena.get_named_imports(clause_node) {
            for &spec_idx in &named_exports.elements.nodes {
                if let Some(spec_node) = self.arena.get(spec_idx)
                    && let Some(spec) = self.arena.get_specifier(spec_node)
                {
                    if spec.is_type_only {
                        continue;
                    }
                    let export_name =
                        get_identifier_text(self.arena, spec.name).unwrap_or_default();
                    let import_name = if !spec.property_name.is_none() {
                        get_identifier_text(self.arena, spec.property_name)
                            .unwrap_or(export_name.clone())
                    } else {
                        export_name.clone()
                    };

                    statements.push(IRNode::ReExportProperty {
                        export_name,
                        module_var: var_name.clone(),
                        import_name,
                    });
                }
            }
        }

        Some(IRNode::Block(statements))
    }

    /// Transform a variable statement (check for export modifier)
    fn transform_variable_statement(&mut self, var_idx: NodeIndex) -> Option<IRNode> {
        let var_data = self.arena.get_variable_at(var_idx)?;

        let is_exported = has_export_modifier_from_list(self.arena, &var_data.modifiers);

        if is_exported {
            // Need to add export assignments after the variable statement
            let mut result = Vec::new();

            // The variable statement itself
            result.push(IRNode::ASTRef(var_idx));

            // Export assignments for each declared variable
            for &decl_list_idx in &var_data.declarations.nodes {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                    && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                {
                    for &decl_idx in &decl_list.declarations.nodes {
                        if let Some(decl_node) = self.arena.get(decl_idx)
                            && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                            && let Some(name) = get_identifier_text(self.arena, decl.name)
                        {
                            result.push(IRNode::ExportAssignment { name });
                        }
                    }
                }
            }

            Some(IRNode::Block(result))
        } else {
            Some(IRNode::ASTRef(var_idx))
        }
    }

    /// Transform a function statement (check for export modifier)
    fn transform_function_statement(&mut self, func_idx: NodeIndex) -> Option<IRNode> {
        let func_data = self.arena.get_function_at(func_idx)?;

        let is_exported = has_export_modifier_from_list(self.arena, &func_data.modifiers);

        if is_exported {
            let func_name = get_identifier_text(self.arena, func_data.name)?;
            let mut result = Vec::new();

            // The function declaration
            result.push(IRNode::ASTRef(func_idx));

            // Export assignment
            result.push(IRNode::ExportAssignment { name: func_name });

            Some(IRNode::Block(result))
        } else {
            Some(IRNode::ASTRef(func_idx))
        }
    }

    /// Transform a class statement (check for export modifier)
    fn transform_class_statement(&mut self, class_idx: NodeIndex) -> Option<IRNode> {
        let class_data = self.arena.get_class_at(class_idx)?;

        let is_exported = has_export_modifier_from_list(self.arena, &class_data.modifiers);

        if is_exported {
            let class_name = get_identifier_text(self.arena, class_data.name)?;
            let mut result = Vec::new();

            // The class declaration
            result.push(IRNode::ASTRef(class_idx));

            // Export assignment
            result.push(IRNode::ExportAssignment { name: class_name });

            Some(IRNode::Block(result))
        } else {
            Some(IRNode::ASTRef(class_idx))
        }
    }

    /// Transform an enum statement (check for export modifier)
    fn transform_enum_statement(&mut self, enum_idx: NodeIndex) -> Option<IRNode> {
        let enum_data = self.arena.get_enum_at(enum_idx)?;

        let is_exported = has_export_modifier_from_list(self.arena, &enum_data.modifiers);

        if is_exported {
            let enum_name = get_identifier_text(self.arena, enum_data.name)?;
            let mut result = Vec::new();

            // The enum declaration
            result.push(IRNode::ASTRef(enum_idx));

            // Export assignment
            result.push(IRNode::ExportAssignment { name: enum_name });

            Some(IRNode::Block(result))
        } else {
            Some(IRNode::ASTRef(enum_idx))
        }
    }

    /// Transform a namespace statement (check for export modifier)
    fn transform_namespace_statement(&mut self, ns_idx: NodeIndex) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        let is_exported = has_export_modifier_from_list(self.arena, &ns_data.modifiers);

        if is_exported {
            let ns_name = get_identifier_text(self.arena, ns_data.name)?;
            let mut result = Vec::new();

            // The namespace declaration
            result.push(IRNode::ASTRef(ns_idx));

            // Export assignment
            result.push(IRNode::ExportAssignment { name: ns_name });

            Some(IRNode::Block(result))
        } else {
            Some(IRNode::ASTRef(ns_idx))
        }
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn get_identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        arena.get_identifier(node).map(|id| id.escaped_text.clone())
    } else {
        None
    }
}

fn get_string_literal_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::StringLiteral as u16 {
        arena.get_literal(node).map(|s| s.text.clone())
    } else {
        None
    }
}

fn has_modifier(arena: &NodeArena, modifiers: &Option<crate::parser::NodeList>, kind: u16) -> bool {
    if let Some(mods) = modifiers {
        for &mod_idx in &mods.nodes {
            if let Some(mod_node) = arena.get(mod_idx)
                && mod_node.kind == kind
            {
                return true;
            }
        }
    }
    false
}

fn has_export_modifier_from_list(
    arena: &NodeArena,
    modifiers: &Option<crate::parser::NodeList>,
) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::ExportKeyword as u16)
}

/// Sanitize module specifier for use as variable name
pub fn sanitize_module_name(module_spec: &str) -> String {
    module_spec
        .trim_start_matches("./")
        .trim_start_matches("../")
        .replace(['/', '-', '.', '@'], "_")
}
