//! ES module (`--module es2015|esnext`) downlevel export-destructuring lowering.
//!
//! At an ES5/ES3 target a `export const <pattern> = <init>` whose source is not
//! a reusable identifier needs a temporary to hold the source value. `tsc`
//! hoists that temporary as a plain, non-exported `var _a;` and folds its
//! assignment into the first binding via a comma expression
//! (`export var first = (_a = init, _a[0]), rest = _a.slice(1);`), so the
//! temporary never becomes a named export.
//!
//! tsz's shared ES5 destructuring path instead emits the temporary inline in the
//! `export var` list (`export var _a = init, first = _a[0], ...`), which leaks
//! `_a` as a spurious named export of the module. This module reproduces `tsc`'s
//! hoisted-comma form for the flat array/object patterns that trigger the leak;
//! any shape it does not model (nested/defaulted/computed-key patterns,
//! `downlevelIteration` array reads, object rest — handled by
//! `emit_esm_object_rest_export_statement` — reusable identifier sources, or a
//! single-element pattern that `tsc` already inlines) is left to the existing
//! path unchanged.

use super::super::{ModuleKind, Printer};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// How a single binding reads its value out of the hoisted source temp.
enum EsmDestructAccess {
    /// Array element access `temp[index]`.
    Index(usize),
    /// Array rest `temp.slice(index)`.
    Slice(usize),
    /// Object property access `temp.<key>` (identifier key only).
    Prop(NodeIndex),
}

/// One flattened binding: the target identifier plus how it reads from the temp.
struct EsmDestructBinding {
    name: NodeIndex,
    access: EsmDestructAccess,
}

