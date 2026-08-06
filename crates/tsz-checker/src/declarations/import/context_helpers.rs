//! AST context-checking utilities for import/export validation.
//!
//! Functions that walk the parent chain to determine context
//! (namespace, function body, module augmentation, etc.).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Check if a statement has an export modifier.
    pub(crate) fn has_export_modifier(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        let Some(mods) = self.get_declaration_modifiers(node) else {
            return false;
        };

        self.ctx
            .arena
            .has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
    }

    /// Check whether a node is nested inside a namespace declaration.
    /// String-literal ambient modules (`declare module "x"`) are excluded.
    pub(crate) fn is_inside_namespace_declaration(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;

        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }

            let Some(module_decl) = self.ctx.arena.get_module(node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(module_decl.name) else {
                continue;
            };

            if name_node.kind != SyntaxKind::StringLiteral as u16 {
                return true;
            }
        }

        false
    }

    /// Check if a node is NOT in a valid module-element context (`SourceFile` or `ModuleBlock`).
    /// Returns true when the node is inside a block, function body, or other non-module context.
    pub(crate) fn is_in_non_module_element_context(&self, node_idx: NodeIndex) -> bool {
        let parent_idx = self.ctx.arena.parent_of(node_idx);
        let parent_kind = parent_idx
            .and_then(|p| self.ctx.arena.get(p))
            .map(|p| p.kind);

        // For import-equals inside `export import X = N;`, the direct parent is
        // EXPORT_DECLARATION. Look through it to the grandparent.
        let effective_parent_kind = if matches!(parent_kind, Some(k) if k == syntax_kind_ext::EXPORT_DECLARATION)
        {
            parent_idx
                .and_then(|p| self.ctx.arena.get_extended(p))
                .and_then(|ext| self.ctx.arena.get(ext.parent))
                .map(|p| p.kind)
        } else {
            parent_kind
        };

        match effective_parent_kind {
            Some(k) if k == syntax_kind_ext::SOURCE_FILE || k == syntax_kind_ext::MODULE_BLOCK => {
                false
            }
            None => false, // Top-level
            _ => true,
        }
    }

    /// Check if a node is inside a function/method body.
    /// Walks up the parent chain to find a function-like ancestor.
    ///
    /// A class `static { }` block is function-like for this purpose: tsc's
    /// binder gives it its own `container` cursor, same as a function body,
    /// and it never resolves a position-invalid `import`/`import =` module
    /// specifier inside one (#16450).
    pub(crate) fn is_inside_function_body(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            match node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION =>
                {
                    return true;
                }
                k if k == syntax_kind_ext::SOURCE_FILE || k == syntax_kind_ext::MODULE_BLOCK => {
                    return false;
                }
                _ => continue,
            }
        }
        false
    }

    /// Whether the nearest enclosing function-like body is one tsc revisits through
    /// its *deferred* queue instead of the eager statement walk.
    ///
    /// `checkExportAssignment` opens with `checkGrammarModuleElementContext` and
    /// `return`s the moment the context is invalid, so a position-invalid
    /// `export default [ expr ]` never resolves its exported expression — the
    /// placement diagnostic is the whole answer. The exception is a body tsc reaches
    /// a second time through `checkFunctionExpressionOrObjectLiteralMethodDeferred`:
    /// a function expression, an arrow function, or an object-literal method. Inside
    /// one of those the expression *is* resolved, and an unresolved name still
    /// reports.
    ///
    /// The object-literal **accessor** is the row that pins this to the deferred set
    /// rather than to "is inside some function": tsc does not defer accessors
    /// (`checkAccessorDeclaration` runs eagerly from `checkObjectLiteral`), and an
    /// object-literal getter suppresses exactly like a class method does, while the
    /// object-literal method one line away does not.
    ///
    /// Only meaningful for a node in a non-module-element context.
    pub(crate) fn nearest_function_like_body_is_deferred_checked(
        &self,
        node_idx: NodeIndex,
    ) -> bool {
        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            match node.kind {
                k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    return true;
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    // An object-literal method is deferred; a class method is not.
                    return self
                        .ctx
                        .arena
                        .parent_of(current)
                        .and_then(|p| self.ctx.arena.get(p))
                        .is_some_and(|p| p.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION);
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION =>
                {
                    return false;
                }
                k if k == syntax_kind_ext::SOURCE_FILE || k == syntax_kind_ext::MODULE_BLOCK => {
                    return false;
                }
                _ => continue,
            }
        }
        false
    }

    /// Whether `node_idx` is, or is the exported expression of, an
    /// `export default [ expr ]` whose placement diagnostic is the whole answer.
    ///
    /// tsc never types the expression of such a declaration, so no walker may
    /// demand it. tsz has two demand sites: `build_type_environment`, which types
    /// every file symbol up front, and the statement walk in
    /// `check_export_declaration`. Both consult this.
    pub(crate) fn is_unchecked_position_invalid_default_export(&self, node_idx: NodeIndex) -> bool {
        if node_idx.is_none() {
            return false;
        }
        let export_idx = if self
            .ctx
            .arena
            .kind_at(node_idx)
            .is_some_and(|k| k == syntax_kind_ext::EXPORT_DECLARATION)
        {
            node_idx
        } else {
            match self.ctx.arena.parent_of(node_idx) {
                Some(parent)
                    if self
                        .ctx
                        .arena
                        .kind_at(parent)
                        .is_some_and(|k| k == syntax_kind_ext::EXPORT_DECLARATION) =>
                {
                    parent
                }
                _ => return false,
            }
        };

        let Some(export_decl) = self
            .ctx
            .arena
            .get(export_idx)
            .and_then(|node| self.ctx.arena.get_export_decl(node))
        else {
            return false;
        };
        if !export_decl.is_default_export || export_decl.export_clause.is_none() {
            return false;
        }
        let clause_idx = export_decl.export_clause;

        self.is_in_non_module_element_context(export_idx)
            && self.export_default_clause_is_expression(clause_idx)
            && !self.nearest_function_like_body_is_deferred_checked(export_idx)
    }

    /// Whether an `export default` wraps a bare expression rather than a declaration.
    ///
    /// tsc parses `export default class C {}` / `export default function f() {}` in a
    /// statement position as the declaration itself carrying an illegal `export`
    /// modifier (TS1184, `checkGrammarModifiers`), not as an `ExportAssignment`, so
    /// `checkExportAssignment`'s bail never applies and the declaration is checked
    /// normally. Only the expression form reaches TS1258 and the bail.
    pub(crate) fn export_default_clause_is_expression(&self, clause_idx: NodeIndex) -> bool {
        !self.ctx.arena.kind_at(clause_idx).is_some_and(|k| {
            k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::VARIABLE_STATEMENT
                || k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION
                || k == syntax_kind_ext::MODULE_DECLARATION
        })
    }

    /// Whether a module element that has *already* drawn a placement diagnostic
    /// still resolves its module specifier.
    ///
    /// Only meaningful for a node in a non-module-element context — i.e. one for
    /// which [`Self::is_in_non_module_element_context`] is true. A declaration in a
    /// valid context (`SourceFile`, or the `ModuleBlock` of an ambient module)
    /// always resolves and must not consult this.
    ///
    /// tsc's `checkExportDeclaration` reports the placement diagnostic and then
    /// `return`s, so `resolveExternalModuleName` — the only TS2307/TS2305 site — is
    /// never reached. That return is reached only when a *declaration scope*
    /// encloses the declaration: a function-like body, or a namespace/ambient-module
    /// body it does not directly belong to. A container that opens no declaration
    /// scope — a bare block, an `if`/loop/`try` body, a labeled statement, a
    /// `switch` clause — leaves the declaration in the source file's own scope, and
    /// resolution still runs there.
    ///
    /// The walk therefore stops at the first scope-opening ancestor and answers
    /// from its kind, rather than testing for any single container shape.
    ///
    /// One measured exception is deliberately not encoded: a block inside
    /// `declare global { }` keeps resolving, because a global augmentation re-opens
    /// the global scope rather than introducing one. tsz reports no diagnostic at
    /// all for that shape today, so the branch would be unreachable and untestable;
    /// it is recorded here instead of written as dead code.
    pub(crate) fn position_invalid_module_element_resolves_specifier(
        &self,
        node_idx: NodeIndex,
    ) -> bool {
        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            match node.kind {
                // Function-like ancestors are the same set `is_inside_function_body`
                // walks, including a class `static { }` block (#16450).
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION =>
                {
                    return false;
                }
                // Reaching a `ModuleBlock` from a position-invalid node means the
                // node is nested *inside* a namespace/module body rather than being
                // one of its own elements, so the body is a scope it sits within.
                k if k == syntax_kind_ext::MODULE_BLOCK => return false,
                k if k == syntax_kind_ext::SOURCE_FILE => return true,
                _ => continue,
            }
        }
        true
    }

    /// Whether a position-invalid `import`/`import =` still resolves its module
    /// specifier — the import-side companion to
    /// [`Self::position_invalid_export_declaration_resolves_specifier`], refining
    /// [`Self::position_invalid_module_element_resolves_specifier`] with the
    /// import side's own demand model.
    ///
    /// Once the scope walk establishes that no declaration scope encloses the
    /// element (so tsc's `checkImportDeclaration` does not `return` at the
    /// placement diagnostic), tsc reaches `resolveExternalModuleName` under
    /// `markAliasReferenced`: an import binding's specifier is resolved when the
    /// binding is **used**, wherever it sits (#16411 established this for
    /// `import =`; it holds for every import binding, so a used binding in a
    /// function body resolves too). The one fact a use cannot express is that a
    /// **script** additionally resolves its bound-but-unused top-level-block
    /// imports, because a script has no module import table gating that pass. A
    /// side-effect `import "m"` binds nothing, so nothing ever marks it
    /// referenced; it resolves only at a valid position, never here.
    ///
    /// Measured against the pinned oracle across every import production in a
    /// top-level bare block / `if` body / loop body (#16505):
    ///
    /// | production                          | script   | module   |
    /// | ----------------------------------- | -------- | -------- |
    /// | `import`/`import =` (bound & used)   | resolve  | resolve  |
    /// | `import`/`import =` (bound, unused)  | resolve  | suppress |
    /// | `import "m"` (side-effect, no bind)  | suppress | suppress |
    ///
    /// The export side inverts on the `export *` row and is handled separately by
    /// [`Self::position_invalid_export_declaration_resolves_specifier`]; the two
    /// rules cannot be shared.
    ///
    /// Only meaningful for a node in a non-module-element context; callers guard
    /// with [`Self::is_in_non_module_element_context`].
    pub(crate) fn position_invalid_import_resolves_specifier(&self, node_idx: NodeIndex) -> bool {
        // A side-effect import binds no name, so nothing triggers its specifier
        // resolution here.
        if !self.import_element_binds_a_name(node_idx) {
            return false;
        }
        let top_level_block = self.position_invalid_module_element_resolves_specifier(node_idx);
        let is_module = self.ctx.is_external_module_file();
        (top_level_block && !is_module) || self.import_element_alias_is_referenced(node_idx)
    }

    /// Whether an import statement introduces a local binding: an `import =`
    /// always does; a regular `import` does unless it is a bare side-effect
    /// `import "m"` with no import clause.
    fn import_element_binds_a_name(&self, node_idx: NodeIndex) -> bool {
        match self.ctx.arena.kind_at(node_idx) {
            Some(k) if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => true,
            Some(k) if k == syntax_kind_ext::IMPORT_DECLARATION => self
                .ctx
                .arena
                .get(node_idx)
                .and_then(|node| self.ctx.arena.get_import_decl(node))
                .is_some_and(|import| import.import_clause.is_some()),
            _ => false,
        }
    }

    /// The local symbols an import (or `import =`) declaration binds into its
    /// enclosing scope: the `import =` alias, or a regular import's default,
    /// namespace and named locals. A side-effect `import "m"` yields none.
    fn import_binding_symbols(&self, node_idx: NodeIndex) -> Vec<tsz_binder::SymbolId> {
        let mut symbols = Vec::new();
        if self
            .ctx
            .arena
            .kind_at(node_idx)
            .is_some_and(|k| k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        {
            // `import x = require(...)` / `import x = N` hangs its alias symbol
            // on the statement node itself.
            if let Some(sym) = self.ctx.binder.get_node_symbol(node_idx) {
                symbols.push(sym);
            }
            return symbols;
        }

        let Some(import) = self
            .ctx
            .arena
            .get(node_idx)
            .and_then(|node| self.ctx.arena.get_import_decl(node))
        else {
            return symbols;
        };
        if import.import_clause.is_none() {
            return symbols; // side-effect import
        }
        // Every binding identifier under the import clause carries a local
        // symbol; walk the clause subtree and collect them.
        let mut stack: Vec<NodeIndex> = vec![import.import_clause];
        while let Some(current) = stack.pop() {
            if let Some(sym) = self.ctx.binder.get_node_symbol(current) {
                symbols.push(sym);
            }
            stack.extend(self.ctx.arena.get_children(current));
        }
        symbols
    }

    /// Whether any binding of the import (or `import =`) at `node_idx` is
    /// referenced anywhere in its source file — tsc's `markAliasReferenced`
    /// discriminator. A side-effect import (no binding) is never referenced.
    ///
    /// The scan runs from the `SourceFile` rather than from the enclosing block
    /// because the test is symbol identity: only an identifier that resolves to
    /// one of this import's binding symbols counts, so widening the walk cannot
    /// produce a false positive and it does catch a use from a nested closure.
    fn import_element_alias_is_referenced(&self, node_idx: NodeIndex) -> bool {
        let symbols = self.import_binding_symbols(node_idx);
        if symbols.is_empty() {
            return false;
        }

        let mut scan_root = node_idx;
        loop {
            if self
                .ctx
                .arena
                .get(scan_root)
                .is_some_and(|node| node.kind == syntax_kind_ext::SOURCE_FILE)
            {
                break;
            }
            let Some(ext) = self.ctx.arena.get_extended(scan_root) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            scan_root = ext.parent;
        }

        let mut stack: Vec<NodeIndex> = self.ctx.arena.get_children(scan_root);
        while let Some(current) = stack.pop() {
            // The declaration's own subtree is the binding site, not a use.
            if current == node_idx {
                continue;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };
            if node.kind == SyntaxKind::Identifier as u16
                && self
                    .resolve_identifier_symbol(current)
                    .is_some_and(|sym| symbols.contains(&sym))
            {
                return true;
            }
            stack.extend(self.ctx.arena.get_children(current));
        }
        false
    }

    /// Whether a position-invalid `export ... from "m"` still resolves its module
    /// specifier, refining [`Self::position_invalid_module_element_resolves_specifier`]
    /// with the export side's own demand model.
    ///
    /// [`Self::position_invalid_module_element_resolves_specifier`] answers whether a
    /// *declaration scope* encloses the declaration. When one does, nothing resolves
    /// and that answer is final. When one does not — a top-level bare block, an
    /// `if`/loop/`try` body, a labeled statement, a `switch` clause — the declaration
    /// is left in the source file's own scope, and the export side then diverges from
    /// the import side: `checkExportDeclaration` has already returned at the placement
    /// diagnostic, so whatever resolution still happens comes from a *later* pass over
    /// whichever symbol table the binder put the declaration in, and which table that
    /// is depends on the file.
    ///
    /// In an external module the file symbol carries an export table, so `export *`
    /// binds as an export-star entry that computing that table resolves eagerly, while
    /// a named or namespace export clause binds an alias resolved only on reference —
    /// and a position-invalid one is never referenced. In a file that is *not* a
    /// module there is no export table to compute, so the export-star is never
    /// resolved at all, and only the individual specifiers of a named clause bind as
    /// ordinary aliases that a later pass reaches.
    ///
    /// The two forms therefore swap roles across that one axis, measured against the
    /// pinned oracle (#16495):
    ///
    /// | clause | external module | not a module |
    /// | --- | --- | --- |
    /// | `export * from "m"` | resolves | silent |
    /// | `export * as ns from "m"` | silent | silent |
    /// | `export { a } from "m"` | silent | resolves |
    ///
    /// Only the export side consults this. `import`/`import =` keep resolving in every
    /// one of these containers, which is why the rule cannot be shared with them.
    pub(crate) fn position_invalid_export_declaration_resolves_specifier(
        &self,
        export_idx: NodeIndex,
        export_clause: NodeIndex,
    ) -> bool {
        if !self.position_invalid_module_element_resolves_specifier(export_idx) {
            return false;
        }

        if export_clause.is_none() {
            // Bare `export * from "m"`: an export-star entry, resolved only when the
            // file actually has a module export table to compute.
            return self.ctx.is_external_module_file();
        }

        let is_named_exports = self
            .ctx
            .arena
            .get(export_clause)
            .is_some_and(|n| n.kind == syntax_kind_ext::NAMED_EXPORTS);

        // A named clause's specifiers bind as plain aliases outside a module; inside
        // one they land in the export table and stay lazy. `export * as ns` is an
        // alias either way, so it never resolves here.
        is_named_exports && !self.ctx.is_external_module_file()
    }

    /// Whether a position-invalid `import ... from "m"` or `import x =
    /// require("m")` — already outside a function body and a namespace body,
    /// i.e. still left in the source file's own scope by
    /// [`Self::position_invalid_module_element_resolves_specifier`] — still
    /// resolves its module specifier.
    ///
    /// The import side does not share `export ... from`'s per-clause table
    /// story (#16504): every clause-bearing form (`import { a }`, `import a`,
    /// `import * as ns`, `import a, { b }`, `import type { a }`, and
    /// `import x = require(...)`) binds the same way regardless of which
    /// clause it is, so there is only one axis here — whether the file is an
    /// external module — not a clause-kind split. `tsc`'s
    /// `checkImportDeclaration` has already returned at the placement
    /// diagnostic (TS1232), so whatever still resolves comes from a later
    /// pass; a script file still runs that pass, an external module's does
    /// not (measured against the pinned oracle, #16505).
    ///
    /// A bare `import "m"` (no clause at all) is the one form that answers
    /// differently: its resolution diagnostic (TS2882/TS2307) never fires
    /// outside a declaration scope, in a script or a module alike — the
    /// opposite of every clause-bearing form.
    ///
    /// Deliberately not covered by this predicate, and not by its caller
    /// either: a plain import nested inside a namespace body's own block
    /// (`namespace N { { import { a } from "m"; } }`) turns on whether the
    /// imported binding is later *referenced*, the same discriminator
    /// `import x = require(...)` already applies through
    /// `namespace_import_alias_is_referenced` — not on module-ness. tsz has
    /// no equivalent reference check for the plain-import clause forms yet,
    /// so that shape is left exactly as it already was rather than folded
    /// into this predicate; both call sites guard this out via
    /// `!is_inside_namespace_declaration`.
    pub(crate) fn position_invalid_import_declaration_resolves_specifier(
        &self,
        has_import_clause: bool,
    ) -> bool {
        if !has_import_clause {
            return false;
        }
        !self.ctx.is_external_module_file()
    }

    /// Check if a node is inside a module augmentation
    /// (`declare module "string" { ... }`).  Module augmentations have a
    /// `MODULE_DECLARATION` ancestor whose name is a string literal.
    pub(crate) fn is_inside_module_augmentation(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(mod_data) = self.ctx.arena.get_module_at(current)
                && let Some(name_node) = self.ctx.arena.get(mod_data.name)
                && name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            {
                return true;
            }
        }
        false
    }

    /// Check if a node is inside a `declare global { ... }` augmentation block.
    pub(crate) fn is_inside_global_augmentation(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::MODULE_DECLARATION && node.is_global_augmentation() {
                return true;
            }
        }
        false
    }

    /// Returns `true` when `decl_idx` is (or is the name identifier of) an
    /// `export as namespace X;` declaration. These attach a global namespace
    /// alias to the containing module and do not introduce a local binding, so
    /// the TS2440 import/local-declaration conflict must ignore them.
    pub(crate) fn decl_is_namespace_export_declaration(&self, decl_idx: NodeIndex) -> bool {
        if let Some(node) = self.ctx.arena.get(decl_idx)
            && node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
        {
            return true;
        }
        if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && let Some(parent) = self.ctx.arena.get(ext.parent)
            && parent.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
        {
            return true;
        }
        false
    }
}
