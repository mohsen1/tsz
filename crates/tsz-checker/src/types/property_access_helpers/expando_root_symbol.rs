//! Root-symbol eligibility gates for JS `checkJs` expando reads and writes.
//!
//! Whether a plain variable root (`var X = {}`) or a namespace/function/class
//! root grants expando-member treatment for a property read or a direct
//! write. Extracted from `expando.rs` to keep that shard under the size
//! limit.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn root_symbol_for_expando_read(
        &self,
        object_expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        self.resolve_identifier_symbol(object_expr_idx)
            .or_else(|| self.resolve_qualified_symbol(object_expr_idx))
    }

    pub(super) fn expando_read_root_keys(&self, object_expr_idx: NodeIndex) -> Vec<String> {
        let mut keys = Vec::new();

        if let Some(obj_key) = Self::property_access_chain_in_arena(self.ctx.arena, object_expr_idx)
        {
            keys.push(obj_key.clone());
            if let Some((_, last_segment)) = obj_key.rsplit_once('.') {
                keys.push(last_segment.to_string());
            }
        }

        if let Some(sym_id) = self.root_symbol_for_expando_read(object_expr_idx)
            && let Some(symbol) = self.get_cross_file_symbol(sym_id)
        {
            let escaped_name = symbol.escaped_name.to_string();
            if !keys.iter().any(|key| key == &escaped_name) {
                keys.push(escaped_name);
            }
        }

        keys
    }

    pub(super) fn root_symbol_supports_js_expando_read(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
        else {
            return false;
        };

        // A namespace/module root grants expando access only when it is also
        // bound as a FUNCTION or CLASS (the `function f() {} namespace f {}`
        // merge pattern) — tsc's own `getExpandoInitializer` restricts the
        // assignment-declared-member pattern to callable/constructible
        // hosts. A pure module (VALUE_MODULE/NAMESPACE_MODULE with no
        // function/class merge) never grants it: `declare namespace C { ... }`
        // followed by `C.prototype = {}` or any other undeclared `C.x = ...`
        // in a JS file is an ordinary property write against `typeof C`
        // (`TS2339` when the member is undeclared), oracle-verified.
        if symbol.has_any_flags(symbol_flags::FUNCTION | symbol_flags::CLASS) {
            return true;
        }

        // A block/function-scoped variable never declaration-merges with a
        // class, so on a name collision it reports `TS2451`/`TS2300`, but tsc
        // still resolves every value-position reference to the name from
        // whichever declaration bound first (`mergeSymbol` in `checker.ts`,
        // see `cross_file_variable_class_merge.rs`). When an earlier-processed
        // file's class wins that merge, `symbol`'s own `var X = {}` shape is
        // not this name's real value anymore — it must not grant expando
        // eligibility, or a `.js` file's `X.d = {}` silently accepts an
        // assignment tsc reports `TS2339` for (the class has no `d`).
        if self.cross_file_class_declaration_shadows_variable(sym_id, symbol.flags) {
            return false;
        }

        if !symbol.has_any_flags(symbol_flags::VARIABLE) {
            return false;
        }

        let decl_idx = symbol.value_declaration;
        let file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(decl_node) = arena.get(decl_idx) else {
            return false;
        };
        let Some(var_decl) = arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(init_node) = arena.get(var_decl.initializer) else {
            return false;
        };

        if init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && self.variable_declaration_has_jsdoc_type_annotation(decl_idx)
        {
            return false;
        }

        // Only an EMPTY object literal (`var X = {}`) is an expando host: tsc's
        // `getExpandoInitializer` treats a non-empty literal (`var X = { a: 1 }`)
        // as a closed shape, so a later `X.b` read is `TS2339`, not an expando
        // member. Function/class expression initializers stay hosts regardless.
        init_node.is_function_expression_or_arrow()
            || init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || arena.is_empty_object_literal(var_decl.initializer)
    }

    /// tsc 7.0.2 binds an assignment-declared expando member only when the
    /// write appears in the host's OWN declaring file (oracle-pinned for
    /// function, class, and `var X = {}` hosts, TS and JS script files alike;
    /// see `js_cross_file_expando_declaration_tests`). A foreign-file write is
    /// an ordinary property assignment against the host's declared shape:
    /// `TS2339` under `noImplicitAny`, with the open-container leniency still
    /// silencing `{}`-typed receivers when it is off.
    pub(crate) fn expando_write_host_is_foreign_file(&self, sym_id: SymbolId) -> bool {
        self.ctx
            .resolve_symbol_file_index(sym_id)
            .is_some_and(|file_idx| file_idx != self.ctx.current_file_idx)
    }

    pub(super) fn root_symbol_supports_js_direct_expando_write(&self, sym_id: SymbolId) -> bool {
        if self.expando_write_host_is_foreign_file(sym_id) {
            return false;
        }

        let Some(symbol) = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
        else {
            return false;
        };

        // Same restriction as `root_symbol_supports_js_expando_read`: a pure
        // namespace/module root (no FUNCTION/CLASS merge) does not grant
        // direct expando writes either — oracle-verified via
        // `declare namespace C { ... }` rejecting both `C.prototype = {}`
        // and any other undeclared `C.x = ...` in a JS file.
        if symbol.has_any_flags(symbol_flags::FUNCTION | symbol_flags::CLASS) {
            return true;
        }

        // Same cross-file shadowing exclusion as `root_symbol_supports_js_expando_read`
        // (see `cross_file_class_declaration_shadows_variable`): a variable
        // whose name is won by an earlier-processed file's class is not this
        // name's real value, so it must not grant a direct expando write.
        if self.cross_file_class_declaration_shadows_variable(sym_id, symbol.flags) {
            return false;
        }

        if !symbol.has_any_flags(symbol_flags::VARIABLE) {
            return false;
        }

        let decl_idx = symbol.value_declaration;
        let file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(decl_node) = arena.get(decl_idx) else {
            return false;
        };
        let Some(var_decl) = arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(init_node) = arena.get(var_decl.initializer) else {
            return false;
        };

        // Mirror `root_symbol_supports_js_expando_read`: a `var X = {}` object
        // literal is an expando host. The binder's per-file expando tracking only
        // records the write when the writing file can resolve the root, so a
        // cross-file (or forward-referenced) `X.member = value` whose `X = {}`
        // declaration lives in another file is missed there; this cross-file-aware
        // predicate keeps the write from surfacing a spurious TS2339 on `{}`,
        // matching the read side that already resolves such members. A JSDoc
        // `@type` annotation opts the variable out of the expando model.
        if init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && self.variable_declaration_has_jsdoc_type_annotation(decl_idx)
        {
            return false;
        }

        // Emptiness gate, mirroring the read side and tsc's
        // `getExpandoInitializer`: a `var X = {}` empty literal hosts expando
        // writes, but `var X = { a: 1 }` is a closed shape whose later `X.b = …`
        // write is an ordinary property assignment (`TS2339` under
        // `noImplicitAny`; silenced under the open-container leniency otherwise).
        init_node.is_function_expression_or_arrow()
            || init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || arena.is_empty_object_literal(var_decl.initializer)
    }
}