impl Printer<'_> {
    /// Emit an exported destructuring declaration through `tsc`'s hoisted-temp
    /// comma form when it would otherwise leak the synthesized source temp as a
    /// named export. Returns `false` (leaving the statement to the existing
    /// path) for every shape not modeled here.
    pub(in crate::emitter) fn emit_esm_destructuring_export_statement(
        &mut self,
        node: &Node,
    ) -> bool {
        if !matches!(
            self.ctx.options.module,
            ModuleKind::ES2015 | ModuleKind::ESNext
        ) {
            return false;
        }
        // Below ES2015 the binding pattern is lowered and needs the temp; at
        // ES2015+ destructuring is emitted natively, so there is no leak.
        if !self.ctx.target_es5 {
            return false;
        }

        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };
        if !self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
            || self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::DefaultKeyword)
            || self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
        {
            return false;
        }

        // Scope to a single declaration so temp numbering and comma joining stay
        // trivially correct; multi-declaration lists fall through unchanged.
        if var_stmt.declarations.nodes.len() != 1 {
            return false;
        }
        let decl_list_idx = var_stmt.declarations.nodes[0];
        let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
            return false;
        };
        let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
            return false;
        };
        if decl_list.declarations.nodes.len() != 1 {
            return false;
        }
        let decl_idx = decl_list.declarations.nodes[0];
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if decl.initializer.is_none() {
            return false;
        }
        let Some(pattern_node) = self.arena.get(decl.name) else {
            return false;
        };

        // Structural gate first: most statements reaching here are not a modeled
        // flat multi-element pattern, and this bails without allocating the
        // identifier text the reusable-source check below would.
        let Some(bindings) = self.collect_esm_flat_destructuring_bindings(pattern_node) else {
            return false;
        };

        // A reusable identifier source is repeated inline at every access by the
        // existing path (no temp, no leak); only non-reusable sources leak.
        if self
            .reusable_object_rest_export_source(decl.initializer)
            .is_some()
        {
            return false;
        }

        let temp = self.make_unique_name_hoisted();
        // The `export ` keyword is already written by the enclosing transform
        // path before this statement's body is emitted (unlike
        // `emit_esm_object_rest_export_statement`, which is reached through the
        // non-transform path and writes its own `export `). Emit only the `var`
        // keyword and the bindings here.
        self.write("var ");
        for (index, binding) in bindings.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.write_binding_identifier_text(binding.name);
            self.write(" = ");
            if index == 0 {
                // Fold the source assignment into the first binding so the
                // hoisted temp is never part of the exported declaration list.
                self.write("(");
                self.write(&temp);
                self.write(" = ");
                self.emit(decl.initializer);
                self.write(", ");
                self.emit_esm_destruct_access(&temp, &binding.access);
                self.write(")");
            } else {
                self.emit_esm_destruct_access(&temp, &binding.access);
            }
        }
        self.write_semicolon();
        true
    }

    /// Flatten a supported array/object binding pattern into `(target, access)`
    /// pairs, or `None` for any shape this lowering does not model.
    fn collect_esm_flat_destructuring_bindings(
        &self,
        pattern_node: &Node,
    ) -> Option<Vec<EsmDestructBinding>> {
        let pattern = self.arena.get_binding_pattern(pattern_node)?;
        // A single-element pattern is inlined by `tsc` (the source is read once,
        // so no temp is minted); the existing path already matches that. Only a
        // pattern with more than one element mints the leaking temp.
        if pattern.elements.nodes.len() < 2 {
            return None;
        }

        let mut bindings = Vec::new();
        match pattern_node.kind {
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                // `downlevelIteration` reads array elements through `__read`
                // rather than index access — a different form this lowering does
                // not model.
                if self.ctx.options.downlevel_iteration {
                    return None;
                }
                for (element_index, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                    if elem_idx.is_none() {
                        // Array hole: no binding, but the position still counts.
                        continue;
                    }
                    let elem_node = self.arena.get(elem_idx)?;
                    let elem = self.arena.get_binding_element(elem_node)?;
                    // Defaults and nested patterns are not modeled here.
                    if elem.initializer.is_some() {
                        return None;
                    }
                    let name_node = self.arena.get(elem.name)?;
                    if !name_node.is_identifier() {
                        return None;
                    }
                    let access = if elem.dot_dot_dot_token {
                        EsmDestructAccess::Slice(element_index)
                    } else {
                        EsmDestructAccess::Index(element_index)
                    };
                    bindings.push(EsmDestructBinding {
                        name: elem.name,
                        access,
                    });
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                for &elem_idx in &pattern.elements.nodes {
                    if elem_idx.is_none() {
                        continue;
                    }
                    let elem_node = self.arena.get(elem_idx)?;
                    let elem = self.arena.get_binding_element(elem_node)?;
                    // Object rest is handled by the dedicated
                    // `emit_esm_object_rest_export_statement`; defaults and nested
                    // patterns are not modeled here.
                    if elem.dot_dot_dot_token || elem.initializer.is_some() {
                        return None;
                    }
                    let name_node = self.arena.get(elem.name)?;
                    if !name_node.is_identifier() {
                        return None;
                    }
                    let key_idx = if elem.property_name.is_some() {
                        elem.property_name
                    } else {
                        elem.name
                    };
                    let key_node = self.arena.get(key_idx)?;
                    // Only plain identifier keys map to `temp.<key>`; string /
                    // numeric / computed keys are left to the existing path.
                    if key_node.kind != SyntaxKind::Identifier as u16 {
                        return None;
                    }
                    bindings.push(EsmDestructBinding {
                        name: elem.name,
                        access: EsmDestructAccess::Prop(key_idx),
                    });
                }
            }
            _ => return None,
        }

        (!bindings.is_empty()).then_some(bindings)
    }

    /// Emit `temp[index]` / `temp.slice(index)` / `temp.<key>` for one binding.
    fn emit_esm_destruct_access(&mut self, temp: &str, access: &EsmDestructAccess) {
        self.write(temp);
        match *access {
            EsmDestructAccess::Index(index) => {
                self.write("[");
                self.write_usize(index);
                self.write("]");
            }
            EsmDestructAccess::Slice(index) => {
                self.write(".slice(");
                self.write_usize(index);
                self.write(")");
            }
            EsmDestructAccess::Prop(key_idx) => {
                let key = self.get_identifier_text(key_idx);
                self.write(".");
                self.write(&key);
            }
        }
    }
}
