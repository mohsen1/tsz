//! Name resolution error reporting (TS2304, TS2552, TS2583, TS2584)
//! and known-global classifiers for "did you mean to install @types/...?" suggestions.

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Name Resolution Errors
    // =========================================================================

    fn unresolved_name_matches_enclosing_param(&self, name: &str, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

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
            if ext.parent.is_none() {
                break;
            }
            let parent = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };

            let matches_param = match parent_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    self.ctx
                        .arena
                        .get_function(parent_node)
                        .is_some_and(|func| {
                            func.parameters.nodes.iter().any(|&param_idx| {
                                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                    return false;
                                };
                                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                    return false;
                                };
                                let Some(name_node) = self.ctx.arena.get(param.name) else {
                                    return false;
                                };
                                self.ctx
                                    .arena
                                    .get_identifier(name_node)
                                    .is_some_and(|id| id.escaped_text == name)
                            })
                        })
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(parent_node)
                    .is_some_and(|method| {
                        method.parameters.nodes.iter().any(|&param_idx| {
                            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                return false;
                            };
                            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                return false;
                            };
                            let Some(name_node) = self.ctx.arena.get(param.name) else {
                                return false;
                            };
                            self.ctx
                                .arena
                                .get_identifier(name_node)
                                .is_some_and(|id| id.escaped_text == name)
                        })
                    }),
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(parent_node)
                    .is_some_and(|ctor| {
                        ctor.parameters.nodes.iter().any(|&param_idx| {
                            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                return false;
                            };
                            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                return false;
                            };
                            let Some(name_node) = self.ctx.arena.get(param.name) else {
                                return false;
                            };
                            self.ctx
                                .arena
                                .get_identifier(name_node)
                                .is_some_and(|id| id.escaped_text == name)
                        })
                    }),
                _ => false,
            };

            if matches_param {
                return true;
            }
            current = parent;
        }

        false
    }

    /// Check if a node is in a type-annotation context (type reference, implements, extends, etc.).
    /// Used to determine which symbol meaning to use for spelling suggestions.
    fn is_in_type_context(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the AST to find if we're inside a type annotation
        let mut current = idx;
        let mut guard = 0;
        while current.is_some() {
            guard += 1;
            if guard > 64 {
                break;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    syntax_kind_ext::TYPE_REFERENCE
                    | syntax_kind_ext::HERITAGE_CLAUSE
                    | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    | syntax_kind_ext::INTERFACE_DECLARATION
                    | syntax_kind_ext::TYPE_PARAMETER
                    | syntax_kind_ext::MAPPED_TYPE
                    | syntax_kind_ext::CONDITIONAL_TYPE
                    | syntax_kind_ext::INDEXED_ACCESS_TYPE
                    | syntax_kind_ext::UNION_TYPE
                    | syntax_kind_ext::INTERSECTION_TYPE
                    | syntax_kind_ext::ARRAY_TYPE
                    | syntax_kind_ext::TUPLE_TYPE
                    | syntax_kind_ext::TYPE_LITERAL
                    | syntax_kind_ext::FUNCTION_TYPE
                    | syntax_kind_ext::CONSTRUCTOR_TYPE
                    | syntax_kind_ext::PARENTHESIZED_TYPE
                    | syntax_kind_ext::TYPE_OPERATOR
                    | syntax_kind_ext::TYPE_QUERY
                    | syntax_kind_ext::INFER_TYPE => return true,
                    syntax_kind_ext::IMPORT_CLAUSE => {
                        if self
                            .ctx
                            .arena
                            .get_import_clause(node)
                            .is_some_and(|clause| clause.is_type_only)
                        {
                            return true;
                        }
                    }
                    syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                        if self
                            .ctx
                            .arena
                            .get_import_decl(node)
                            .is_some_and(|decl| decl.is_type_only)
                        {
                            return true;
                        }
                    }
                    syntax_kind_ext::IMPORT_SPECIFIER | syntax_kind_ext::EXPORT_SPECIFIER => {
                        if self
                            .ctx
                            .arena
                            .get_specifier(node)
                            .is_some_and(|specifier| specifier.is_type_only)
                        {
                            return true;
                        }
                    }
                    syntax_kind_ext::EXPORT_DECLARATION => {
                        if self
                            .ctx
                            .arena
                            .get_export_decl(node)
                            .is_some_and(|decl| decl.is_type_only)
                        {
                            return true;
                        }
                    }
                    // Stop at expression/statement boundaries
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::VARIABLE_STATEMENT
                    | syntax_kind_ext::EXPRESSION_STATEMENT
                    | syntax_kind_ext::BLOCK
                    | syntax_kind_ext::SOURCE_FILE => return false,
                    _ => {}
                }
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

    /// Report a cannot find name error using solver diagnostics with source tracking.
    /// Enhanced to provide suggestions for similar names, import suggestions, and
    /// library change suggestions for ES2015+ types.
    pub fn error_cannot_find_name_at(&mut self, name: &str, idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;

        if self.should_suppress_unresolved_name_for_constructor_capture(name, idx) {
            return;
        }

        // Suppress TS2304/TS2552 for expression inside `export default` in a namespace.
        // TS1319 is the correct diagnostic; name resolution produces false positives.
        if self.should_suppress_name_in_export_default_namespace(idx) {
            return;
        }

        // TS1212/TS1213/TS1214: Emit strict-mode reserved word diagnostic
        // before any TS2304 suppression logic. This fires independently of TS2304.
        // tsc emits these only when the identifier is used as a DECLARATION name
        // (function name, parameter, variable binding), not when it's used as a
        // value reference in expression position. For example, `foo(public ...)` in
        // a class body should not emit TS1213 — `public` is a valid expression
        // identifier even in strict mode. Parse error recovery can also produce
        // identifier nodes from keywords that shouldn't trigger strict-mode diagnostics.
        let is_declaration_site = self.ctx.arena.get_extended(idx).is_some_and(|ext| {
            let parent = ext.parent;
            self.ctx.arena.get(parent).is_some_and(|pn| {
                use tsz_parser::parser::syntax_kind_ext;
                matches!(
                    pn.kind,
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION
                        || k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::ARROW_FUNCTION
                        || k == syntax_kind_ext::PARAMETER
                        || k == syntax_kind_ext::VARIABLE_DECLARATION
                        || k == syntax_kind_ext::CLASS_DECLARATION
                        || k == syntax_kind_ext::CLASS_EXPRESSION
                        || k == syntax_kind_ext::INTERFACE_DECLARATION
                        || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                        || k == syntax_kind_ext::ENUM_DECLARATION
                        || k == syntax_kind_ext::MODULE_DECLARATION
                        || k == syntax_kind_ext::METHOD_DECLARATION
                        || k == syntax_kind_ext::METHOD_SIGNATURE
                        || k == syntax_kind_ext::PROPERTY_DECLARATION
                        || k == syntax_kind_ext::PROPERTY_SIGNATURE
                        || k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR
                        || k == syntax_kind_ext::BINDING_ELEMENT
                        || k == syntax_kind_ext::IMPORT_SPECIFIER
                        || k == syntax_kind_ext::EXPORT_SPECIFIER
                        || k == syntax_kind_ext::TYPE_PARAMETER
                )
            })
        });
        // When an identifier is spelled with unicode escapes (e.g., \u0079ield for yield),
        // TSC treats it as a regular identifier and does NOT emit TS1212/TS1213/TS1214.
        let has_unicode_escape = self
            .ctx
            .arena
            .get(idx)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .is_some_and(|ident| ident.original_text.is_some());
        if crate::state_checking::is_strict_mode_reserved_name(name)
            && self.ctx.checking_computed_property_name.is_none()
            && is_declaration_site
            && !has_unicode_escape
        {
            // Detect class context by walking up the AST (enclosing_class may not
            // be set during type resolution or other non-statement-walk phases).
            let in_class = {
                let mut cur = idx;
                let mut found = false;
                let mut g = 0;
                while cur.is_some() {
                    g += 1;
                    if g > 256 {
                        break;
                    }
                    if let Some(n) = self.ctx.arena.get(cur) {
                        if n.kind == syntax_kind_ext::CLASS_DECLARATION
                            || n.kind == syntax_kind_ext::CLASS_EXPRESSION
                        {
                            found = true;
                            break;
                        }
                        if n.kind == syntax_kind_ext::SOURCE_FILE {
                            break;
                        }
                    }
                    let Some(ext) = self.ctx.arena.get_extended(cur) else {
                        break;
                    };
                    if ext.parent.is_none() {
                        break;
                    }
                    cur = ext.parent;
                }
                found
            };

            let is_strict = self.ctx.is_strict_mode_for_node(idx);

            // Suppress TS1212/TS1213/TS1214 when there are parse errors.
            // In error recovery, reserved words may appear as identifiers
            // in positions where they were not intended (e.g., `public` inside
            // a function call argument). tsc doesn't emit these diagnostics
            // for parser error recovery artifacts.
            if is_strict && !self.ctx.has_parse_errors {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                if in_class {
                    if name == "arguments" {
                        let message = format_message(
                            diagnostic_messages::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                            &[name],
                        );
                        self.error_at_node(
                            idx,
                            &message,
                            diagnostic_codes::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                        );
                    } else {
                        let message = format_message(
                            diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                            &[name],
                        );
                        self.error_at_node(
                            idx,
                            &message,
                            diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                        );
                    }
                } else if self.ctx.binder.is_external_module() {
                    let message = format_message(
                        diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                        &[name],
                    );
                    self.error_at_node(
                        idx,
                        &message,
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                    );
                } else {
                    let message = format_message(
                        diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                        &[name],
                    );
                    self.error_at_node(
                        idx,
                        &message,
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                    );
                }
            }
        }

        // Note: Previously we forced TS2304 for `<<T>` ambiguous generic assertions,
        // but tsc does NOT emit TS2304 for these (only TS1005/TS1109).

        // NOTE: `symbol` is intentionally excluded — tsc never emits TS2693 for
        // lowercase `symbol`. Instead it emits TS2552 "Cannot find name 'symbol'.
        // Did you mean 'Symbol'?" via the normal spelling-suggestion path.
        let is_primitive_type_keyword = matches!(
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
        );
        let is_import_equals_module_specifier = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent))
            .is_some_and(|parent_node| {
                if parent_node.kind
                    != tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    return false;
                }
                self.ctx
                    .arena
                    .get_import_decl(parent_node)
                    .is_some_and(|imp| imp.module_specifier == idx)
            });

        if is_primitive_type_keyword && !is_import_equals_module_specifier {
            self.error_type_only_value_at(name, idx);
            return;
        }

        if self.unresolved_name_matches_enclosing_param(name, idx) {
            return;
        }

        // Suppress TS2304 for `intrinsic` when it is the bare, unparenthesized direct body
        // of a type alias. TS2795 is emitted separately; TSC never also emits TS2304 in this
        // position. However, when parenthesized like `type TE1 = (intrinsic)`, TSC treats it
        // as a regular identifier and DOES emit TS2304.
        // `idx` is the IDENTIFIER node; parent = TypeReference; grandparent = TypeAliasDeclaration.
        if name == "intrinsic" {
            let ext0 = self.ctx.arena.get_extended(idx);
            // Get the TYPE_REFERENCE parent node (to check source position)
            let type_ref_parent = ext0.and_then(|e| self.ctx.arena.get(e.parent));
            let p1_kind = ext0
                .and_then(|e| self.ctx.arena.get_extended(e.parent))
                .and_then(|e2| self.ctx.arena.get(e2.parent))
                .map(|n| n.kind);
            let is_type_alias_body =
                p1_kind.is_some_and(|k| k == syntax_kind_ext::TYPE_ALIAS_DECLARATION);
            if is_type_alias_body {
                // Check that it's not parenthesized
                let is_parenthesized = type_ref_parent.is_some_and(|tr_node| {
                    if let Some(sf) = self.ctx.arena.source_files.first() {
                        let pos = tr_node.pos as usize;
                        if pos > 0 {
                            let before = &sf.text[..pos];
                            let last_non_ws = before
                                .bytes()
                                .rev()
                                .find(|&b| b != b' ' && b != b'\t' && b != b'\n' && b != b'\r');
                            return last_non_ws == Some(b'(');
                        }
                    }
                    false
                });
                if !is_parenthesized {
                    return;
                }
            }
        }

        // In `import x = <expr>` module reference position, unresolved names should
        // report namespace/module diagnostics (TS2503/TS2307), not TS2304.
        let mut cur = idx;
        while let Some(ext) = self.ctx.arena.get_extended(cur) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                return;
            }
            cur = parent;
        }

        // Skip TS2304 for identifiers that are clearly not valid names.
        // These are likely parse errors (e.g., ",", ";", "(", or empty names) that were
        // added to the AST for error recovery. The parse error should have
        // already been emitted (e.g., TS1003 "Identifier expected").
        if name.is_empty() {
            return;
        }
        let is_obviously_invalid = name.len() == 1
            && matches!(
                name.chars().next(),
                Some(
                    ',' | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '%'
                        | '&'
                        | '|'
                        | '^'
                        | '!'
                        | '~'
                        | '<'
                        | '>'
                        | '='
                        | '.'
                )
            );
        if is_obviously_invalid {
            return;
        }

        // Detect computed property name context: class/object vs enum.
        // tsc emits TS2304 for computed property expressions in class/object-literal
        // contexts, but NOT in enum contexts (only TS1164 is emitted for enum computed names).
        let computed_ctx = self.ctx.arena.get_extended(idx).and_then(|ext| {
            let parent = self.ctx.arena.get(ext.parent)?;
            if parent.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                return None;
            }
            let gp_ext = self.ctx.arena.get_extended(ext.parent)?;
            let gp = self.ctx.arena.get(gp_ext.parent)?;
            if gp.kind == syntax_kind_ext::ENUM_MEMBER {
                Some(false) // enum context: suppress TS2304/TS2552
            } else {
                Some(true) // class/object context: allow TS2304
            }
        });
        // Suppress TS2304/TS2552 for identifiers inside enum computed property names.
        // tsc only emits TS1164 for these and doesn't resolve the expressions.
        if computed_ctx == Some(false) {
            return;
        }
        // Fallback: if TS1164 (computed property not allowed in enum) was already
        // emitted covering this identifier's position, suppress TS2304/TS2552.
        // This catches cases where get_extended parent info is unavailable.
        if computed_ctx.is_none()
            && let Some(node) = self.ctx.arena.get(idx)
        {
            let has_1164 =
                self.ctx.diagnostics.iter().any(|d| {
                    d.code == 1164 && d.start <= node.pos && d.start + d.length >= node.pos
                });
            if has_1164 {
                return;
            }
        }
        let is_in_computed_property = computed_ctx == Some(true);
        // When there are parse errors, modifier keywords appearing as identifiers
        // are parser-recovery artifacts. Suppress TS2304 for these to avoid cascades.
        // Exception: computed property name expressions like `[public]` — tsc emits
        // TS2304 for these even in parse-error contexts (but not in enums).
        if self.has_parse_errors()
            && !is_in_computed_property
            && matches!(
                name,
                "static"
                    | "public"
                    | "private"
                    | "protected"
                    | "readonly"
                    | "abstract"
                    | "declare"
                    | "override"
                    | "accessor"
            )
        {
            return;
        }

        // In parse-error files, identifiers inside class member bodies are often
        // secondary errors from a primary parse error (e.g. `yield foo` in a non-generator
        // constructor — TSC emits TS1163 but not TS2304 for `foo`).
        // Suppress TS2304 when there is a parse error at or just before the identifier
        // (within ~10 chars) AND the identifier is in a class member body.
        // This targets cases like `yield foo` where the error (on `yield`) immediately
        // precedes the identifier (foo). It does NOT suppress cases like `if (a` where
        // the parse error (missing `)`) comes AFTER the identifier `a`.
        if self.has_syntax_parse_errors()
            && !is_in_computed_property
            && !self.ctx.syntax_parse_error_positions.is_empty()
            && let Some(node) = self.ctx.arena.get(idx)
        {
            let ident_pos = node.pos;
            let has_nearby_preceding_error =
                self.ctx
                    .syntax_parse_error_positions
                    .iter()
                    .any(|&err_pos| {
                        // Error must be BEFORE the identifier (not after) and within 10 chars
                        err_pos <= ident_pos && (ident_pos - err_pos) <= 10
                    });
            if has_nearby_preceding_error {
                let mut current = idx;
                let mut guard = 0;
                let mut in_class = false;
                let mut in_class_member_body = false;
                while current.is_some() {
                    guard += 1;
                    if guard > 256 {
                        break;
                    }
                    let Some(inner_node) = self.ctx.arena.get(current) else {
                        break;
                    };
                    if inner_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        || inner_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    {
                        in_class = true;
                    }
                    if inner_node.kind == syntax_kind_ext::CONSTRUCTOR
                        || inner_node.kind == syntax_kind_ext::METHOD_DECLARATION
                        || inner_node.kind == syntax_kind_ext::GET_ACCESSOR
                        || inner_node.kind == syntax_kind_ext::SET_ACCESSOR
                    {
                        in_class_member_body = true;
                    }
                    let Some(ext) = self.ctx.arena.get_extended(current) else {
                        break;
                    };
                    if ext.parent.is_none() {
                        break;
                    }
                    current = ext.parent;
                }
                if in_class && in_class_member_body {
                    return;
                }
            }
        }

        // tsc propagates `ThisNodeHasError` / `ThisNodeOrAnySubNodesHasError`
        // flags through the AST when parse errors occur. The checker skips
        // semantic resolution for error-flagged nodes, effectively suppressing
        // TS2304 for identifiers in or near error-recovery AST regions.
        //
        // Since our parser uses a compact u16 flags field that cannot store
        // bit-18 error flags, we approximate tsc's behavior with a file-level
        // check: when the file has "real" syntax errors (TS1005/TS1109/TS1127
        // etc. that indicate actual parse failure), suppress TS2304 broadly.
        //
        // Grammar-only errors (TS1100, TS1173, TS1212) should NOT suppress
        // TS2304 — tsc still emits TS2304 in those files.
        if self.ctx.has_real_syntax_errors {
            return;
        }

        // In JavaScript files, suppress TS2304 for names inside syntactic type
        // annotations (return types, parameter types, etc.). tsc emits TS8010
        // ("Type annotations can only be used in TypeScript files") for these but
        // does NOT attempt name resolution within them.
        // JSDoc type annotations are NOT syntactic type nodes, so they are unaffected.
        if self.is_js_file() && self.is_in_syntactic_type_node(idx) {
            return;
        }

        // In files with real syntax errors, unresolved names inside `typeof` type queries
        // are often cascades from malformed declaration syntax; TypeScript commonly keeps
        // the primary parse diagnostic only for these.
        // Only suppress when a parse error falls directly within the identifier's span.
        // The wider `node_has_nearby_parse_error` margin (8 bytes) would incorrectly
        // suppress TS2304 for `A` in `typeof A.` where the error is the missing
        // identifier after the dot, not A itself.
        if self.has_syntax_parse_errors() && self.node_span_contains_parse_error(idx) {
            let mut current = idx;
            let mut guard = 0;
            while current.is_some() {
                guard += 1;
                if guard > 256 {
                    break;
                }
                if let Some(node) = self.ctx.arena.get(current)
                    && node.kind == syntax_kind_ext::TYPE_QUERY
                {
                    return;
                }
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            }
        }

        if let Some(original_name) =
            self.unresolved_unused_renaming_property_in_type_query(name, idx)
        {
            let message = format!(
                "'{name}' is an unused renaming of '{original_name}'. Did you intend to use it as a type annotation?"
            );
            self.error_at_node(
                idx,
                &message,
                diagnostic_codes::IS_AN_UNUSED_RENAMING_OF_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION,
            );
            return;
        }

        // Route through the environment capability boundary for known globals.
        // `diagnose_missing_name` returns a structured CapabilityDiagnostic that
        // tells us which error to emit, keeping the decision centralized.
        use crate::query_boundaries::environment::CapabilityDiagnostic;
        if let Some(cap_diag) = self.ctx.capabilities.diagnose_missing_name(name) {
            match cap_diag {
                CapabilityDiagnostic::MissingEs2015Type { .. } => {
                    self.error_cannot_find_name_change_lib(name, idx);
                    return;
                }
                CapabilityDiagnostic::MissingDomGlobal { .. } => {
                    self.error_cannot_find_name_change_target_lib(name, idx);
                    return;
                }
                CapabilityDiagnostic::MissingJQueryGlobal { .. } => {
                    self.error_cannot_find_name_install_jquery_types(name, idx);
                    return;
                }
                CapabilityDiagnostic::MissingNodeGlobal { .. } => {
                    // Special cases: private-name access and "module" with parse errors
                    // fall through to TS2304 instead of TS2591.
                    if self.is_private_name_access_base(idx)
                        || (name == "module" && self.has_parse_errors())
                    {
                        // Fall through to TS2304
                    } else {
                        self.error_cannot_find_name_install_node_types(name, idx);
                        return;
                    }
                }
                CapabilityDiagnostic::MissingTestRunnerGlobal { .. } => {
                    self.error_cannot_find_name_install_test_types(name, idx);
                    return;
                }
                CapabilityDiagnostic::MissingBunGlobal { .. } => {
                    self.error_cannot_find_name_install_bun_types(name, idx);
                    return;
                }
                // MissingGlobalType and FeatureRequiresGlobalType are handled by
                // check_missing_global_types, not the name resolution path.
                _ => {}
            }
        }

        // Suggestion collection and diagnostic emission are now handled by
        // `collect_spelling_suggestions` + the boundary's `report_name_resolution_failure`.
        // Attempt to collect suggestions and emit the appropriate diagnostic.
        let suggestions = self.collect_spelling_suggestions(name, idx);
        if !suggestions.is_empty() {
            self.error_cannot_find_name_with_suggestions(name, &suggestions, idx);
            return;
        }

        // tsc increments suggestionCount unconditionally for every name resolution
        // failure, not just when a suggestion is found. This ensures the cap of 10
        // counts all resolution attempts, matching tsc's behavior.
        self.ctx.spelling_suggestions_emitted += 1;

        // Fall back to standard error without suggestions
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.cannot_find_name(name, loc.start, loc.length());
            self.ctx
                .push_diagnostic(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Collect spelling suggestions for an unresolved name, respecting all
    /// suppression rules (accessibility modifiers, `arguments`,
    /// max-suggestion cap, parse-error suppression).
    ///
    /// Returns an empty `Vec` when suggestions should be suppressed.
    /// This is the single source of truth for suggestion collection, shared by
    /// both `error_cannot_find_name_at` and the boundary's
    /// `report_not_found_at_boundary`.
    pub(crate) fn collect_spelling_suggestions(&self, name: &str, idx: NodeIndex) -> Vec<String> {
        // Keep TS2304 for accessibility modifier keywords recovered as identifiers.
        // tsc does not emit TS2552 suggestions (e.g. "private" -> "print") in these cases.
        let is_accessibility_modifier_name = matches!(name, "public" | "private" | "protected");
        // Keep TS2304 (no TS2552 suggestion) for `arguments` lookups.
        // TypeScript does not offer spelling suggestions for unresolved `arguments`.
        let is_arguments_name = name == "arguments";
        let suppress_spelling_suggestion = is_accessibility_modifier_name || is_arguments_name;

        if suppress_spelling_suggestion {
            return Vec::new();
        }

        let reached_max_suggestions = self.ctx.spelling_suggestions_emitted >= 10;
        if reached_max_suggestions {
            return Vec::new();
        }

        // Suppress spelling suggestions in files with parse errors.
        // When the AST is malformed, symbols may not be properly bound and
        // name resolution cascades are unhelpful.  tsc keeps only primary
        // diagnostics in these files.
        if self.has_syntax_parse_errors() {
            return Vec::new();
        }

        // Determine spelling suggestion meaning based on context.
        // In type positions (type annotations, implements clauses, type references),
        // only suggest TYPE-meaning symbols. In value positions, suggest VALUE symbols.
        // This matches tsc's getSpellingSuggestionForName behavior.
        let suggestion_meaning = if self.is_in_type_context(idx) {
            tsz_binder::symbol_flags::TYPE
        } else {
            tsz_binder::symbol_flags::VALUE
        };

        self.find_similar_identifiers(name, idx, suggestion_meaning)
            .unwrap_or_default()
    }

    /// Report TS2318: Cannot find global type 'X' at a raw source position.
    ///
    /// Used when no AST node is available (e.g., missing core global types
    /// detected during initialization, where TSC emits at position 0).
    /// Routes through `push_diagnostic` for consistent deduplication of
    /// multiple TS2318 errors at position 0.
    pub fn error_global_type_missing_at_position(
        &mut self,
        type_name: &str,
        file_name: String,
        start: u32,
        length: u32,
    ) {
        use tsz_binder::lib_loader;
        let diag = lib_loader::emit_error_global_type_missing(type_name, file_name, start, length);
        self.ctx.push_diagnostic(diag);
    }

    /// Report TS2304: Cannot find name 'X' at a raw source position.
    ///
    /// Used when no AST node is available (e.g., JSDoc comment scanning where
    /// positions are computed from comment text offsets rather than AST nodes).
    pub fn error_cannot_find_name_at_position(&mut self, name: &str, start: u32, length: u32) {
        use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages};
        let message = diagnostic_messages::CANNOT_FIND_NAME.replace("{0}", name);
        self.ctx.push_diagnostic(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            message,
            diagnostic_codes::CANNOT_FIND_NAME,
        ));
    }

    /// Report error 2318/2583: Cannot find global type 'X'.
    /// - TS2318: Cannot find global type (for @noLib tests)
    /// - TS2583: Cannot find name - suggests changing target library (for ES2015+ types)
    pub fn error_cannot_find_global_type(&mut self, name: &str, idx: NodeIndex) {
        use tsz_binder::lib_loader;

        // Check if this is an ES2015+ type that would require a specific lib
        let is_es2015_type = lib_loader::is_es2015_plus_type(name);

        let (code, message) = if is_es2015_type {
            let lib_version = lib_loader::get_suggested_lib_for_type(name);
            (
                lib_loader::MISSING_ES2015_LIB_SUPPORT,
                format!(
                    "Cannot find name '{name}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{lib_version}' or later."
                ),
            )
        } else {
            (
                lib_loader::CANNOT_FIND_GLOBAL_TYPE,
                format!("Cannot find global type '{name}'."),
            )
        };

        self.error_at_node(idx, &message, code);
    }

    /// Report TS2583: Cannot find name 'X' - suggest changing target library.
    ///
    /// This error is emitted when an ES2015+ global (Promise, Map, Set, Symbol, etc.)
    /// is used as a value but is not available in the current lib configuration.
    /// It provides a helpful suggestion to change the lib compiler option.
    pub fn error_cannot_find_name_change_lib(&mut self, name: &str, idx: NodeIndex) {
        use tsz_binder::lib_loader;
        let lib_version = lib_loader::get_suggested_lib_for_type(name);
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB,
            &[name, lib_version],
        );
    }

    /// Report TS2584: Cannot find name 'X' - suggest including 'dom' lib.
    ///
    /// This error is emitted when a known DOM/ScriptHost global (console, window,
    /// document, `HTMLElement`, etc.) is used but the 'dom' lib is not included.
    pub fn error_cannot_find_name_change_target_lib(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2,
            &[name],
        );
    }

    /// Report TS2591: Cannot find name 'X' - suggest installing @types/node
    /// and adding 'node' to the types field in tsconfig.
    /// tsc 6.0 defaults to TS2591 (with tsconfig suggestion) in nearly all cases.
    pub fn error_cannot_find_name_install_node_types(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
            &[name],
        );
    }

    /// Report TS2592: Cannot find name 'X' - suggest installing @types/jquery
    /// and adding 'jquery' to the types field in tsconfig.
    pub fn error_cannot_find_name_install_jquery_types(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA_2,
            &[name],
        );
    }

    /// Report TS2593: Cannot find name 'X' - suggest installing test runner types
    /// and adding to the types field in tsconfig.
    pub fn error_cannot_find_name_install_test_types(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N_2,
            &[name],
        );
    }

    /// Report TS2868: Cannot find name 'Bun' - suggest installing @types/bun
    /// and adding 'bun' to the types field in tsconfig.
    pub fn error_cannot_find_name_install_bun_types(&mut self, name: &str, idx: NodeIndex) {
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_BUN_TRY_NPM_I_SAVE_2,
            &[name],
        );
    }

    /// Report error 2304/2552: Cannot find name 'X' with suggestions.
    /// Provides a list of similar names that might be what the user intended.
    /// tsc limits spelling suggestions to 10 per file; after that, emits TS2304 only.
    pub fn error_cannot_find_name_with_suggestions(
        &mut self,
        name: &str,
        suggestions: &[String],
        idx: NodeIndex,
    ) {
        // Suppress TS2552 for expression inside `export default` in a namespace,
        // mirroring the TS2304 suppression in error_cannot_find_name_at.
        // TS1319 is the correct diagnostic; name resolution produces false positives.
        if self.should_suppress_name_in_export_default_namespace(idx) {
            return;
        }

        // tsc caps spelling suggestions at 10 per file.
        if self.ctx.spelling_suggestions_emitted >= 10 {
            self.error_at_node(
                idx,
                &format!("Cannot find name '{name}'."),
                diagnostic_codes::CANNOT_FIND_NAME,
            );
            return;
        }
        self.ctx.spelling_suggestions_emitted += 1;

        // Skip TS2304 for identifiers that are clearly not valid names.
        // These are likely parse errors that were added to the AST for error recovery.
        let is_obviously_invalid = name.len() == 1
            && matches!(
                name.chars().next(),
                Some(
                    ',' | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '%'
                        | '&'
                        | '|'
                        | '^'
                        | '!'
                        | '~'
                        | '<'
                        | '>'
                        | '='
                        | '.'
                )
            );
        if is_obviously_invalid {
            return;
        }

        // Format the suggestions list
        let suggestions_text = if suggestions.len() == 1 {
            format!("'{}'", suggestions[0])
        } else {
            let formatted: Vec<String> = suggestions.iter().map(|s| format!("'{s}")).collect();
            formatted.join(", ")
        };

        let message = if suggestions.len() == 1 {
            format!("Cannot find name '{name}'. Did you mean {suggestions_text}?")
        } else {
            format!("Cannot find name '{name}'. Did you mean one of: {suggestions_text}?")
        };

        let code = if suggestions.len() == 1 {
            diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
        } else {
            diagnostic_codes::CANNOT_FIND_NAME
        };

        self.error_at_node(idx, &message, code);
    }

    /// Report error 2552: Cannot find name 'X'. Did you mean 'Y'?
    pub fn error_cannot_find_name_did_you_mean_at(
        &mut self,
        name: &str,
        suggestion: &str,
        idx: NodeIndex,
    ) {
        let message = format!("Cannot find name '{name}'. Did you mean '{suggestion}'?");
        self.error_at_node(
            idx,
            &message,
            diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN,
        );
    }

    /// Report error 2662: Cannot find name 'X'. Did you mean the static member 'C.X'?
    pub fn error_cannot_find_name_static_member_at(
        &mut self,
        name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        let message = format!(
            "Cannot find name '{name}'. Did you mean the static member '{class_name}.{name}'?"
        );
        self.error_at_node(
            idx,
            &message,
            diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER,
        );
    }

    /// Check whether `idx` is inside a syntactic type node (`TYPE_REFERENCE`,
    /// `UNION_TYPE`, etc. — kinds 183..=206). This covers inline type annotations
    /// in the AST but NOT JSDoc type annotations, which are separate nodes.
    fn is_in_syntactic_type_node(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = idx;
        let mut guard = 0;
        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                let k = node.kind;
                // Syntactic type node range: TYPE_PREDICATE (183) .. IMPORT_TYPE (206)
                if (syntax_kind_ext::TYPE_PREDICATE..=syntax_kind_ext::IMPORT_TYPE).contains(&k) {
                    return true;
                }
                // Also check TYPE_ASSERTION (217) — `<Foo>expr` in .js
                if k == syntax_kind_ext::TYPE_ASSERTION {
                    return true;
                }
                // Stop at statement/declaration boundaries
                if k == syntax_kind_ext::SOURCE_FILE
                    || k == syntax_kind_ext::BLOCK
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                {
                    return false;
                }
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

    /// Report TS2503 or TS2833 for a missing namespace.
    /// If a similar namespace name is found in scope, emits TS2833
    /// ("Cannot find namespace 'X'. Did you mean 'Y'?") instead of TS2503.
    pub(crate) fn error_cannot_find_namespace_with_suggestion(
        &mut self,
        name: &str,
        idx: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_binder::symbol_flags;

        if name.is_empty() {
            return;
        }

        if !self.has_syntax_parse_errors()
            && let Some(suggestions) = self.find_similar_identifiers(
                name,
                idx,
                symbol_flags::NAMESPACE | symbol_flags::TYPE,
            )
            && let Some(suggestion) = suggestions.first()
        {
            self.error_at_node_msg(
                idx,
                diagnostic_codes::CANNOT_FIND_NAMESPACE_DID_YOU_MEAN,
                &[name, suggestion],
            );
            return;
        }

        self.error_at_node_msg(idx, diagnostic_codes::CANNOT_FIND_NAMESPACE, &[name]);
    }
}

// =============================================================================
// Known-Global Classifiers — CANONICAL LOCATION: query_boundaries/capabilities.rs
// =============================================================================
// The classifier functions (is_known_dom_global, is_known_node_global, etc.)
// have been consolidated in crate::query_boundaries::capabilities.
// The EnvironmentCapabilities::classify_missing_global() method is the single
// decision point for routing unresolved names to diagnostic families.
