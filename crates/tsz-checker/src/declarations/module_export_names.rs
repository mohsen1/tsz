//! Module export names written as string literals: TS1003 and TS18057.
//!
//! `tsc` centralises both codes in a single `checkModuleExportName`, which runs
//! over every *module export name* position: the property name of an import
//! specifier, both halves of an export specifier, and the name of a namespace
//! export (`export * as "ns" from "m"`). It reports through `grammarErrorOnNode`,
//! so the whole check is suppressed once the file has parse diagnostics.
//!
//! ```ts
//! function checkModuleExportName(name, allowStringLiteral = true) {
//!     if (name === undefined || name.kind !== SyntaxKind.StringLiteral) return;
//!     if (!allowStringLiteral) grammarErrorOnNode(name, Identifier_expected);
//!     else if (moduleKind === ES2015 || moduleKind === ES2020) grammarErrorOnNode(name, ...);
//! }
//! ```
//!
//! The two branches are **mutually exclusive per position and answer to
//! different conditions**, which is the whole structural point of this module:
//!
//! * `!allowStringLiteral` (TS1003) means *this position binds a local*, and no
//!   string can name a local. It is **module-target independent**.
//! * `allowStringLiteral` (TS18057) means the position really is a module export
//!   name, and ECMAScript arbitrary module namespace names postdate the `es2015`
//!   and `es2020` output formats, so `tsc` rejects them on exactly those two
//!   targets and accepts them everywhere else (`es2022`, `esnext`, `commonjs`,
//!   `preserve`, the `node*` family).
//!
//! `allowStringLiteral` is `false` at exactly one position: an export
//! specifier's property name when the export declaration has **no module
//! specifier**, per `checkExportSpecifier`'s
//! `checkModuleExportName(node.propertyName, hasModuleSpecifier)`. Because the
//! branches are chosen per position rather than per declaration, one specifier
//! can draw one of each — oracle-confirmed under `--module es2015`:
//!
//! ```text
//! const q = 1; export { "q" as "y" };
//!                       ^^^ TS1003 (property name binds a local)
//!                              ^^^ TS18057 (exported name, es2015 target)
//! ```
//!
//! A second asymmetry is load-bearing and oracle-confirmed.
//! `checkImportDeclaration` only walks its specifiers when the module specifier
//! *resolves*, while the export paths do not, so an unresolved module suppresses
//! TS18057 on the import side but not on the export side:
//!
//! ```text
//! import { "x" as y } from "./nope";   // TS2307 only
//! export { x as "a" }  from "./nope";  // TS18057 + TS2307
//! ```

use crate::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::node::ExportDeclData;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Shared gate: like every other check-time grammar diagnostic, both codes
    /// are suppressed once the file has a real parse error (`tsc`'s
    /// `grammarErrorOnNode` does the same).
    const fn should_check_module_export_names(&self) -> bool {
        !self.ctx.has_parse_errors
    }

    /// Whether the current module target is one of the two that reject
    /// arbitrary module namespace names. This is the condition on
    /// `checkModuleExportName`'s *second* branch only.
    const fn module_target_rejects_string_export_names(&self) -> bool {
        matches!(
            self.ctx.compiler_options.module,
            ModuleKind::ES2015 | ModuleKind::ES2020
        )
    }

    /// `tsc`'s `checkModuleExportName`, both branches.
    ///
    /// `allow_string_literal` is `false` only where the position binds a local
    /// rather than naming a module export — an export specifier's property name
    /// with no module specifier. There a string literal is not a module export
    /// name at all but a malformed binding identifier, so the answer is TS1003
    /// regardless of module target, and TS18057 is never reached.
    fn check_module_export_name(&mut self, name_idx: NodeIndex, allow_string_literal: bool) {
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
        if !allow_string_literal {
            self.error_at_node(
                name_idx,
                crate::diagnostics::diagnostic_messages::IDENTIFIER_EXPECTED,
                crate::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
            );
        } else if self.module_target_rejects_string_export_names() {
            self.error_at_node(
                name_idx,
                crate::diagnostics::diagnostic_messages::STRING_LITERAL_IMPORT_AND_EXPORT_NAMES_ARE_NOT_SUPPORTED_WHEN_THE_MODULE_FLAG_IS,
                crate::diagnostics::diagnostic_codes::STRING_LITERAL_IMPORT_AND_EXPORT_NAMES_ARE_NOT_SUPPORTED_WHEN_THE_MODULE_FLAG_IS,
            );
        }
    }

    /// Whether an export declaration's `export_clause` holds a *namespace
    /// export name* — the `<name>` of `export * as <name> from "m"` — rather
    /// than some other node that merely shares the field.
    ///
    /// tsz has no distinct `NamespaceExport` node, so `ExportDeclData::export_clause`
    /// is reused by five unrelated productions: the namespace name, the
    /// `NAMED_EXPORTS` clause, a **default-export expression**, an
    /// `export import X = Y` declaration, and the declaration of an
    /// `export <declaration>`. Only the first is a module export name in `tsc`'s
    /// sense, so only the first may reach `checkModuleExportName`; treating the
    /// others as names is how `export default "./foo"` came to draw TS18057
    /// while `tsc` is silent.
    ///
    /// `parse_export_star` is the sole producer of a namespace name and always
    /// parses a `from` clause, so a non-default export declaration carrying a
    /// module specifier selects exactly that production. The other four
    /// productions all leave `module_specifier` empty or set `is_default_export`.
    const fn export_clause_is_namespace_export_name(export_decl: &ExportDeclData) -> bool {
        !export_decl.is_default_export && export_decl.module_specifier.is_some()
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
        // Every module export name reachable from an import declaration is
        // `allowStringLiteral = true`, so TS18057 is the only branch this path
        // can take. Skipping the whole walk when the module target does not
        // reject string export names is therefore exactly equivalent to running
        // it, and avoids resolving a module specifier for every import in the
        // program on targets where nothing could be reported.
        if !self.should_check_module_export_names()
            || !self.module_target_rejects_string_export_names()
        {
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
            // `checkImportBinding` passes no second argument: an import
            // specifier's property name is always a module export name.
            self.check_module_export_name(property_name, true);
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
            if !Self::export_clause_is_namespace_export_name(export_decl) {
                return;
            }
            // `export * as "ns" from "m"` — the clause *is* the namespace name.
            // `checkExportDeclaration` passes no second argument, so this
            // position always allows a string literal.
            self.check_module_export_name(export_decl.export_clause, true);
            return;
        }
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };
        // `allowStringLiteral` in tsc's `checkExportSpecifier` is `!!moduleSpecifier`
        // for the property name: without a `from` clause the property name is a
        // *local* binding, which no string can name, so tsc answers TS1003 there
        // and never TS18057. The specifier's own name is always a module export
        // name and always allows a string literal.
        let property_names_are_module_export_names = export_decl.module_specifier.is_some();
        let mut names = Vec::with_capacity(named_exports.elements.nodes.len() * 2);
        for element_idx in &named_exports.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(*element_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(element_node) else {
                continue;
            };
            // Source order within a specifier: property name, then name — the
            // order `checkExportSpecifier` calls them in.
            names.push((
                specifier.property_name,
                property_names_are_module_export_names,
            ));
            names.push((specifier.name, true));
        }
        for (name_idx, allow_string_literal) in names {
            self.check_module_export_name(name_idx, allow_string_literal);
        }
    }
}
