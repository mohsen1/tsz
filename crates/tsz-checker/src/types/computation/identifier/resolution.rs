//! Identifier resolution helpers — unresolved identifiers, known globals,
//! cross-file lookups, UMD handling, import binding checks, and value
//! declaration selection.

use crate::context::{is_js_file_name, should_resolve_jsdoc_for_file};
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Resolve an identifier that was NOT found in the binder's scope chain.
    ///
    /// Handles intrinsics (`undefined`, `NaN`, `Symbol`), known globals
    /// (`console`, `Math`, `Array`, etc.), static member suggestions, and
    /// "cannot find name" error reporting.
    pub(crate) fn resolve_unresolved_identifier(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        match name {
            "undefined" => TypeId::UNDEFINED,
            "NaN" | "Infinity" => TypeId::NUMBER,
            "Symbol" => self.resolve_symbol_constructor(idx, name),
            _ if self.is_known_global_value_name(name) => self.resolve_known_global(idx, name),
            _ => self.resolve_truly_unknown_identifier(idx, name),
        }
    }

    /// Resolve the `Symbol` constructor. Emits TS2583/TS2585 if Symbol is
    /// unavailable or type-only (ES5 target).
    fn resolve_symbol_constructor(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        if !self.ctx.has_symbol_in_lib() {
            self.error_cannot_find_name_change_lib(name, idx);
            return TypeId::ERROR;
        }
        // NOTE: tsc 6.0 does NOT emit TS2585 based on target version alone.
        // Symbol may be available even with target ES5 via transitive lib loading.
        // Proceed to check if the value binding actually exists.
        let value_type = self.type_of_value_symbol_by_name(name);
        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
            return value_type;
        }
        // Route through wrong-meaning boundary: Symbol is a type-only name
        use crate::query_boundaries::name_resolution::NameLookupKind;
        self.report_wrong_meaning_diagnostic(name, idx, NameLookupKind::Type);
        TypeId::ERROR
    }

    /// Resolve a known global value name (e.g. `console`, `Math`, `Array`).
    /// Tries binder `file_locals` and lib binders, then falls back to error reporting.
    fn resolve_known_global(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        if self.is_nodejs_runtime_global(name) {
            if self.is_private_name_access_base(idx) {
                self.error_at_node_msg(
                    idx,
                    crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                    &[name],
                );
                return TypeId::ERROR;
            }
            // In CommonJS module mode, `exports` is implicitly available as the module namespace.
            // Other node globals (module, require, __dirname, __filename) still need @types/node;
            // tsc emits TS2591 for them even in CommonJS mode when type definitions are absent.
            if self.ctx.compiler_options.module.is_commonjs()
                && name == "exports"
                && !self.current_source_file_has_esm_syntax()
            {
                return self.current_file_commonjs_namespace_type();
            }
            if self.is_require_call_callee_without_node_global(name, idx) {
                return TypeId::ANY;
            }
            if self.is_js_file() {
                match name {
                    "exports" => {
                        if self.current_source_file_has_esm_syntax() {
                            self.error_at_node_msg(
                                idx,
                                crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                                &[name],
                            );
                            return TypeId::ERROR;
                        }
                        return self.current_file_commonjs_namespace_type();
                    }
                    "module" | "require" if !self.current_source_file_has_esm_syntax() => {
                        return TypeId::ANY;
                    }
                    "module" | "require" => {
                        self.error_at_node_msg(
                            idx,
                            crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
                            &[name],
                        );
                        return TypeId::ERROR;
                    }
                    "__dirname" | "__filename" => {
                        self.error_at_node_msg(
                            idx,
                            crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                            &[name],
                        );
                        return TypeId::ERROR;
                    }
                    _ => {}
                }
            }
            // Non-JS (`.ts`) fallthrough. tsc's `getCannotFindNameDiagnosticForName`
            // gives the "@types/node" hint (TS2591) only to `module` / `require`
            // here; `exports`, `__dirname`, and `__filename` are CommonJS wrapper
            // locals reported as plain TS2304 (`process` / `Buffer` / `NodeJS`
            // never reach this branch — they are not CommonJS wrapper locals and
            // are routed through the shared install-hint dispatch).
            match name {
                "module" | "require" => {
                    self.error_cannot_find_name_install_node_types(name, idx);
                }
                _ => {
                    self.error_at_node_msg(
                        idx,
                        crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                        &[name],
                    );
                }
            }
            return TypeId::ERROR;
        }

        let value_type = self.type_of_value_symbol_by_name(name);
        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
            return value_type;
        }

        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return self.get_type_of_symbol(sym_id);
        }
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)
        {
            return self.get_type_of_symbol(sym_id);
        }

        // The ambient `globalThis` value has no `declare var` declaration; it is
        // the synthetic global-object reference. tsc types a value read of it as
        // `typeof globalThis` (a concrete object whose members are the in-scope
        // globals), not `any`. Resolving it to the surface keeps member reads
        // (`globalThis.X`) and `(typeof globalThis)['X']` indexed accesses from
        // collapsing to `any`. Guarded by `is_global_this_expression` so a
        // same-file local named `globalThis` keeps its own binding.
        if name == "globalThis" && self.is_global_this_expression(idx) {
            return self.ctx.global_this_surface_type();
        }

        self.emit_global_not_found_error(idx, name)
    }

    /// Check whether an identifier is the base of a private-name access (`obj.#field`).
    pub(crate) fn is_private_name_access_base(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let mut current = idx;
        let mut guard = 0;
        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                && let Some(member) = self.ctx.arena.get(access.name_or_argument)
                && member.kind == SyntaxKind::PrivateIdentifier as u16
            {
                let mut chain_expr = access.expression;
                let mut chain_guard = 0;
                while chain_expr.is_some() {
                    chain_guard += 1;
                    if chain_guard > 256 {
                        break;
                    }
                    if chain_expr == idx {
                        return true;
                    }
                    let Some(chain_node) = self.ctx.arena.get(chain_expr) else {
                        break;
                    };
                    if chain_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        && chain_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    {
                        break;
                    }
                    let Some(chain_access) = self.ctx.arena.get_access_expr(chain_node) else {
                        break;
                    };
                    chain_expr = chain_access.expression;
                }
            }
            current = parent_idx;
        }
        false
    }

    /// Emit an appropriate error when a known global is not found.
    ///
    /// Routes through the environment capability boundary (`diagnose_missing_name`)
    /// to determine the appropriate diagnostic.
    fn emit_global_not_found_error(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        use crate::query_boundaries::environment::CapabilityDiagnostic;

        // tsc's value-position TS2583 ("change your target library") only
        // applies to a narrow set of well-known ES2015+ globals (Map, Set,
        // Promise, Symbol, …). Other ES2015+ types like `Proxy`, `Generator`,
        // `WeakRef` fall through to plain TS2304 at the call site even though
        // they live in es2015+ libs. See `getCannotFindNameDiagnosticForName`
        // in tsc's checker.ts.
        let value_lib_suggestion =
            tsz_binder::lib_loader::is_es2015_plus_value_lib_suggestion(name);

        if !self.ctx.capabilities.has_lib {
            if value_lib_suggestion
                && matches!(
                    self.ctx.capabilities.diagnose_missing_name(name),
                    Some(CapabilityDiagnostic::MissingEs2015Type { .. })
                )
            {
                self.error_cannot_find_name_change_lib(name, idx);
            } else {
                // Route through boundary for TS2304/TS2552 with suggestion collection
                self.report_not_found_at_boundary(
                    name,
                    idx,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                );
            }
            return TypeId::ERROR;
        }

        match self.ctx.capabilities.diagnose_missing_name(name) {
            Some(CapabilityDiagnostic::MissingPlainGlobalValue { .. }) => {
                self.report_not_found_at_boundary(
                    name,
                    idx,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                );
                return TypeId::ERROR;
            }
            Some(CapabilityDiagnostic::MissingDomGlobal { .. }) => {
                // Route through boundary for TS2304/TS2552 with suggestion collection
                self.report_not_found_at_boundary(
                    name,
                    idx,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                );
                return TypeId::ERROR;
            }
            Some(CapabilityDiagnostic::MissingEs2015Type { .. }) if value_lib_suggestion => {
                self.error_cannot_find_global_type(name, idx);
                return TypeId::ERROR;
            }
            Some(CapabilityDiagnostic::MissingEs2015Type { .. }) => {
                // Names like `Proxy` are in ES2015+ libs but tsc still emits
                // plain TS2304 in value position — fall through to the
                // not-found boundary path.
                self.report_not_found_at_boundary(
                    name,
                    idx,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                );
                return TypeId::ERROR;
            }
            _ => {}
        }

        let first_char = name.chars().next().unwrap_or('a');
        // A genuinely-missing *known global* (lib + binder lookup already failed
        // above, and it is not one of the lib/install-hint categories handled in
        // the match) is a real "cannot find name": `tsc` reports TS2304 for it,
        // e.g. `window` / `fetch` / `URL` without the `dom` lib. Only a
        // *non-global* uppercase identifier stays `any` here — it may be a
        // not-yet-resolved user type, where suppressing TS2304 avoids a cascade.
        // Known globals fall through to `report_not_found_at_boundary` so the
        // diagnostic is actually emitted (previously they returned `any` and
        // silently dropped the error — masked only because the over-broad DOM
        // classifier used to mis-route them to TS2584).
        //
        // `globalThis` is the exception: it is the ambient global-object
        // reference, in scope under every `lib`/`target` (tsc never reports it
        // as missing). In value position it resolves to the synthetic
        // `typeof globalThis` surface — the same concrete object the `typeof`
        // type query produces — so a value read (`const g = globalThis`) and
        // its member/indexed access match tsc instead of collapsing to `any`.
        if name == "globalThis" {
            return self.get_global_this_type(idx);
        }
        if first_char.is_uppercase() && !self.is_known_global_value_name(name) {
            return TypeId::ANY;
        }

        // TS2693: Primitive type keywords used as values
        // TypeScript primitive type keywords (number, string, boolean, etc.) are language keywords
        // for types, not identifiers. When used in value position, emit TS2693.
        // NOTE: `symbol` is excluded — tsc never emits TS2693 for lowercase `symbol`.
        // Instead it emits TS2552 "Cannot find name 'symbol'. Did you mean 'Symbol'?"
        // Exception: in import equals module references (e.g., `import r = undefined`),
        // TS2503 is already emitted by check_namespace_import — don't also emit TS2693.
        if matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "void"
                | "undefined"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint"
        ) {
            // Suppress TS2693 when this identifier is the expression of an element
            // access with a missing argument (e.g., `new number[]`).  The parser
            // already emits TS1011 and tsc does not emit TS2693 in this case.
            if let Some(parent_ext) = self.ctx.arena.get_extended(idx)
                && parent_ext.parent.is_some()
                && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
            {
                use tsz_parser::parser::syntax_kind_ext;
                if parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                    && access.name_or_argument.is_none()
                {
                    return TypeId::ERROR;
                }
            }
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            self.error_at_node(
                idx,
                &format_message(
                    diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                    &[name],
                ),
                diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
            );
            return TypeId::ERROR;
        }

        // Route through boundary for TS2304/TS2552 with suggestion collection
        self.report_not_found_at_boundary(
            name,
            idx,
            crate::query_boundaries::name_resolution::NameLookupKind::Value,
        );
        TypeId::ERROR
    }

    /// The declared member nodes and name text of the class at `class_idx`.
    fn class_members_and_name(&self, class_idx: NodeIndex) -> Option<(Vec<NodeIndex>, String)> {
        let class_node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(class_node)?;
        let members = class.members.nodes.clone();
        let name_node = self.ctx.arena.get(class.name)?;
        let name = self
            .ctx
            .arena
            .get_identifier(name_node)?
            .escaped_text
            .to_string();
        Some((members, name))
    }

    /// Whether the global `Function` interface has a member `name`. Every class
    /// constructor type is a subtype of `Function` (which extends `Object`), so
    /// tsc's `getPropertyOfType(typeof Class, name)` resolves the `Function`
    /// (and `Object`) members on it — `arguments`, `caller`, `length`, `name`,
    /// `prototype`, `apply`, `call`, `bind`, `hasOwnProperty`, … This is the same
    /// subtype-of-`Function` mechanism, applied to any name (not an
    /// `arguments`-specific special case), and it is stable across the two
    /// resolution passes because it depends only on the global lib type, not on
    /// the enclosing class's own (mid-computation) constructor type.
    fn global_function_type_has_member(&mut self, name: &str) -> bool {
        let Some(function_type) = self.resolve_lib_type_by_name("Function") else {
            return false;
        };
        matches!(
            self.resolve_property_access_with_env(function_type, name),
            tsz_solver::operations::property::PropertyAccessResult::Success {
                from_index_signature: false,
                ..
            }
        )
    }

    /// Whether a class in the `extends` chain *above* `class_idx` (its base
    /// classes, not `class_idx` itself) declares a member named `name` that is
    /// static when `want_static`, or instance otherwise. This extends the
    /// member-prefix suggestion to inherited members — `class D extends C`
    /// referencing `C`'s static `s` unqualified is `TS2662 'D.s'` in tsc, just
    /// like an own static.
    ///
    /// Resolved purely from the AST heritage and binder symbol flags (via
    /// [`Self::get_base_class_idx`] / [`Self::is_static_member`]) — never a
    /// materialized constructor/instance type — so, like the own-member check,
    /// it yields the identical answer in both the type-computation and
    /// statement-walk passes and cannot desync into a double diagnostic. The
    /// walk is cycle-guarded (an illegal `extends` cycle would otherwise loop).
    fn base_chain_declares_member(
        &self,
        class_idx: NodeIndex,
        name: &str,
        want_static: bool,
    ) -> bool {
        let mut visited: rustc_hash::FxHashSet<NodeIndex> = rustc_hash::FxHashSet::default();
        visited.insert(class_idx);
        let mut current = class_idx;
        while let Some(base) = self.get_base_class_idx(current) {
            if !visited.insert(base) {
                break;
            }
            if let Some(class) = self.ctx.arena.get_class_at(base) {
                let members = &class.members.nodes;
                let found = if want_static {
                    self.is_static_member(members, name)
                } else {
                    self.is_instance_member(members, name)
                };
                if found {
                    return true;
                }
            }
            current = base;
        }
        false
    }

    /// Handle a truly unresolved identifier — not a type parameter, not in the
    /// binder, not a known global. Emits the member-prefix suggestion
    /// (`TS2662`/`TS2663`), `TS2524`/`TS2523`, or falls through to the bare
    /// `TS2304`/`TS2552` name-resolution boundary.
    fn resolve_truly_unknown_identifier(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        // Note: TS1212/1213/1214 strict-mode reserved word check is now handled
        // centrally in error_cannot_find_name_at to cover both value and type contexts.

        // Member-prefix suggestion (tsc's `checkAndReportErrorForMissingPrefix`):
        // an unresolved identifier inside a class that names a member reachable on
        // the class's constructor type gets `TS2662` ("Did you mean the static
        // member 'C.x'?"), or — in a non-static context — its instance type gets
        // `TS2663` ("Did you mean the instance member 'this.x'?"), instead of a
        // bare `TS2304`.
        //
        // The class is found from the AST (`nearest_enclosing_class`) rather than
        // the ambient `enclosing_class` context, which is unset during on-demand
        // class-type computation (evaluating member initializers to type them).
        // A suggestion keyed on that context would fire in the statement-walk
        // pass but not the type-computation pass; since both passes resolve the
        // same identifier and diagnostics dedupe by `(start, code)`, a suggestion
        // in one pass and a bare `TS2304` in the other would *both* survive.
        // Deciding from the AST (and the global-lib `Function` type) makes the two
        // passes agree — one diagnostic. `nearest_enclosing_class` walks through
        // function boundaries, so a class's statics are suggestable from a nested
        // function too; the `this`-rebind boundary is enforced on the instance
        // arm below.
        // Only from a *value* position. In a type position — `b: typeof a`,
        // `m(x: typeof a)`, `m(): typeof a` — tsc reports the bare `TS2304`
        // even when the name matches a member, because `this.a` is not
        // writable there. In tsc this falls out of call-site placement:
        // `checkAndReportErrorForMissingPrefix` is reached from
        // `checkIdentifier`, while a type-position name resolves through the
        // type-reference path. Here both paths share this function, so the
        // position has to be asked for explicitly.
        if !self.is_in_type_query(idx)
            && let Some(class_idx) = self.nearest_enclosing_class(idx)
            && let Some((member_nodes, class_name)) = self.class_members_and_name(class_idx)
        {
            // Static (TS2662): a declared static — own or inherited from a base
            // class — or a member of `Function` (`arguments`/`caller`/… — every
            // constructor type is a subtype of `Function`). Checked before the
            // instance arm, matching tsc.
            if self.is_static_member(&member_nodes, name)
                || self.global_function_type_has_member(name)
                || self.base_chain_declares_member(class_idx, name, true)
            {
                self.error_cannot_find_name_static_member_at(name, &class_name, idx);
                return TypeId::ERROR;
            }
            // Instance (TS2663): a declared instance member — own or inherited
            // from a base class — in a non-static context (in a static member
            // `this` is the constructor, so tsc reports the bare `TS2304`) with
            // no regular-function boundary between the reference and the class (a
            // function rebinds `this`, so `this.x` would not reach the class
            // instance).
            if !self.is_in_static_class_member_context(idx)
                && !self.has_regular_function_boundary_to_class(idx)
                && (self.is_instance_member(&member_nodes, name)
                    || self.base_chain_declares_member(class_idx, name, false))
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let message = format_message(
                    diagnostic_messages::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
                    &[name],
                );
                self.error_at_node(
                    idx,
                    &message,
                    diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
                );
                return TypeId::ERROR;
            }
        }
        // TS2524: 'await' in default parameter
        if name == "await" && self.is_in_default_parameter(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
            return TypeId::ERROR;
        }
        // TS2523: 'yield' in default parameter
        if name == "yield" && self.is_in_default_parameter(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
            return TypeId::ERROR;
        }
        // Suppress TS2304 for unresolved imports (TS2307 was already emitted)
        if self.is_unresolved_import_symbol(idx) {
            return TypeId::ANY;
        }
        if self.is_root_identifier_of_js_prototype_assignment(idx) {
            return TypeId::ANY;
        }
        // Check known globals that might be missing
        if self.is_known_global_value_name(name) {
            return self.emit_global_not_found_error(idx, name);
        }
        // Primitive type keywords used as values should emit TS2693 ("only
        // refers to a type, but is being used as a value here"), not TS2304.
        // These are built-in language keywords — they exist as types but
        // cannot be used as runtime values.
        if matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "symbol"
                | "void"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint"
        ) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            self.error_at_node(
                idx,
                &format_message(
                    diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                    &[name],
                ),
                diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
            );
            return TypeId::ERROR;
        }
        // Suppress in single-file mode to prevent cascading false positives
        if !self.ctx.report_unresolved_imports {
            return TypeId::ANY;
        }
        // Route through the unified name resolution boundary for TS2304/TS2552
        let req = crate::query_boundaries::name_resolution::NameResolutionRequest::value(name, idx);
        let result = self.resolve_name_structured(&req);
        match result {
            Ok(_) => {
                // Symbol found — shouldn't happen since binder already failed,
                // but avoid false diagnostic
                TypeId::ERROR
            }
            Err(failure) => {
                self.report_name_resolution_failure(&req, &failure);
                TypeId::ERROR
            }
        }
    }

    /// Check if there's a regular function expression (non-arrow) between `idx`
    /// and the enclosing class declaration. Regular functions rebind `this`, so
    /// `this.member` wouldn't refer to the class instance. Arrow functions and
    /// method declarations preserve the outer `this`.
    fn has_regular_function_boundary_to_class(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = idx;
        let mut guard = 0;
        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                // Hit the class boundary — no regular function in between
                if node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    return false;
                }
                // Regular function expression rebinds `this`
                if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                {
                    return true;
                }
                // Arrow functions and method declarations are fine (preserve `this`)
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        false
    }

    /// Returns `true` if any cross-file binder provides a non-UMD VALUE binding for
    /// `name`. This handles cases like `declare global { const React }` where a separate
    /// declaration provides a legitimate global value binding alongside the UMD export.
    pub(crate) fn has_non_umd_global_value(&self, name: &str) -> bool {
        // VALUE_MODULE alone means "I'm a namespace declaration" — not a real
        // runtime value.  Exclude it so uninstantiated namespaces (which only
        // carry VALUE_MODULE) do not suppress TS2708 / TS2686 diagnostics.
        let real_value_flags = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;

        // A symbol counts as a non-UMD global value if:
        // 1. It has real value flags AND is not a UMD export, OR
        // 2. It is a UMD export that has been merged with a non-UMD value declaration
        //    (e.g., `export as namespace X` merged with `declare global { const X }`).
        //    In this case, the symbol retains `is_umd_export = true` but gains
        //    VARIABLE flags from the global augmentation.
        let is_non_umd_value = |sym: &tsz_binder::Symbol| -> bool {
            let has_real_value = sym.has_any_flags(real_value_flags);
            if !has_real_value {
                return false;
            }
            if !sym.is_umd_export {
                return true;
            }
            // UMD export merged with a variable declaration from `declare global`
            sym.has_any_flags(symbol_flags::VARIABLE)
        };

        // Check lib_contexts (lib files + some user files)
        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(sym) = lib_ctx.binder.get_symbol(sym_id)
                && is_non_umd_value(sym)
            {
                return true;
            }
        }
        // Check all_binders (all project files in multi-file mode)
        // Use global_file_locals_index for O(1) lookup
        if let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
        {
            let all_binders = self.ctx.all_binders.as_ref();
            for &(file_idx, sym_id) in entries {
                let binder = all_binders
                    .and_then(|binders| binders.get(file_idx))
                    .map(|binder| binder.as_ref())
                    .unwrap_or(self.ctx.binder);
                if let Some(sym) = binder.get_symbol(sym_id)
                    && is_non_umd_value(sym)
                {
                    return true;
                }
            }
        } else if let Some(ref all_binders) = self.ctx.all_binders {
            for binder in all_binders.iter() {
                if let Some(sym_id) = binder.file_locals.get(name)
                    && let Some(sym) = binder.get_symbol(sym_id)
                    && is_non_umd_value(sym)
                {
                    return true;
                }
            }
        }

        // Check global augmentations (`declare global { namespace X { ... } }`).
        // A `declare global` namespace with value members provides a legitimate
        // global value binding that should suppress TS2686 / TS2708, even though
        // the namespace symbol only carries VALUE_MODULE (excluded above).
        // tsc merges these augmentations into the global symbol, giving it value
        // semantics; we check for their existence as a proxy.
        if self.ctx.binder.global_augmentations.contains_key(name) {
            return true;
        }
        for lib_ctx in self.ctx.lib_contexts.iter() {
            if lib_ctx.binder.global_augmentations.contains_key(name) {
                return true;
            }
        }
        if let Some(ref all_binders) = self.ctx.all_binders {
            for binder in all_binders.iter() {
                if binder.global_augmentations.contains_key(name) {
                    return true;
                }
            }
        }

        false
    }

    pub(crate) fn current_file_is_module_for_umd_global_access(&self) -> bool {
        if self.ctx.binder.is_external_module() {
            return true;
        }

        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }

        // `self.ctx.arena` is a per-file arena holding exactly one source file
        // at position 0; selecting it by the program-global `current_file_idx`
        // only succeeds when the file sits at program index 0. Any checkJs file
        // at a non-zero index otherwise reported "not a module" here, silently
        // suppressing TS2686 UMD-global-access diagnostics depending on file
        // order (#17410). Use `.first()` like the 98 sibling sites so module
        // detection depends on the file's own arena, not on program order.
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match stmt.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => return true,
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    let Some(var_stmt) = self.ctx.arena.get_variable(stmt) else {
                        continue;
                    };
                    for &list_idx in &var_stmt.declarations.nodes {
                        let Some(list_node) = self.ctx.arena.get(list_idx) else {
                            continue;
                        };
                        let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                            continue;
                        };
                        for &decl_idx in &var_list.declarations.nodes {
                            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                                continue;
                            };
                            let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node)
                            else {
                                continue;
                            };
                            if decl.initializer.is_some()
                                && self
                                    .get_require_module_specifier(decl.initializer)
                                    .is_some()
                            {
                                return true;
                            }
                        }
                    }
                }
                syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt) else {
                        continue;
                    };
                    if self
                        .get_require_module_specifier(expr_stmt.expression)
                        .is_some()
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    pub(crate) fn cross_file_global_value_type_by_name(
        &mut self,
        name: &str,
        include_js: bool,
    ) -> Option<TypeId> {
        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;
        let entries = if let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
            .cloned()
        {
            entries
        } else {
            all_binders
                .iter()
                .enumerate()
                .filter_map(|(file_idx, binder)| {
                    binder
                        .file_locals
                        .get(name)
                        .map(|sym_id| (file_idx, sym_id))
                })
                .collect()
        };

        for (file_idx, sym_id) in entries {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }

            let Some(arena) = all_arenas.get(file_idx).cloned() else {
                continue;
            };
            let Some(binder) = all_binders.get(file_idx).cloned() else {
                continue;
            };
            let Some(source_file) = arena.source_files.first() else {
                continue;
            };
            if !include_js && is_js_file_name(&source_file.file_name) {
                continue;
            }

            let Some(symbol) = binder.get_symbol(sym_id) else {
                continue;
            };
            if symbol.escaped_name != name
                || !symbol.has_any_flags(symbol_flags::VALUE)
                || symbol.is_umd_export
            {
                continue;
            }
            let candidate_decl = symbol
                .declarations
                .iter()
                .copied()
                .find(|&decl_idx| {
                    if !decl_idx.is_some() {
                        return false;
                    }
                    let Some(node) = arena.get(decl_idx) else {
                        return false;
                    };
                    matches!(
                        node.kind,
                        tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
                            | tsz_parser::parser::syntax_kind_ext::VARIABLE_STATEMENT
                            | tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                    )
                })
                .or_else(|| {
                    (symbol.value_declaration.is_some()
                        && arena.get(symbol.value_declaration).is_some())
                    .then_some(symbol.value_declaration)
                })
                .unwrap_or(NodeIndex::NONE);

            // Fast path: if another checker (parallel worker, prior delegation)
            // already resolved this cross-file value symbol's type, reuse the
            // cached answer instead of building a fresh child checker.
            if let Some((cached_type, _)) = self
                .ctx
                .cached_cross_file_symbol_type(sym_id, file_idx as u32)
                && cached_type != TypeId::ANY
            {
                return Some(cached_type);
            }

            let Some(_cross_arena_guard) = Self::enter_cross_arena_delegation() else {
                continue;
            };

            let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
                arena.as_ref(),
                binder.as_ref(),
                self.ctx.types,
                source_file.file_name.clone(),
                self.ctx.compiler_options.clone(),
                self,
                tsz_common::perf_counters::CheckerCreationReason::IdentifierResolution,
            ));
            checker.ctx.copy_cross_file_state_from(&self.ctx);
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            checker.ctx.current_file_idx = file_idx;
            checker.ctx.symbol_resolution_set = self.ctx.symbol_resolution_set.clone();
            checker.ctx.symbol_resolution_stack = self.ctx.symbol_resolution_stack.clone();
            checker
                .ctx
                .symbol_resolution_depth
                .set(self.ctx.symbol_resolution_depth.get());

            let candidate_type = if candidate_decl.is_some() {
                checker.type_of_value_declaration(candidate_decl)
            } else {
                checker.get_type_of_symbol(sym_id)
            };

            if !matches!(
                candidate_type,
                TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
            ) {
                return Some(candidate_type);
            }
        }

        None
    }

    fn non_js_cross_file_global_value_type_by_name(&mut self, name: &str) -> Option<TypeId> {
        self.cross_file_global_value_type_by_name(name, false)
    }

    pub(crate) fn preferred_non_js_cross_file_global_value_type(
        &mut self,
        name: &str,
        local_sym_id: SymbolId,
    ) -> Option<TypeId> {
        // A JS expando container declared in the current file is authoritative
        // for its own uses: `tsc` keeps `a.js`'s `var x = function(){}` (plus
        // its `x.prop = ...` members) even when `x` merges with a plain
        // `var x` of another type in a sibling `.ts` file. Deferring to the
        // sibling's non-JS type here is exactly the contamination that types
        // the local `x` as the foreign value and rejects its own expando write.
        if self.current_file_owns_expando_container_variable(local_sym_id) {
            return None;
        }
        if self.ctx.binder.file_locals.get(name) != Some(local_sym_id) {
            return self.non_js_cross_file_global_value_type_by_name(name);
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id) {
            for &decl_idx in &symbol.declarations {
                if !decl_idx.is_some() {
                    continue;
                }
                let Some(source_file) = self.source_file_data_for_node(decl_idx) else {
                    continue;
                };
                if source_file.file_name == self.ctx.file_name
                    || is_js_file_name(&source_file.file_name)
                {
                    continue;
                }

                let candidate_type = self.type_of_value_declaration(decl_idx);
                if !matches!(
                    candidate_type,
                    TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
                ) {
                    return Some(candidate_type);
                }
            }
        }
        self.non_js_cross_file_global_value_type_by_name(name)
    }

    /// Returns `true` if `idx` is the name identifier inside an
    /// `export as namespace X` declaration.
    pub(crate) fn is_namespace_export_declaration_name(&self, idx: NodeIndex) -> bool {
        if let Some(ext) = self.ctx.arena.get_extended(idx)
            && ext.parent.is_some()
            && let Some(parent) = self.ctx.arena.get(ext.parent)
        {
            return parent.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION;
        }
        false
    }

    /// Returns `true` if `idx` is the expression identifier inside an
    /// `export = X` assignment. That reference is part of the UMD definition
    /// site and should not be treated as a module-side UMD global usage.
    pub(crate) fn is_export_assignment_expression_name(&self, idx: NodeIndex) -> bool {
        if let Some(ext) = self.ctx.arena.get_extended(idx)
            && ext.parent.is_some()
            && let Some(parent) = self.ctx.arena.get(ext.parent)
            && parent.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            && let Some(assign) = self.ctx.arena.get_export_assignment(parent)
        {
            return assign.expression == idx;
        }
        false
    }

    fn is_root_identifier_of_js_prototype_assignment(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let mut current = idx;
        let Some(first_parent_ext) = self.ctx.arena.get_extended(current) else {
            return false;
        };
        let first_parent = first_parent_ext.parent;
        let Some(first_parent_node) = self.ctx.arena.get(first_parent) else {
            return false;
        };
        if first_parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && first_parent_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }

        let Some(first_access) = self.ctx.arena.get_access_expr(first_parent_node) else {
            return false;
        };
        if first_access.expression != current
            || !self.access_member_is_named(first_access.name_or_argument, "prototype")
        {
            return false;
        }
        current = first_parent;

        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };

            if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                && access.expression == current
            {
                current = parent;
                continue;
            }

            return parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_binary_expr(parent_node)
                    .is_some_and(|binary| {
                        binary.left == current && self.is_assignment_operator(binary.operator_token)
                    });
        }
    }

    fn access_member_is_named(&self, idx: NodeIndex, expected: &str) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return ident.escaped_text == expected;
        }

        if let Some(lit) = self.ctx.arena.get_literal(node) {
            return lit.text == expected;
        }

        false
    }

    pub(crate) fn source_file_default_import_module_named(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> Option<String> {
        if self.ctx.binder.file_import_sources.is_empty() {
            return None;
        }

        let mut current = idx;
        let mut guard = 0u32;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            guard += 1;
            if guard > 4096 {
                return None;
            }
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
            if let Some(node) = self.ctx.arena.get(current)
                && (node.kind == tsz_parser::parser::syntax_kind_ext::SOURCE_FILE
                    || node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK)
            {
                break;
            }
        }
        let root = self.ctx.arena.get(current)?;
        if root.kind != tsz_parser::parser::syntax_kind_ext::SOURCE_FILE
            && root.kind != tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
        {
            return None;
        }

        for stmt_idx in self.ctx.arena.get_children(current) {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let Some(import_decl) = self.ctx.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only {
                continue;
            }
            if clause.name.is_some()
                && self.ctx.arena.get_identifier_text(clause.name) == Some(name)
                && let Some(module_specifier) = self.get_import_module_specifier(import_decl)
                && !self.is_export_type_only_across_binders(&module_specifier, "default")
                && !self.is_module_export_equals_type_only(&module_specifier)
            {
                return Some(module_specifier);
            }
        }

        None
    }

    pub(crate) fn source_file_has_value_import_binding_named(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        let mut current = idx;
        let mut guard = 0u32;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            guard += 1;
            if guard > 4096 {
                return false;
            }
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
            if let Some(node) = self.ctx.arena.get(current)
                && (node.kind == tsz_parser::parser::syntax_kind_ext::SOURCE_FILE
                    || node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK)
            {
                break;
            }
        }
        let Some(root) = self.ctx.arena.get(current) else {
            return false;
        };
        if root.kind != tsz_parser::parser::syntax_kind_ext::SOURCE_FILE
            && root.kind != tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
        {
            return false;
        }

        for stmt_idx in self.ctx.arena.get_children(current) {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }
            let Some(import_decl) = self.ctx.arena.get_import_decl(stmt_node) else {
                continue;
            };
            // For import-equals declarations, is_type_only lives on ImportDeclData.
            if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                if import_decl.is_type_only {
                    continue;
                }
                if self
                    .ctx
                    .arena
                    .get_identifier_text(import_decl.import_clause)
                    == Some(name)
                {
                    // Check if the target module's export= is type-only
                    if let Some(module_specifier) = self.get_import_module_specifier(import_decl)
                        && self.is_module_export_equals_type_only(&module_specifier)
                    {
                        // type-only through export= chain — skip
                    } else if let Some(&alias_sym_id) = self
                        .ctx
                        .binder
                        .node_symbols
                        .get(&import_decl.import_clause.0)
                        .or_else(|| self.ctx.binder.node_symbols.get(&stmt_idx.0))
                        && self.alias_resolves_to_type_only(alias_sym_id)
                    {
                        // `import Alias = NS.Type` is not a value import when the
                        // qualified target resolves to an interface/type alias.
                    } else {
                        return true;
                    }
                }
                continue;
            }
            // For regular import declarations, is_type_only lives on ImportClauseData,
            // NOT on ImportDeclData (which is always false for regular imports).
            let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
                continue;
            };
            // Skip the entire import if the clause is type-only (`import type { ... }`)
            if clause.is_type_only {
                continue;
            }

            // Default import: `import Foo from ...`
            if clause.name.is_some()
                && self.ctx.arena.get_identifier_text(clause.name) == Some(name)
            {
                // Also check cross-file type-only chain for default imports.
                if let Some(module_specifier) = self.get_import_module_specifier(import_decl) {
                    if self.is_export_type_only_across_binders(&module_specifier, "default")
                        || self.is_module_export_equals_type_only(&module_specifier)
                    {
                        // type-only through export chain or export= chain — skip
                    } else {
                        return true;
                    }
                } else {
                    return true;
                }
            }

            // Named imports: `import { Foo } from ...`
            if clause.named_bindings.is_some()
                && let Some(named_bindings_node) = self.ctx.arena.get(clause.named_bindings)
                && (named_bindings_node.kind == tsz_parser::parser::syntax_kind_ext::NAMED_IMPORTS
                    || named_bindings_node.kind
                        == tsz_parser::parser::syntax_kind_ext::NAMESPACE_IMPORT)
                && let Some(named_imports) = self.ctx.arena.get_named_imports(named_bindings_node)
            {
                if named_bindings_node.kind == tsz_parser::parser::syntax_kind_ext::NAMESPACE_IMPORT
                {
                    if named_imports.name.is_some()
                        && self.ctx.arena.get_identifier_text(named_imports.name) == Some(name)
                    {
                        if let Some(module_specifier) =
                            self.get_import_module_specifier(import_decl)
                        {
                            if self.is_export_type_only_across_binders(&module_specifier, "*")
                                || self.is_module_export_equals_type_only(&module_specifier)
                            {
                                // type-only through export chain or export= chain — skip
                            } else {
                                return true;
                            }
                        } else {
                            return true;
                        }
                    }
                } else {
                    for &specifier_idx in &named_imports.elements.nodes {
                        let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                            continue;
                        };
                        let Some(specifier) = self.ctx.arena.get_specifier(specifier_node) else {
                            continue;
                        };
                        // Skip individual type-only specifiers (`import { type Foo, Bar }`)
                        if specifier.is_type_only {
                            continue;
                        }
                        let local_name = specifier.name;
                        if local_name.is_none() {
                            continue;
                        }
                        if self.ctx.arena.get_identifier_text(local_name) == Some(name) {
                            // Also check whether this import's target is type-only through
                            // the export chain (e.g., `import { A } from './b'` where b.ts
                            // has `export type * from './a'`). If the target export is
                            // type-only, this import doesn't provide a runtime value binding.
                            if let Some(module_specifier) =
                                self.get_import_module_specifier(import_decl)
                            {
                                // Get the original export name (before any rename).
                                // For `import { Foo as Bar }`, the export name is "Foo".
                                let export_name = if specifier.property_name.is_some() {
                                    self.ctx
                                        .arena
                                        .get_identifier_text(specifier.property_name)
                                        .unwrap_or(name)
                                } else {
                                    name
                                };
                                if self.is_export_type_only_across_binders(
                                    &module_specifier,
                                    export_name,
                                ) {
                                    continue; // type-only through export chain — skip
                                }
                            }
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// For a merged interface+value symbol (e.g. `Promise`, `Symbol`), pick the
    /// best value declaration from the list. Prefers `VariableDeclaration` nodes
    /// (corresponding to `declare var X: XConstructor`) over interface or other
    /// declaration kinds, since those carry the constructor-side type.
    pub(crate) fn preferred_value_declaration(
        &self,
        sym_id: SymbolId,
        default_decl: NodeIndex,
        declarations: &[NodeIndex],
    ) -> Option<NodeIndex> {
        // Among all declarations, prefer a VariableDeclaration with a type
        // annotation — this is the `declare var X: XConstructor` pattern that
        // gives us the constructor type for merged interface+value symbols.
        for &decl_idx in declarations {
            if decl_idx == default_decl || decl_idx.is_none() {
                continue;
            }
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                && let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
                && var_decl.type_annotation.is_some()
            {
                return Some(decl_idx);
            }
        }
        // Also check the default declaration itself
        if let Some(node) = self.ctx.arena.get(default_decl)
            && node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            return Some(default_decl);
        }
        if let Some(js_ctor_decl) =
            self.checked_js_constructor_value_declaration(sym_id, default_decl, declarations)
        {
            return Some(js_ctor_decl);
        }
        None
    }

    pub(crate) fn checked_js_constructor_value_declaration(
        &self,
        sym_id: SymbolId,
        default_decl: NodeIndex,
        declarations: &[NodeIndex],
    ) -> Option<NodeIndex> {
        if self.declaration_is_checked_js_constructor_value_declaration(sym_id, default_decl) {
            return Some(default_decl);
        }

        declarations.iter().copied().find(|&decl_idx| {
            decl_idx != default_decl
                && self.declaration_is_checked_js_constructor_value_declaration(sym_id, decl_idx)
        })
    }

    pub(crate) fn declaration_is_checked_js_constructor_value_declaration(
        &self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> bool {
        if decl_idx.is_none() {
            return false;
        }

        if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            return arenas.iter().any(|arena| {
                self.arena_has_checked_js_constructor_value_declaration(arena.as_ref(), decl_idx)
            });
        }

        self.arena_has_checked_js_constructor_value_declaration(self.ctx.arena, decl_idx)
    }

    pub(crate) fn checked_js_constructor_initializer_expression(
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        fn constructor_expr(
            arena: &tsz_parser::parser::node::NodeArena,
            idx: NodeIndex,
        ) -> Option<NodeIndex> {
            let idx = arena.skip_parenthesized_and_assertions(idx);
            let node = arena.get(idx)?;
            if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return Some(idx);
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let binary = arena.get_binary_expr(node)?;
                let op = binary.operator_token;
                if op == tsz_scanner::SyntaxKind::BarBarToken as u16
                    || op == tsz_scanner::SyntaxKind::QuestionQuestionToken as u16
                {
                    return constructor_expr(arena, binary.right)
                        .or_else(|| constructor_expr(arena, binary.left));
                }
            }
            None
        }

        if decl_idx.is_none() {
            return None;
        }

        let node = arena.get(decl_idx)?;
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = arena.get_variable_declaration(node)?;
            return constructor_expr(arena, var_decl.initializer);
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let binary = arena.get_binary_expr(node)?;
            if !crate::query_boundaries::common::is_assignment_operator(binary.operator_token) {
                return None;
            }
            return constructor_expr(arena, binary.right);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let ext = arena.get_extended(decl_idx)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent_node = arena.get(ext.parent)?;
            if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return None;
            }
            let binary = arena.get_binary_expr(parent_node)?;
            if binary.left != decl_idx
                || !crate::query_boundaries::common::is_assignment_operator(binary.operator_token)
            {
                return None;
            }
            return constructor_expr(arena, binary.right);
        }

        None
    }

    fn arena_has_checked_js_constructor_value_declaration(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        if !is_js_file_name(&source_file.file_name)
            || !should_resolve_jsdoc_for_file(
                &source_file.file_name,
                source_file.text.as_ref(),
                &self.ctx.compiler_options,
            )
        {
            return false;
        }

        let Some(node) = arena.get(decl_idx) else {
            return false;
        };
        let is_function_assignment = || -> bool {
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                return Self::checked_js_constructor_initializer_expression(arena, decl_idx)
                    .is_some();
            }

            if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                return Self::checked_js_constructor_initializer_expression(arena, decl_idx)
                    .is_some();
            }

            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                return Self::checked_js_constructor_initializer_expression(arena, decl_idx)
                    .is_some();
            }

            false
        };

        is_function_assignment()
    }

    /// Extract the module specifier string from an import declaration.
    ///
    /// Given an `ImportDeclData`, resolves the `module_specifier` node index
    /// to a string literal text value (without quotes).
    fn get_import_module_specifier(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> Option<String> {
        let spec_node = self.ctx.arena.get(import_decl.module_specifier)?;
        let literal = self.ctx.arena.get_literal(spec_node)?;
        Some(literal.text.clone())
    }
}
