use super::super::{ModuleKind, Printer, get_operator_text};
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

/// Classification of a local name with respect to CJS live exports.
enum CjsLiveExportKind {
    /// Not a live CJS export (not exported, shadowed, or not CommonJS).
    NotExported,
    /// `export let/const/var x` — all reads rewrite to `exports.x`.
    /// Carries any additional clause aliases (e.g. `export { x as foo }` alongside
    /// `export let x`) that also need to be kept in sync on each mutation.
    Inline(Vec<String>),
    /// `export { x as foo }` only — local `x` lives alongside `exports.foo`.
    Clause(Vec<String>),
}

impl<'a> Printer<'a> {
    /// Not `is_effectively_commonjs()`: that helper also returns true for AMD/UMD/System
    /// wrapper bodies, which use a different export protocol and must not receive
    /// clause-export live binding rewrites (`exports.x = ...`).
    const fn is_commonjs_live_export_context(&self) -> bool {
        if self.is_system_live_export_context() {
            return false;
        }
        // Check the mask field explicitly: with_cjs_export_body_mask() temporarily
        // sets options.module = None while emitting hoisted function bodies, so
        // is_commonjs() alone misses that window. is_effectively_commonjs() is
        // intentionally not used here: it also returns true for AMD/UMD/System
        // wrappers, which are not CJS live-export contexts.
        self.ctx.is_commonjs()
            || matches!(self.ctx.original_module_kind, Some(ModuleKind::CommonJS))
            || matches!(
                self.ctx.cjs_export_body_outer_module,
                Some(ModuleKind::CommonJS)
            )
    }

    /// Write `exports.name` or `exports["name"]` depending on whether the name
    /// is a valid JS identifier. Does NOT write ` = `.
    pub(in crate::emitter) fn write_export_property_access(&mut self, export_name: &str) {
        if super::super::is_valid_identifier_name(export_name) {
            self.write("exports.");
            self.write(export_name);
        } else {
            self.write("exports[\"");
            self.write(export_name);
            self.write("\"]");
        }
    }

    /// Emit the assignment target for a CommonJS-exported local when the target
    /// must update live named exports. Returns `true` when it handled the target.
    pub(in crate::emitter) fn emit_commonjs_live_export_assignment_target(
        &mut self,
        target_idx: NodeIndex,
    ) -> bool {
        let Some(target_node) = self.arena.get(target_idx) else {
            return false;
        };
        if target_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(ident) = self.arena.get_identifier(target_node) else {
            return false;
        };
        let local_name = ident.escaped_text.clone();
        self.emit_commonjs_live_export_assignment_target_name(&local_name)
    }

    pub(in crate::emitter) fn emit_commonjs_live_export_assignment_target_name(
        &mut self,
        local_name: &str,
    ) -> bool {
        let (inline_export, export_names) = match self.cjs_live_export_kind(local_name) {
            CjsLiveExportKind::NotExported => return false,
            CjsLiveExportKind::Inline(aliases) => (true, aliases),
            CjsLiveExportKind::Clause(names) => (false, names),
        };

        let mut written_exports: Vec<String> = Vec::new();
        for export_name in export_names.into_iter().rev() {
            if inline_export && export_name == local_name {
                continue;
            }
            if written_exports.contains(&export_name) {
                continue;
            }
            self.write_export_property_access(&export_name);
            self.write(" = ");
            written_exports.push(export_name);
        }

        if inline_export {
            self.write_export_property_access(local_name);
        } else {
            self.write_identifier(local_name);
        }
        true
    }

    pub(in crate::emitter) fn commonjs_live_export_assignment_target_name_needs_chain(
        &self,
        local_name: &str,
    ) -> bool {
        if local_name.is_empty() || !self.is_commonjs_live_export_context() {
            return false;
        }
        let is_shadowed = self
            .commonjs_exported_var_shadow_stack
            .iter()
            .rev()
            .any(|scope| scope.contains(local_name));
        if !is_shadowed && self.commonjs_exported_var_names.contains(local_name) {
            return true;
        }
        self.deferred_local_export_bindings_all
            .as_ref()
            .and_then(|b| b.get(local_name))
            .is_some_and(|n| !n.is_empty())
            || self
                .deferred_local_export_bindings
                .as_ref()
                .is_some_and(|b| b.contains_key(local_name))
    }

