use super::state::*;
use crate::parser::node::*;
use crate::parser::parse_rules::*;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::interner::AstAtom;
use tsz_scanner::{SyntaxKind, keyword_text_len};

/// Which grammar diagnostic `checkGrammarModifiers` picks for the modifier run
/// `[abstract, export]` depends on the node kind `export` decorates, not on the
/// statement's container alone — so `abstract export ...` cannot reuse the
/// container split the sibling modifier paths use.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AbstractExportTarget {
    /// `abstract export class C {}` — `abstract` is legal on a class, so the
    /// only violation left is the modifier ordering: TS1029 on the `export`
    /// keyword, in every container, and it outranks the container check.
    Class,
    /// A declaration that takes `export` as a modifier but admits no
    /// `abstract` (`const`/`let`/`var`/`function`/`interface`/`type`/`enum`).
    /// TS1242 outside a Block, the generic TS1184 inside one.
    ModifierRun,
    /// A form that reports its own position error inside a Block and therefore
    /// suppresses the modifier diagnostic there — `export namespace N {}`
    /// (TS1235), `export { }` and `export * from "m"` (TS1233). Outside a Block
    /// the modifier diagnostic is reported as usual.
    PositionErrorWins,
}

impl ParserState {
    pub(crate) fn parse_statement_async_declaration_or_expression(&mut self) -> NodeIndex {
        if self.look_ahead_is_async_function() {
            self.parse_async_function_declaration()
        } else if self.look_ahead_is_async_declaration() {
            let start_pos = self.token_pos();
            let async_start = self.token_pos();
            self.parse_expected(SyntaxKind::AsyncKeyword);
            let async_end = self.token_end();
            let async_modifier =
                self.arena
                    .add_token(SyntaxKind::AsyncKeyword as u16, async_start, async_end);
            self.parse_accessor_modified_statement(start_pos, vec![async_modifier])
        } else {
            self.parse_expression_statement()
        }
    }

