use super::state::*;
use crate::parser::node::*;
use crate::parser::parse_rules::*;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::interner::Atom;
use tsz_scanner::{SyntaxKind, keyword_text_len};

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
            let modifiers = Some(self.make_node_list(vec![async_modifier]));
            match self.token() {
                SyntaxKind::ClassKeyword => {
                    self.parse_class_declaration_with_modifiers(start_pos, modifiers)
                }
                SyntaxKind::EnumKeyword => {
                    self.parse_enum_declaration_with_modifiers(start_pos, modifiers)
                }
                SyntaxKind::InterfaceKeyword => {
                    self.parse_interface_declaration_with_modifiers(start_pos, modifiers)
                }
                SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::GlobalKeyword => {
                    if self.look_ahead_is_module_declaration() {
                        self.parse_module_declaration_with_modifiers(start_pos, modifiers)
                    } else {
                        self.parse_expression_statement()
                    }
                }
                _ => self.parse_expression_statement(),
            }
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
            self.parse_error_at_current_token(
                "'accessor' modifier can only appear on a property declaration.",
                diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
            );
            self.next_token();
            self.parse_statement()
        } else {
            self.parse_expression_statement()
        }
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
                // TS1044: '{0}' modifier cannot appear on a module or namespace element.
                let modifier_text = self.scanner.get_token_text();
                self.parse_error_at_current_token(
                    &format!(
                        "'{modifier_text}' modifier cannot appear on a module or namespace element."
                    ),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
                );
                self.next_token();
                self.parse_statement()
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

        let modifiers = Some(self.make_node_list(modifiers));
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
            current_scope.remove(&label_name);
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
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
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
