//! TS18057: string-literal module export names under `--module es2015`/`es2020`.
//!
//! ECMAScript arbitrary module namespace names (`export { x as "str name" }`)
//! postdate the `es2015` and `es2020` module output formats, so `tsc` rejects
//! them when `module` is set to exactly one of those two targets. Every other
//! module target — `es2022`, `esnext`, `commonjs`, `preserve` and the `node*`
//! family — accepts them.
//!
//! `tsc` centralises this in `checkModuleExportName`, which runs over every
//! *module export name* position: the property name of an import specifier,
//! both halves of an export specifier, and the name of a namespace export
//! (`export * as "ns" from "m"`). It reports through `grammarErrorOnNode`, so
//! the whole check is suppressed once the file has parse diagnostics.
//!
//! One asymmetry is load-bearing and oracle-confirmed. `checkImportDeclaration`
//! only walks its specifiers when the module specifier *resolves*, while the
//! export paths do not, so an unresolved module suppresses TS18057 on the
//! import side but not on the export side:
//!
//! ```text
//! import { "x" as y } from "./nope";   // TS2307 only
//! export { x as "a" }  from "./nope";  // TS18057 + TS2307
//! ```

use crate::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Shared gate: the check is module-target-specific and, like every other
    /// check-time grammar diagnostic, is suppressed once the file has a real
    /// parse error (`tsc`'s `grammarErrorOnNode` does the same).
    const fn should_check_module_export_names(&self) -> bool {
        !self.ctx.has_parse_errors
            && matches!(
                self.ctx.compiler_options.module,
                ModuleKind::ES2015 | ModuleKind::ES2020
            )
    }

    /// Report TS18057 on `name_idx` when it is written as a string literal.
    ///
    /// Mirrors `tsc`'s `checkModuleExportName` for the `allowStringLiteral`
    /// case. The `!allowStringLiteral` case — a local binding that cannot be
    /// named by a string, as in `import { "a" as "b" }` or
    /// `export { "a" as b }` with no module specifier — is answered by the
    /// parser as TS1003 and is mutually exclusive with this code, so it is
    /// reached only through the `has_parse_errors` gate above.
    fn check_module_export_name(&mut self, name_idx: NodeIndex) {
        if name_idx.is_none() {
            return;
        }
        let is_string_literal = self
            .ctx
            .arena
            .get(name_idx)
            .is_some_and(|n| n.kind == SyntaxKind::StringLiteral as u16);
        if !is_string_literal {
            return;
        }
        self.error_at_node(
            name_idx,
            crate::diagnostics::diagnostic_messages::STRING_LITERAL_IMPORT_AND_EXPORT_NAMES_ARE_NOT_SUPPORTED_WHEN_THE_MODULE_FLAG_IS,
            crate::diagnostics::diagnostic_codes::STRING_LITERAL_IMPORT_AND_EXPORT_NAMES_ARE_NOT_SUPPORTED_WHEN_THE_MODULE_FLAG_IS,
        );
    }

    /// Whether `tsc` would consider this module specifier resolved, which is
    /// what gates `checkImportDeclaration`'s walk over its specifiers. A file
    /// on disk and an ambient `declare module "..."` both count.
    fn module_specifier_resolves(&self, module_specifier: NodeIndex) -> bool {
        let Some(specifier_node) = self.ctx.arena.get(module_specifier) else {
            return false;
        };
        let Some(literal) = self.ctx.arena.get_literal(specifier_node) else {
            return false;
        };
        let module_name = literal.text.clone();
        self.ctx.resolve_import_target(&module_name).is_some()
            || self
                .ctx
                .declared_modules_contains(self.ctx.binder, &module_name)
    }

    /// TS18057 for the module export names introduced by an import declaration.
    ///
    /// Only the *property name* of an import specifier is a module export
    /// name; the specifier's own name binds a local and must already be an
    /// identifier.
    pub(crate) fn check_import_declaration_module_export_names(&mut self, import_idx: NodeIndex) {
        if !self.should_check_module_export_names() {
            return;
        }
        let Some(import_node) = self.ctx.arena.get(import_idx) else {
            return;
        };
        let Some(import_decl) = self.ctx.arena.get_import_decl(import_node) else {
            return;
        };
        if !self.module_specifier_resolves(import_decl.module_specifier) {
            return;
        }
        let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };
        let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
            return;
        };
        if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
            return;
        }
        let Some(named_imports) = self.ctx.arena.get_named_imports(bindings_node) else {
            return;
        };
        let property_names: Vec<NodeIndex> = named_imports
            .elements
            .nodes
            .iter()
            .filter_map(|element_idx| {
                let element_node = self.ctx.arena.get(*element_idx)?;
                let specifier = self.ctx.arena.get_specifier(element_node)?;
                Some(specifier.property_name)
            })
            .collect();
        for property_name in property_names {
            self.check_module_export_name(property_name);
        }
    }

    /// TS18057 for the module export names introduced by an export declaration.
    ///
    /// Both halves of an export specifier are module export names — the
    /// property name is the re-exported source name and the name is the
    /// exported name — and either may be a string literal, so
    /// `export { "a" as "b" } from "m"` draws two diagnostics in source order.
    ///
    /// tsz does not build a distinct `NAMESPACE_EXPORT` node: `parse_export_star`
    /// stores the `export * as <name>` name directly in `export_clause`. So any
    /// export clause that is not `NAMED_EXPORTS` is itself the namespace name.
    pub(crate) fn check_export_declaration_module_export_names(&mut self, export_idx: NodeIndex) {
        if !self.should_check_module_export_names() {
            return;
        }
        let Some(export_node) = self.ctx.arena.get(export_idx) else {
            return;
        };
        let Some(export_decl) = self.ctx.arena.get_export_decl(export_node) else {
            return;
        };
        let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
            return;
        };
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            // `export * as "ns" from "m"` — the clause *is* the namespace name.
            self.check_module_export_name(export_decl.export_clause);
            return;
        }
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };
        // `allowStringLiteral` in tsc's `checkExportSpecifier` is `!!moduleSpecifier`
        // for the property name: without a `from` clause the property name is a
        // *local* binding, which no string can name, so tsc answers TS1003 there
        // and never TS18057.
        let property_names_are_module_export_names = export_decl.module_specifier.is_some();
        let mut names = Vec::with_capacity(named_exports.elements.nodes.len() * 2);
        for element_idx in &named_exports.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(*element_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(element_node) else {
                continue;
            };
            if property_names_are_module_export_names {
                names.push(specifier.property_name);
            }
            names.push(specifier.name);
        }
        for name_idx in names {
            self.check_module_export_name(name_idx);
        }
    }
}