    pub(crate) fn parse_statement_abstract_keyword(&mut self) -> NodeIndex {
        if self.next_token_is_on_new_line() {
            self.parse_expression_statement()
        } else if self.look_ahead_is_abstract_class() {
            self.parse_abstract_class_declaration()
        } else if self.look_ahead_is_abstract_declaration() {
            use tsz_common::diagnostics::diagnostic_codes;
            // TSC gives TS1242 specifically for 'abstract' before non-class declarations
            self.parse_error_at_current_token(
                "'abstract' modifier can only appear on a class, method, or property declaration.",
                diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
            );
            self.next_token();
            match self.token() {
                SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
                SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
                SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::GlobalKeyword => {
                    if self.look_ahead_is_module_declaration() {
                        self.parse_module_declaration()
                    } else {
                        self.parse_expression_statement()
                    }
                }
                _ => self.parse_expression_statement(),
            }
        } else if self.look_ahead_is_abstract_before_export_as_namespace() {
            // `abstract export as namespace Foo;` — the resulting
            // `NamespaceExportDeclaration` admits no modifiers in any container,
            // unlike the sibling `abstract` var/function declarations handled
            // just below, which split their diagnostic by container. tsc reports
            // TS1184 across the whole statement unconditionally and still parses
            // the namespace export (#16389). The other `abstract export ...`
            // forms (const/class/function/...) are not covered by this branch —
            // `abstract` is a legal modifier on some of those node kinds and tsc
            // picks a different diagnostic there, still open as a separate gap.
            let start_pos = self.token_pos();
            self.parse_expected(SyntaxKind::AbstractKeyword);
            self.parse_expected(SyntaxKind::ExportKeyword);
            let node = self.parse_namespace_export_declaration(start_pos);
            if let Some(n) = self.arena.get(node) {
                let (span_start, span_end) = (n.pos, n.end);
                self.parse_error_at(
                    span_start,
                    span_end - span_start,
                    "Modifiers cannot appear here.",
                    diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                );
            }
            node
        } else if let Some(target) = self.look_ahead_abstract_before_export_target() {
            // `abstract export <declaration>` — `export` here is a *modifier* on
            // the trailing declaration, so tsc reads one modifier run
            // `[abstract, export]` and `checkGrammarModifiers` reports exactly
            // one diagnostic for it, chosen by the node kind `export`
            // decorates. Without this branch `abstract` degraded to an
            // identifier expression and tsz reported a spurious TS2304 on top
            // of the wrong grammar code (#16389's handoff).
            let abstract_start = self.token_pos();
            let abstract_end = self.token_end();
            let in_block = self.in_block_context() || self.in_static_block_context();
            match target {
                // `abstract` is a legal modifier on a class, so the only
                // violation left is the ordering one, and it outranks the
                // container check in every container.
                AbstractExportTarget::Class => {}
                // The trailing form reports its own position error inside a
                // Block (TS1235 / TS1233) and no modifier error at all.
                AbstractExportTarget::PositionErrorWins if in_block => {}
                // Everything else follows the same container split the sibling
                // `abstract` var/function path uses (#16368/#16375): a Block
                // body gets the generic TS1184, a module/namespace body or the
                // source file top level keeps the specific TS1242.
                _ if in_block => {
                    self.parse_error_at(
                        abstract_start,
                        abstract_end - abstract_start,
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
                _ => {
                    self.parse_error_at(
                        abstract_start,
                        abstract_end - abstract_start,
                        "'abstract' modifier can only appear on a class, method, or property declaration.",
                        diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
                    );
                }
            }
            self.parse_expected(SyntaxKind::AbstractKeyword);
            if target == AbstractExportTarget::Class {
                // tsc anchors TS1029 on the *later* of the two modifiers, i.e.
                // the `export` keyword now sitting at the current token.
                let export_start = self.token_pos();
                let export_end = self.token_end();
                self.parse_error_at(
                    export_start,
                    export_end - export_start,
                    &tsz_common::diagnostics::diagnostic_messages::MODIFIER_MUST_PRECEDE_MODIFIER
                        .replace("{0}", "export")
                        .replace("{1}", "abstract"),
                    diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                );
            }
            let abstract_modifier = self.arena.add_token(
                SyntaxKind::AbstractKeyword as u16,
                abstract_start,
                abstract_end,
            );
            self.parse_accessor_modified_statement(abstract_start, vec![abstract_modifier])
        } else if self.look_ahead_is_abstract_before_var_or_function() {
            use tsz_common::diagnostics::diagnostic_codes;
            // `abstract` before a variable or function declaration
            // (`abstract const x = 1;`, `abstract function f() {}`): tsc parses
            // `abstract` as a modifier and reports a diagnostic at the keyword,
            // then parses the trailing declaration with the (invalid) modifier
            // — it does NOT degrade to an identifier expression (which would
            // give a spurious TS2304). Route through the shared
            // modifier-prefixed statement parser so the declaration is still
            // produced.
            //
            // tsc's grammar check picks the message from the statement's
            // container, the same split `parse_statement_top_level_modifier`
            // uses for the sibling modifiers (#16368/#16375): a Block body
            // (function body, a nested block, or a class static block) gets
            // the generic TS1184; a module/namespace body or the source
            // file's own top level, neither of which is a Block, keeps the
            // specific TS1242.
            let abstract_start = self.token_pos();
            let abstract_modifier = if self.in_block_context() || self.in_static_block_context() {
                self.consume_modifier_with_error(
                    SyntaxKind::AbstractKeyword,
                    "Modifiers cannot appear here.",
                    diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                )
            } else {
                self.consume_modifier_with_error(
                    SyntaxKind::AbstractKeyword,
                    "'abstract' modifier can only appear on a class, method, or property declaration.",
                    diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
                )
            };
            self.parse_accessor_modified_statement(abstract_start, vec![abstract_modifier])
        } else {
            // When 'abstract' at statement level is followed by '@' on the same line,
            // tsc emits TS1434 "Unexpected keyword or identifier." at the 'abstract' position,
            // then falls through to parse 'abstract' as an expression statement.
            if look_ahead_is(&mut self.scanner, self.current_token, |t| {
                t == SyntaxKind::AtToken
            }) {
                self.parse_error_at_current_token(
                    "Unexpected keyword or identifier.",
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
            }
            self.parse_expression_statement()
        }
    }

    pub(crate) fn parse_statement_accessor_keyword(&mut self) -> NodeIndex {
        if self.look_ahead_is_accessor_declaration() {
            use tsz_common::diagnostics::diagnostic_codes;
            // tsc emits TS1275 via grammarErrorOnNode for the `accessor` modifier
            // on any non-property-declaration node (top-level class/interface/var/...).
            let start_pos = self.token_pos();
            self.parse_error_at_current_token(
                "'accessor' modifier can only appear on a property declaration.",
                diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
            );
            let accessor_start = self.token_pos();
            self.parse_expected(SyntaxKind::AccessorKeyword);
            let accessor_end = self.token_end();
            let accessor_modifier = self.arena.add_token(
                SyntaxKind::AccessorKeyword as u16,
                accessor_start,
                accessor_end,
            );
            self.parse_accessor_modified_statement(start_pos, vec![accessor_modifier])
        } else {
            self.parse_expression_statement()
        }
    }

    fn parse_accessor_modified_statement(
        &mut self,
        start_pos: u32,
        modifiers: Vec<NodeIndex>,
    ) -> NodeIndex {
        match self.token() {
            SyntaxKind::AsyncKeyword if self.look_ahead_is_async_function() => {
                self.parse_expected(SyntaxKind::AsyncKeyword);
                self.parse_function_declaration_with_async(
                    true,
                    Some(Self::make_node_list(modifiers)),
                )
            }
            SyntaxKind::FunctionKeyword => self.parse_function_declaration_with_async(
                false,
                Some(Self::make_node_list(modifiers)),
            ),
            SyntaxKind::ClassKeyword => self.parse_class_declaration_with_modifiers(
                start_pos,
                Some(Self::make_node_list(modifiers)),
            ),
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration_with_modifiers(
                start_pos,
                Some(Self::make_node_list(modifiers)),
            ),
            SyntaxKind::TypeKeyword => self.parse_type_alias_declaration_with_modifiers(
                start_pos,
                Some(Self::make_node_list(modifiers)),
            ),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration_with_modifiers(
                start_pos,
                Some(Self::make_node_list(modifiers)),
            ),
            SyntaxKind::NamespaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::GlobalKeyword => self.parse_module_declaration_with_modifiers(
                start_pos,
                Some(Self::make_node_list(modifiers)),
            ),
            SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::UsingKeyword
            | SyntaxKind::AwaitKeyword => self.parse_variable_statement_with_modifiers(
                Some(start_pos),
                Some(Self::make_node_list(modifiers)),
            ),
            SyntaxKind::ImportKeyword => {
                if self.look_ahead_is_import_equals() {
                    self.parse_import_equals_declaration_with_modifiers(
                        start_pos,
                        Some(Self::make_node_list(modifiers)),
                    )
                } else {
                    self.parse_import_declaration_with_modifiers(
                        start_pos,
                        Some(Self::make_node_list(modifiers)),
                    )
                }
            }
            SyntaxKind::DeclareKeyword => self.parse_ambient_declaration_with_modifiers(modifiers),
            SyntaxKind::ExportKeyword => {
                if self.look_ahead_export_starts_export_declaration() {
                    return self.parse_export_declaration();
                }
                let export_start = self.token_pos();
                self.parse_expected(SyntaxKind::ExportKeyword);
                let export_end = self.token_end();
                let export_modifier = self.arena.add_token(
                    SyntaxKind::ExportKeyword as u16,
                    export_start,
                    export_end,
                );
                let mut export_modifiers = modifiers;
                export_modifiers.push(export_modifier);
                self.parse_accessor_modified_statement(start_pos, export_modifiers)
            }
            _ => self.parse_statement(),
        }
    }

    fn look_ahead_export_starts_export_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result = matches!(
            self.token(),
            SyntaxKind::OpenBraceToken
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::AsteriskToken
                | SyntaxKind::EqualsToken
                | SyntaxKind::AsKeyword
        );
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    pub(crate) fn parse_statement_top_level_modifier(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.next_token_is_on_new_line() {
            self.parse_expression_statement()
        } else if self.look_ahead_is_modifier_before_declaration() {
            if self.look_ahead_next_token_is_export_keyword() {
                // Modifier keyword followed by `export as namespace ...`:
                // TSC silently accepts the modifier and parses the export statement.
                // e.g., `static export as namespace Foo;` → no error.
                self.next_token();
                self.parse_statement()
            } else {
                // tsc's grammar check picks the message from the statement's
                // container, not the modifier itself: a Block body (function
                // body, a nested block, or a class static block) gets the
                // generic TS1184; a module/namespace body or the source
                // file's own top level, neither of which is a Block, keeps
                // the module/namespace-specific TS1044 (#16368).
                //
                // `in_static_block_context()` covers the static-block case
                // directly rather than through `in_block_context()`
                // (`parse_static_block` does not set CONTEXT_FLAG_IN_BLOCK,
                // deliberately: doing so also makes the class-body nested-
                // block recovery heuristic a few lines up in
                // `parse_statements` fire inside static blocks, which is a
                // separate, pre-existing bug — confirmed it already
                // misparses a plain method body the same way, unrelated to
                // this fix — and out of scope here).
                let modifier_start = self.token_pos();
                let modifier_text = self.scanner.get_token_text();
                if self.in_block_context() || self.in_static_block_context() {
                    self.parse_error_at_current_token(
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                } else {
                    self.parse_error_at_current_token(
                        &format!(
                            "'{modifier_text}' modifier cannot appear on a module or namespace element."
                        ),
                        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
                    );
                }
                let modifier_kind = self.token();
                self.next_token();
                let modifier = self.arena.add_token(
                    modifier_kind as u16,
                    modifier_start,
                    modifier_start + modifier_text.len() as u32,
                );
                self.parse_accessor_modified_statement(modifier_start, vec![modifier])
            }
        } else if self.look_ahead_next_is_identifier_or_keyword_on_same_line() {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            self.next_token();
            let downstream_start = self.token_pos();
            let preserve_downstream_expected = matches!(
                self.token(),
                SyntaxKind::BreakKeyword
                    | SyntaxKind::ContinueKeyword
                    | SyntaxKind::DoKeyword
                    | SyntaxKind::ForKeyword
                    | SyntaxKind::IfKeyword
                    | SyntaxKind::ReturnKeyword
                    | SyntaxKind::SwitchKeyword
                    | SyntaxKind::ThrowKeyword
                    | SyntaxKind::TryKeyword
                    | SyntaxKind::WhileKeyword
                    | SyntaxKind::WithKeyword
            );
            let diag_count = self.parse_diagnostics.len();
            let result = self.parse_statement();
            if !preserve_downstream_expected {
                let mut i = diag_count;
                while i < self.parse_diagnostics.len() {
                    if self.parse_diagnostics[i].code == diagnostic_codes::EXPECTED
                        && self.parse_diagnostics[i].start == downstream_start
                    {
                        self.parse_diagnostics.remove(i);
                    } else {
                        i += 1;
                    }
                }
            }
            result
        } else {
            self.parse_expression_statement()
        }
    }

    pub(crate) fn parse_statement_type_keyword(&mut self) -> NodeIndex {
        if let Some((start, end)) = self.look_ahead_next_void_keyword_on_same_line() {
            self.parse_error_at(
                start,
                end - start,
                "Type alias name cannot be 'void'.",
                tsz_common::diagnostics::diagnostic_codes::TYPE_ALIAS_NAME_CANNOT_BE,
            );
            return self.parse_expression_statement();
        }

        if self.look_ahead_is_type_alias_declaration()
            || self.look_ahead_next_is_numeric_literal_on_same_line()
        {
            self.parse_type_alias_declaration()
        } else {
            self.parse_expression_statement()
        }
    }

    pub(crate) fn parse_statement_declare_or_expression(&mut self) -> NodeIndex {
        // `declare` is a contextual keyword — it can be used as an identifier.
        // Only parse as ambient declaration if the next token is a valid declaration keyword.
        if self.look_ahead_is_declare_before_declaration() {
            self.parse_ambient_declaration()
        } else {
            self.parse_expression_statement()
        }
    }

    pub(crate) fn parse_statement_namespace_or_expression(&mut self) -> NodeIndex {
        if self.look_ahead_is_module_declaration() {
            self.parse_module_declaration()
        } else {
            self.parse_expression_statement()
        }
    }

    pub(crate) fn parse_statement_import_keyword(&mut self) -> NodeIndex {
        if self.look_ahead_is_import_call() {
            self.parse_expression_statement()
        } else if self.look_ahead_is_import_equals() {
            self.parse_import_equals_declaration()
        } else if self.look_ahead_is_import_declaration() {
            self.parse_import_declaration()
        } else {
            // `import` followed by a token that can't start any valid import form
            // (e.g., `import 10;`). tsc emits TS1128 "Declaration or statement expected"
            // at the `import` position. Emit the error, consume remaining tokens on the
            // line, and return an expression statement to avoid infinite recovery loops.
            let start_pos = self.token_pos();
            self.parse_error_at(
                start_pos,
                keyword_text_len(SyntaxKind::ImportKeyword),
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            self.next_token(); // consume 'import'
            if self.is_token(SyntaxKind::CommaToken) {
                let end_pos = self.token_end();
                return self
                    .arena
                    .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos);
            }
            // Consume remaining tokens until statement boundary
            while !self.is_token(SyntaxKind::SemicolonToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
                && !self.scanner.has_preceding_line_break()
            {
                self.next_token();
            }
            if self.is_token(SyntaxKind::SemicolonToken) {
                self.next_token();
            }
            let end_pos = self.token_end();
            self.arena
                .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos)
        }
    }

    pub(crate) fn look_ahead_has_missing_decorator_expression(&mut self) -> bool {
        if !self.is_token(SyntaxKind::AtToken) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token();
        let result = matches!(
            self.token(),
            SyntaxKind::AbstractKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::VarKeyword
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if a modifier keyword (public, protected, private, static, etc.)
    /// is followed by a declaration keyword like class, interface, function, etc.
    /// Used to detect `public interface I {}` or `static class C {}` patterns at module level.
    pub(crate) fn look_ahead_is_modifier_before_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip the modifier keyword
        let is_decl = matches!(
            self.token(),
            SyntaxKind::ClassKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::ExportKeyword
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Check if `declare` is followed by a valid declaration keyword on the same line.
    /// Used to distinguish `declare class ...` (ambient declaration) from
    /// `declare instanceof C` (expression using `declare` as identifier).
    /// ASI prevents treating `declare\nclass ...` as an ambient declaration.
    pub(crate) fn look_ahead_is_declare_before_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip `declare`
        // A statement may begin with a run of redundant `declare` keywords
        // (`declare declare const x`). tsc treats each extra `declare` as a
        // modifier (TS1030) and still parses the trailing ambient declaration,
        // so skip the run here and classify by what ultimately follows. ASI
        // still applies between consecutive `declare` keywords.
        while self.is_token(SyntaxKind::DeclareKeyword) && !self.scanner.has_preceding_line_break()
        {
            self.next_token();
        }
        let is_decl = if self.scanner.has_preceding_line_break() {
            false
        } else if self.is_token(SyntaxKind::ImportKeyword) {
            self.look_ahead_is_import_equals() || self.look_ahead_is_import_declaration()
        } else {
            matches!(
                self.token(),
                SyntaxKind::ClassKeyword
                    | SyntaxKind::InterfaceKeyword
                    | SyntaxKind::EnumKeyword
                    | SyntaxKind::NamespaceKeyword
                    | SyntaxKind::ModuleKeyword
                    | SyntaxKind::FunctionKeyword
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::ConstKeyword
                    | SyntaxKind::VarKeyword
                    | SyntaxKind::LetKeyword
                    | SyntaxKind::TypeKeyword
                    | SyntaxKind::GlobalKeyword
                    | SyntaxKind::AsyncKeyword
                    | SyntaxKind::UsingKeyword
                    | SyntaxKind::AwaitKeyword
                    | SyntaxKind::ExportKeyword
            )
        };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Check if the next token is an identifier or keyword on the same line.
    /// Matches tsc's `nextTokenIsIdentifierOrKeywordOnSameLine`.
    /// Used by `isStartOfStatement()` for modifier keywords (static, public, etc.)
    /// to distinguish class-member-like context from standalone expressions.
    pub(super) fn look_ahead_next_is_identifier_or_keyword_on_same_line(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip the modifier keyword
        let result = !self.scanner.has_preceding_line_break() && self.is_identifier_or_keyword();
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Check if the next token is a numeric literal on the same line.
    /// Used for invalid declaration-name recovery (e.g., `interface 100 {}`).
    pub(super) fn look_ahead_next_is_numeric_literal_on_same_line(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result =
            !self.scanner.has_preceding_line_break() && self.is_token(SyntaxKind::NumericLiteral);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Check if the next token is `void` on the same line.
    pub(super) fn look_ahead_next_void_keyword_on_same_line(&mut self) -> Option<(u32, u32)> {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result = (!self.scanner.has_preceding_line_break()
            && self.is_token(SyntaxKind::VoidKeyword))
        .then(|| (self.token_pos(), self.token_end()));
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Check if the next token is `{` on the same line.
    /// Used to detect `interface { }` where the interface name is missing.
    pub(super) fn look_ahead_next_is_open_brace_on_same_line(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result =
            !self.scanner.has_preceding_line_break() && self.is_token(SyntaxKind::OpenBraceToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Check if the next token is on a new line (ASI applies).
    /// Used to detect cases like:
    ///   abstract
    ///   class C {}
    /// where ASI should terminate `abstract` as an expression statement.
    pub(crate) fn next_token_is_on_new_line(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        self.scanner.scan();
        let has_line_break = self.scanner.has_preceding_line_break();
        self.scanner.restore_state(snapshot);
        has_line_break
    }

    /// Look ahead to see if the next token is `export` on the same line.
    /// Used to distinguish `static export as namespace ...` (modifier as expression + export statement)
    /// from `static class ...` (modifier before declaration).
    pub(crate) fn look_ahead_next_token_is_export_keyword(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result =
            !self.scanner.has_preceding_line_break() && self.token() == SyntaxKind::ExportKeyword;
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if we have "async function"
    pub(crate) fn look_ahead_is_async_function(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            token == SyntaxKind::FunctionKeyword
        })
    }

    /// Look ahead to see if "async" is followed by a declaration keyword.
    pub(crate) fn look_ahead_is_async_declaration(&mut self) -> bool {
        look_ahead_is_async_declaration(&mut self.scanner, self.current_token)
    }

    /// Look ahead to see if we have "abstract class"
    pub(crate) fn look_ahead_is_abstract_class(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            token == SyntaxKind::ClassKeyword
        })
    }

    /// Look ahead to see if "abstract" is followed by another declaration keyword.
    pub(crate) fn look_ahead_is_abstract_declaration(&mut self) -> bool {
        look_ahead_is_abstract_declaration(&mut self.scanner, self.current_token)
    }

    /// Emit a grammar diagnostic for a misplaced or duplicated modifier keyword
    /// at the current token, consume it, and return the modifier token node.
    /// Shared by the statement-leading recovery paths (a duplicated `declare`,
    /// a stray `abstract` before a variable/function declaration).
    pub(crate) fn consume_modifier_with_error(
        &mut self,
        kind: SyntaxKind,
        message: &str,
        code: u32,
    ) -> NodeIndex {
        let start = self.token_pos();
        let end = self.token_end();
        self.parse_error_at(start, end - start, message, code);
        self.next_token();
        self.arena.add_token(kind as u16, start, end)
    }

    /// Look ahead to see if `abstract` is followed by `export as` — the
    /// `export as namespace ...` shape specifically, distinct from the other
    /// `abstract export ...` forms (const/class/function/...), whose
    /// diagnostic choice depends on whether `abstract` is a legal modifier for
    /// the target node kind and is not decided by this lookahead (#16389).
    /// Only the `abstract`-`export` boundary is ASI-sensitive (`abstract` is a
    /// contextual keyword that ASI can cut off into its own expression
    /// statement); `export`-`as` is not — a line break there does not stop
    /// `tsc` from reading one `export as namespace` statement, so this
    /// lookahead does not require it either.
    pub(crate) fn look_ahead_is_abstract_before_export_as_namespace(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip 'abstract'
        let result = !self.scanner.has_preceding_line_break()
            && self.is_token(SyntaxKind::ExportKeyword)
            && {
                self.next_token(); // skip 'export'
                self.is_token(SyntaxKind::AsKeyword)
            };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Classify `abstract export <declaration>` by the node kind that `export`
    /// decorates, or return `None` when the shape is not one where `export` is
    /// a modifier on a trailing declaration (`export as namespace`, a
    /// non-class `export default ...`, `export = ...`, and a bare `abstract`
    /// identifier expression are all left to their existing paths). The
    /// `abstract`-`export` boundary is ASI-sensitive; `abstract` is a
    /// contextual keyword that a line break cuts into its own expression
    /// statement.
    pub(crate) fn look_ahead_abstract_before_export_target(
        &mut self,
    ) -> Option<AbstractExportTarget> {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip `abstract`
        let mut target = None;
        if !self.scanner.has_preceding_line_break() && self.is_token(SyntaxKind::ExportKeyword) {
            self.next_token(); // skip `export`
            target = match self.token() {
                SyntaxKind::ClassKeyword => Some(AbstractExportTarget::Class),
                SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::GlobalKeyword => self
                    .look_ahead_is_module_declaration()
                    .then_some(AbstractExportTarget::PositionErrorWins),
                // A named or star export declaration. `export = ...` is
                // deliberately not here: tsc routes it through TS1120, not
                // this modifier run.
                SyntaxKind::OpenBraceToken | SyntaxKind::AsteriskToken => {
                    Some(AbstractExportTarget::PositionErrorWins)
                }
                // `export default class C {}` (named or anonymous) reads the
                // same modifier run `[abstract, export, default]` as the bare
                // `export class` arm above, and `abstract` is legal on a
                // class regardless of `default` — same TS1029 ordering
                // violation, same anchor on `export`, every container
                // (#16398). A second, legally-placed `abstract` directly
                // before `class` (`abstract export default abstract class`)
                // is tolerated here too: it belongs to the correct
                // `export default abstract class` tail that
                // `parse_export_declaration` already parses standalone, and
                // tsc's own diagnostic set for that shape is still exactly
                // one TS1029 (oracle-confirmed). Every other
                // `export default <expr>` form admits no `abstract` modifier
                // at all and is left to its existing, unaffected path.
                SyntaxKind::DefaultKeyword => {
                    self.next_token(); // skip `default`
                    if self.is_token(SyntaxKind::AbstractKeyword) {
                        self.next_token(); // skip a second, legal `abstract`
                    }
                    matches!(self.token(), SyntaxKind::ClassKeyword)
                        .then_some(AbstractExportTarget::Class)
                }
                // `export type { x }` / `export type * from "m"` is a type-only
                // export declaration, not the type-alias form — same lookahead
                // the ambient `declare export` path uses.
                SyntaxKind::TypeKeyword => {
                    self.next_token();
                    Some(
                        if self.is_token(SyntaxKind::OpenBraceToken)
                            || self.is_token(SyntaxKind::AsteriskToken)
                        {
                            AbstractExportTarget::PositionErrorWins
                        } else {
                            AbstractExportTarget::ModifierRun
                        },
                    )
                }
                SyntaxKind::AsyncKeyword => self
                    .look_ahead_is_async_function()
                    .then_some(AbstractExportTarget::ModifierRun),
                SyntaxKind::ConstKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword => Some(AbstractExportTarget::ModifierRun),
                _ => None,
            };
        }
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        target
    }

    /// Look ahead to see if `abstract` is followed by a variable or function
    /// declaration keyword (`var`/`let`/`const`/`function`) on the same line.
    /// These are reserved words, so the following construct is unambiguously a
    /// declaration where `abstract` is a misplaced modifier (TS1242), not an
    /// expression that happens to start with the identifier `abstract`.
    pub(crate) fn look_ahead_is_abstract_before_var_or_function(&mut self) -> bool {
        look_ahead_is_on_same_line(&mut self.scanner, self.current_token, |token| {
            matches!(
                token,
                SyntaxKind::VarKeyword
                    | SyntaxKind::LetKeyword
                    | SyntaxKind::ConstKeyword
                    | SyntaxKind::FunctionKeyword
            )
        })
    }

    /// Look ahead to see if "accessor" is followed by a declaration keyword.
    pub(crate) fn look_ahead_is_accessor_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip 'accessor'
        let is_decl = matches!(
            self.token(),
            SyntaxKind::ClassKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::ExportKeyword
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Look ahead to see if `let` starts a variable declaration.
    /// In tsc, `let` is only treated as a declaration keyword when followed by
    /// an identifier, `{` (object destructuring), or `[` (array destructuring).
    /// Otherwise (e.g. `let;`), `let` is treated as an identifier expression.
    pub(crate) fn look_ahead_is_let_declaration(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            is_identifier_or_keyword(token)
                || token == SyntaxKind::OpenBraceToken
                || token == SyntaxKind::OpenBracketToken
        })
    }

    /// Look ahead to see if we have "await using"
    pub(crate) fn look_ahead_is_using_declaration(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            is_identifier_or_keyword(token) || token == SyntaxKind::OpenBraceToken
        })
    }

    /// Look ahead for `using` in a for-statement initializer position.
    /// Matches tsc's `nextTokenIsBindingIdentifierOrStartOfDestructuringOnSameLineDisallowOf`.
    ///
    /// When `using` is followed by `of`, we look a second token ahead:
    /// - `for (using of = null;;)` → `=` after `of` means `of` is a binding name (using declaration)
    /// - `for (using of;;)` → `;` after `of` means `of` is a binding name (using declaration)
    /// - `for (using of: T = v;;)` → `:` after `of` means `of` is a binding name (using declaration)
    /// - `for (using of expr)` → anything else means `of` is the for-of keyword
    ///
    /// `in` after `using` always indicates for-in, not a using declaration.
    pub(crate) fn look_ahead_is_using_declaration_in_for(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let next = self.scanner.scan();

        let result = if next == SyntaxKind::InKeyword {
            false
        } else if next == SyntaxKind::OfKeyword {
            // Look one more token ahead: if `=`, `;`, or `:` follows `of`,
            // then `of` is a binding name in a using declaration.
            let next2 = self.scanner.scan();
            next2 == SyntaxKind::EqualsToken
                || next2 == SyntaxKind::SemicolonToken
                || next2 == SyntaxKind::ColonToken
        } else {
            (is_identifier_or_keyword(next) || next == SyntaxKind::OpenBraceToken)
                && !self.scanner.has_preceding_line_break()
        };

        self.scanner.restore_state(snapshot);
        result
    }

    pub(crate) fn look_ahead_is_await_using_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let t1 = self.scanner.scan();
        let t2 = self.scanner.scan();
        let result = t1 == SyntaxKind::UsingKeyword
            && (is_identifier_or_keyword(t2) || t2 == SyntaxKind::OpenBraceToken);
        self.scanner.restore_state(snapshot);
        result
    }

    /// Look ahead for `await using` in a for-statement initializer position.
    /// In `for (await using of ...)`, the first `of` is the for-of keyword, not a
    /// binding name. But in `for (await using of of [...])`, the first `of` IS the
    /// binding name and the second `of` is the for-of keyword. Disambiguate by
    /// scanning further: if t2 is `of` and t3 is also `of`, then t2 is a binding name.
    pub(crate) fn look_ahead_is_await_using_declaration_in_for(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let t1 = self.scanner.scan(); // should be `using`
        let t2 = self.scanner.scan(); // binding name or `of`/`in`
        let result = if t1 != SyntaxKind::UsingKeyword {
            false
        } else if t2 == SyntaxKind::OfKeyword {
            // `await using of` — check if the next token is also `of`,
            // meaning the first `of` is the binding name (e.g., `await using of of [...]`).
            let t3 = self.scanner.scan();
            t3 == SyntaxKind::OfKeyword
        } else if t2 == SyntaxKind::InKeyword {
            false
        } else {
            is_identifier_or_keyword(t2) || t2 == SyntaxKind::OpenBraceToken
        };
        self.scanner.restore_state(snapshot);
        result
    }

    #[allow(dead_code)]
    pub(crate) fn look_ahead_is_await_using(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            token == SyntaxKind::UsingKeyword
        })
    }

    /// Look ahead to see if we have "import identifier ="
    pub(crate) fn look_ahead_is_import_equals(&mut self) -> bool {
        look_ahead_is_import_equals(
            &mut self.scanner,
            self.current_token,
            is_identifier_or_contextual_keyword,
        )
    }

    /// Look ahead to check if the current identifier is directly followed by `=`.
    /// Used to disambiguate `import type X =` (where `type` is import name)
    /// from `import type X = require(...)` (where `type` is modifier).
    pub(crate) fn look_ahead_is_equals_after_identifier(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        // Skip current token (the identifier)
        self.next_token();
        let result = self.is_token(SyntaxKind::EqualsToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if we have "import (" (dynamic import call)
    pub(crate) fn look_ahead_is_import_call(&mut self) -> bool {
        look_ahead_is_import_call(&mut self.scanner, self.current_token)
    }

    /// Look ahead to see if `import` is starting a declaration rather than an expression.
    /// Valid starts are:
    /// - string literal: `import "mod";`
    /// - identifier/keyword: default import or contextual modifier/name
    /// - `{` / `*`: named or namespace imports
    pub(crate) fn look_ahead_is_import_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip `import`
        let result = matches!(
            self.token(),
            SyntaxKind::StringLiteral
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::AsteriskToken
                | SyntaxKind::TypeKeyword
                | SyntaxKind::DeferKeyword
        ) || self.is_identifier_or_keyword();
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if we have `export =`.
    #[allow(dead_code)]
    pub(crate) fn look_ahead_is_export_assignment(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip `export`
        let result = self.is_token(SyntaxKind::EqualsToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if "namespace"/"module" starts a declaration.
    /// Updated to recognize anonymous modules: module { ... }
    pub(crate) fn look_ahead_is_module_declaration(&mut self) -> bool {
        look_ahead_is_module_declaration(&mut self.scanner, self.current_token)
    }

    /// Look ahead to see if "type" starts a type alias declaration.
    pub(crate) fn look_ahead_is_type_alias_declaration(&mut self) -> bool {
        look_ahead_is_type_alias_declaration(&mut self.scanner, self.current_token)
    }

    /// Look ahead to see if we have "identifier :" (labeled statement)
    pub(crate) fn look_ahead_is_labeled_statement(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip identifier
        self.next_token();
        // Check for ':'
        let is_colon = self.is_token(SyntaxKind::ColonToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_colon
    }

    /// Look ahead to get the colon position for a labeled statement.
    /// Used to emit TS1109 at the colon position when a reserved word
    /// like `await` is used as a label in static blocks.
    pub(crate) fn look_ahead_get_labeled_colon_pos(&mut self) -> u32 {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip identifier
        self.next_token();
        // Get colon position
        let colon_pos = self.u32_from_usize(self.token_pos() as usize);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        colon_pos
    }

    /// Look ahead to see if we have "const enum"
    pub(crate) fn look_ahead_is_const_enum(&mut self) -> bool {
        look_ahead_is_const_enum(&mut self.scanner, self.current_token)
    }

    /// Parse const enum declaration
    pub(crate) fn parse_const_enum_declaration(
        &mut self,
        start_pos: u32,
        mut modifiers: Vec<NodeIndex>,
    ) -> NodeIndex {
        let const_start = self.token_pos();
        self.parse_expected(SyntaxKind::ConstKeyword);
        let const_end = self.token_end();
        let const_modifier =
            self.arena
                .add_token(SyntaxKind::ConstKeyword as u16, const_start, const_end);
        modifiers.push(const_modifier);

        let modifiers = Some(Self::make_node_list(modifiers));
        self.parse_enum_declaration_with_modifiers(start_pos, modifiers)
    }

    /// Parse labeled statement: label: statement
    pub(crate) fn parse_labeled_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the label (identifier)
        let label = self.parse_identifier_name();

        // Note: tsc does NOT emit TS1003 for `await` used as a label in static
        // blocks or async contexts. Instead, it treats `await` as a keyword and
        // parses it as an expression, emitting TS1109 when `:<statement>` follows.
        // The TS1109 error is emitted in parse_statement() before calling this function.

        // Check for duplicate labels (TS1114) and record this label
        let label_name = if let Some(label_node) = self.arena.get(label) {
            if let Some(ident) = self.arena.get_identifier_at(label) {
                let escaped_text = ident.escaped_text.clone();
                let pos = label_node.pos;
                self.check_duplicate_label(&escaped_text, pos);
                Some(escaped_text)
            } else {
                None
            }
        } else {
            None
        };

        // Consume the colon
        self.parse_expected(SyntaxKind::ColonToken);

        // Parse the statement
        let statement = self.parse_statement();

        // Remove the label from the current scope (labels are statement-scoped)
        // This allows sequential labels with the same name: target: stmt1; target: stmt2;
        if let Some(label_name) = label_name
            && let Some(current_scope) = self.label_scopes.last_mut()
        {
            current_scope.remove(label_name.as_str());
        }

        let end_pos = self.token_end();

        self.arena.add_labeled(
            syntax_kind_ext::LABELED_STATEMENT,
            start_pos,
            end_pos,
            LabeledData { label, statement },
        )
    }

    /// Parse import equals declaration: import X = require("...") or import X = Y.Z
    pub(crate) fn parse_import_equals_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_import_equals_declaration_with_modifiers(start_pos, None)
    }

    pub(crate) fn parse_import_equals_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        self.parse_expected(SyntaxKind::ImportKeyword);

        // Check for type modifier: `import type X = require(...)`
        let is_type_only = if self.is_token(SyntaxKind::TypeKeyword)
            && !self.look_ahead_is_equals_after_identifier()
        {
            self.next_token();
            true
        } else {
            false
        };
        let reserved_word_import_equals_name = self.is_reserved_word();
        // Parse the name - allow keywords like `require` and `exports` as valid names.
        // The import-equals parser itself is responsible for the recovery shape.
        let name = if reserved_word_import_equals_name {
            let name_start = self.token_pos();
            let name_end = self.token_end();
            self.error_expression_expected();
            self.next_token();
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                name_start,
                name_end,
                IdentifierData {
                    atom: AstAtom::NONE,
                    escaped_text: IdentText::empty(),
                    original_text: None,
                },
            )
        } else {
            self.parse_identifier_name()
        };

        if reserved_word_import_equals_name {
            self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            if self.is_token(SyntaxKind::EqualsToken) {
                self.next_token();
            }
            while !matches!(
                self.token(),
                SyntaxKind::SemicolonToken | SyntaxKind::EndOfFileToken
            ) {
                self.next_token();
            }
            if self.is_token(SyntaxKind::SemicolonToken) {
                self.parse_error_at_current_token("')' expected.", diagnostic_codes::EXPECTED);
            }
            self.parse_semicolon();
            let end_pos = self.token_full_start();
            return self.arena.add_import_decl(
                syntax_kind_ext::IMPORT_EQUALS_DECLARATION,
                start_pos,
                end_pos,
                ImportDeclData {
                    modifiers,
                    is_type_only,
                    import_clause: name,
                    module_specifier: NodeIndex::NONE,
                    attributes: NodeIndex::NONE,
                },
            );
        }

        self.parse_expected(SyntaxKind::EqualsToken);

        // Parse module reference: require("...") or qualified name
        let module_reference = if self.is_token(SyntaxKind::RequireKeyword) {
            self.parse_external_module_reference()
        } else {
            self.parse_entity_name()
        };

        self.parse_semicolon();
        let end_pos = self.token_full_start();

        // Use ImportDeclData with import_clause as the name and module_specifier as reference
        // This is a simplified representation
        self.arena.add_import_decl(
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION,
            start_pos,
            end_pos,
            ImportDeclData {
                modifiers,
                is_type_only,
                import_clause: name,
                module_specifier: module_reference,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse external module reference: require("...")
    pub(crate) fn parse_external_module_reference(&mut self) -> NodeIndex {
        self.parse_expected(SyntaxKind::RequireKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_string_literal();
        // If parse_string_literal failed (non-string token), skip past the invalid token
        // so we can find the closing paren and avoid cascading errors (e.g. TS1128).
        if expression == NodeIndex::NONE
            && !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            self.next_token();
        }
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Return the string literal as the module reference
        expression
    }

    /// Parse entity name: A or A.B.C or this or this.x
    pub(crate) fn parse_entity_name(&mut self) -> NodeIndex {
        self.parse_entity_name_inner(false)
    }

    pub(crate) fn parse_entity_name_allow_reserved(&mut self) -> NodeIndex {
        self.parse_entity_name_inner(true)
    }
}
