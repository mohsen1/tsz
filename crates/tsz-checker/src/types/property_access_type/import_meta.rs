//! `import.meta` property access handling.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Handles import.meta property access.
    /// Returns Some(type) if this is an import.meta access, None otherwise.
    pub(crate) fn try_resolve_import_meta_access(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
    ) -> Option<TypeId> {
        let expr_node = self.ctx.arena.get(expression)?;
        if expr_node.kind != SyntaxKind::ImportKeyword as u16 {
            return None;
        }

        let is_meta = self
            .ctx
            .arena
            .get(name_or_argument)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .is_some_and(|ident| ident.escaped_text == "meta");

        if is_meta {
            self.check_import_meta_in_cjs(idx);
            // import.meta resolves to the global `ImportMeta` interface
            // (declared in lib.es2020.full.d.ts). Returning that type
            // enables TS2339 on unknown properties (`import.meta.blah`)
            // and merges `declare global { interface ImportMeta { ... } }`
            // augmentations through lib-heritage merging.
            if let Some(import_meta_ty) = self.resolve_lib_type_by_name("ImportMeta") {
                return Some(import_meta_ty);
            }
        }
        // Fallback (ImportMeta not in lib scope, or non-`meta` meta-property
        // like `import.metal`): return ANY so downstream access doesn't
        // cascade misleading TS2339s. A separate grammar check is expected
        // to emit TS17012 for the invalid meta-property name.
        Some(TypeId::ANY)
    }
}
