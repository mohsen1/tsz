//! Gating and anchoring for the JSDoc import-type member TS2694 ("namespace has
//! no exported member") diagnostic (issue #17176).
//!
//! A JSDoc `import('mod').Member` type string is resolved from the comment-scan
//! validation pass AND again from each *lazy* type computation of the annotated
//! symbol (declaration, parameter, return). The resolution kernel emits the
//! diagnostic as a side effect, so without a gate the same failure is reported
//! two or more times, at coarse/wrong anchors. The comment-scan pass owns the
//! single diagnostic (anchored at the member token); the lazy paths resolve the
//! same string silently.
//!
//! The gate is a scoped thread-local rather than a `CheckerContext` field: the
//! comment-scan pass sets it around each annotation's resolution with
//! [`ImportTypeMemberDiagGuard`], and always restores the prior value, so it
//! never leaks across annotations, files, or reused workers. (The string-based
//! JSDoc resolver is the only reader; the TS-syntax `import(...)` AST uses a
//! separate emitter and is unaffected.)

use crate::state::CheckerState;
use std::cell::Cell;

thread_local! {
    /// `true` only while the comment-scan validation pass is resolving a JSDoc
    /// type expression and therefore owns the single TS2694. `false` for every
    /// lazy type computation.
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
}

/// Whether the import-type member TS2694 may be emitted for the current
/// resolution (i.e. the comment-scan validation pass is active).
pub(in crate::jsdoc) fn import_type_member_diag_active() -> bool {
    ACTIVE.with(Cell::get)
}

/// RAII guard that marks the comment-scan validation pass active for the
/// duration of a resolution, restoring the prior value on drop.
pub(in crate::jsdoc) struct ImportTypeMemberDiagGuard {
    previous: bool,
}

impl ImportTypeMemberDiagGuard {
    /// Activate the gate; the diagnostic may be emitted until the guard drops.
    pub(in crate::jsdoc) fn active() -> Self {
        let previous = ACTIVE.with(|cell| cell.replace(true));
        Self { previous }
    }
}

impl Drop for ImportTypeMemberDiagGuard {
    fn drop(&mut self) {
        ACTIVE.with(|cell| cell.set(self.previous));
    }
}

/// Source span for the trailing member-name token of a JSDoc
/// `import('mod').Member` type expression, used to anchor the TS2694 diagnostic
/// at `Member` (tsc parity) rather than at the coarse comment/declaration
/// anchor.
///
/// `anchor` is the start of the JSDoc comment (or the tag's own type expression)
/// carrying `type_expr`; the member token is located by finding `type_expr`
/// at/after the anchor and offsetting to its last identifier segment. Falls back
/// to `(anchor, type_expr.len())` when the anchor is unscoped or the expression
/// cannot be located (e.g. a reformatted multi-line comment).
pub(in crate::jsdoc) fn import_type_member_anchor(
    source_text: &str,
    anchor: u32,
    type_expr: &str,
    member_name: &str,
) -> (u32, u32) {
    let fallback = (anchor, type_expr.len() as u32);
    if anchor == u32::MAX {
        return fallback;
    }
    let anchor_usize = anchor as usize;
    if anchor_usize > source_text.len() {
        return fallback;
    }
    let Some(rel) = source_text[anchor_usize..].find(type_expr) else {
        return fallback;
    };
    let Some(member_offset) = type_expr.rfind(member_name) else {
        return fallback;
    };
    (
        (anchor_usize + rel + member_offset) as u32,
        member_name.len() as u32,
    )
}

impl<'a> CheckerState<'a> {
    /// Emit the JSDoc import-type member TS2694 for an `import('mod').Member`
    /// whose `Member` does not resolve — but only while the comment-scan
    /// validation pass is active, anchored at the member-name token. Lazy type
    /// computations that reach the same resolution leave the gate inactive and
    /// this is a no-op, so the diagnostic fires exactly once (issue #17176).
    pub(in crate::jsdoc) fn emit_jsdoc_import_type_member_missing(
        &mut self,
        module_specifier: &str,
        member_name: &str,
        type_expr: &str,
    ) {
        if !import_type_member_diag_active() {
            return;
        }
        let namespace_name = self.imported_namespace_display_module_name(module_specifier);
        let message = crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
            &[&format!("\"{namespace_name}\""), member_name],
        );
        let source_text = self
            .ctx
            .arena
            .source_files
            .first()
            .map_or("", |source_file| source_file.text.as_ref());
        let (start, length) = import_type_member_anchor(
            source_text,
            self.ctx.jsdoc_typedef_anchor_pos.get(),
            type_expr,
            member_name,
        );
        self.ctx.error(
            start,
            length,
            message,
            crate::diagnostics::diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
        );
    }
}