    fn cjs_live_export_kind(&self, local_name: &str) -> CjsLiveExportKind {
        if local_name.is_empty() || !self.is_commonjs_live_export_context() {
            return CjsLiveExportKind::NotExported;
        }
        let is_shadowed = self
            .commonjs_exported_var_shadow_stack
            .iter()
            .rev()
            .any(|scope| scope.contains(local_name));
        let is_inline = !is_shadowed && self.commonjs_exported_var_names.contains(local_name);
        // Always collect clause aliases: a name can be both inline-exported
        // (`export let x`) AND carry additional clause aliases (`export { x as foo }`),
        // and both forms must be updated on every mutation.
        let mut names: Vec<String> = self
            .deferred_local_export_bindings_all
            .as_ref()
            .and_then(|b| b.get(local_name))
            .cloned()
            .unwrap_or_default();
        if names.is_empty() {
            if let Some(name) = self
                .deferred_local_export_bindings
                .as_ref()
                .and_then(|b| b.get(local_name))
            {
                names.push(name.clone());
            }
        }
        if is_inline {
            CjsLiveExportKind::Inline(names)
        } else if !names.is_empty() {
            CjsLiveExportKind::Clause(names)
        } else {
            CjsLiveExportKind::NotExported
        }
    }

    /// Emit `++x` / `--x` on a live CJS export, threading the update through
    /// `exports.*`. Returns `true` when it handled the output.
    pub(in crate::emitter) fn emit_cjs_live_export_prefix_unary(
        &mut self,
        local_name: &str,
        operator: u16,
    ) -> bool {
        let needs_parens = !self.ctx.flags.in_statement_expression;
        match self.cjs_live_export_kind(local_name) {
            CjsLiveExportKind::NotExported => false,
            CjsLiveExportKind::Inline(aliases) => {
                if needs_parens {
                    self.write("(");
                }
                self.write_export_property_chain(&aliases);
                self.write(get_operator_text(operator));
                self.write_export_property_access(local_name);
                if needs_parens {
                    self.write(")");
                }
                true
            }
            CjsLiveExportKind::Clause(names) => {
                if needs_parens {
                    self.write("(");
                }
                self.write_export_property_chain(&names);
                self.write(get_operator_text(operator));
                self.write_identifier(local_name);
                if needs_parens {
                    self.write(")");
                }
                true
            }
        }
    }

    /// Emit `x++` / `x--` on a live CJS export, threading the update through
    /// `exports.*`. `is_statement` is true when the result value is discarded;
    /// false when the pre-update value must be returned (expression context).
    /// Returns `true` when it handled the output.
    pub(in crate::emitter) fn emit_cjs_live_export_postfix_unary(
        &mut self,
        local_name: &str,
        operator: u16,
        is_statement: bool,
    ) -> bool {
        match self.cjs_live_export_kind(local_name) {
            CjsLiveExportKind::NotExported => false,
            CjsLiveExportKind::Inline(aliases) => {
                if is_statement {
                    if aliases.is_empty() {
                        self.write_export_property_access(local_name);
                        self.write(get_operator_text(operator));
                    } else {
                        self.write_export_property_chain(&aliases);
                        self.write("(");
                        self.write_export_property_access(local_name);
                        self.write(get_operator_text(operator));
                        self.write(", ");
                        self.write_export_property_access(local_name);
                        self.write(")");
                    }
                    return true;
                }
                if aliases.is_empty() {
                    self.write_export_property_access(local_name);
                    self.write(get_operator_text(operator));
                    return true;
                }
                // Expression context with clause aliases: pre-update value must be returned.
                let temp = self.make_unique_name_file_hoisted();
                self.write("(");
                self.write_export_property_chain(&aliases);
                self.write("(");
                self.write(&temp);
                self.write(" = ");
                self.write_export_property_access(local_name);
                self.write(get_operator_text(operator));
                self.write(", ");
                self.write_export_property_access(local_name);
                self.write("), ");
                self.write(&temp);
                self.write(")");
                true
            }
            CjsLiveExportKind::Clause(names) => {
                if is_statement {
                    // `x++` returns the pre-update value, so `exports.foo = x++` would
                    // capture the stale value. The comma form `(x++, x)` sequences the
                    // increment first and reads back the updated local for the export.
                    self.write_export_property_chain(&names);
                    self.write("(");
                    self.write_identifier(local_name);
                    self.write(get_operator_text(operator));
                    self.write(", ");
                    self.write_identifier(local_name);
                    self.write(")");
                    return true;
                }
                // The pre-update value must survive: wrap in a comma sequence
                // that captures it in a hoisted temp before the export update.
                let temp = self.make_unique_name_file_hoisted();
                self.write("(");
                self.write_export_property_chain(&names);
                self.write("(");
                self.write(&temp);
                self.write(" = ");
                self.write_identifier(local_name);
                self.write(get_operator_text(operator));
                self.write(", ");
                self.write_identifier(local_name);
                self.write("), ");
                self.write(&temp);
                self.write(")");
                true
            }
        }
    }

    /// Write `exports.A = exports.B = ` for each name in `names` (in reverse
    /// order), so callers can chain the final value assignment through all aliases.
    fn write_export_property_chain(&mut self, names: &[String]) {
        for name in names.iter().rev() {
            self.write_export_property_access(name);
            self.write(" = ");
        }
    }
}
