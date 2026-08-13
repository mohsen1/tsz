use super::state::*;
use crate::parser::node::*;
use crate::parser::parse_rules::*;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::interner::AstAtom;
use tsz_scanner::{SyntaxKind, keyword_text_len};

/// What the `export` that follows a stray modifier keyword actually starts.
///
/// tsc attaches the modifier to whichever node the `export` begins, and
/// `checkGrammarModifiers` then answers by that node's own kind — not by the
/// modifier, and not by the container alone. The three arms below get three
/// genuinely different answers, so the classification has to happen before the
/// container gate runs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModifiedExportForm {
    /// `export as namespace Foo;` — a `NamespaceExportDeclaration`, which
    /// admits no modifiers in any container.
    NamespaceExport,
    /// `export {}`, `export * from` — an `ExportDeclaration` node. Its own
    /// placement diagnostic (TS1233) wins over the modifier only inside a
    /// `Block`; at the source file's own top level and inside a namespace
    /// body the modifier diagnostic still fires (oracle-pinned, #16403).
    ExportDeclaration,
    /// `export =`, `export default` — an `ExportAssignment` node. Its own
    /// placement diagnostic (TS1231/TS1258 in a `Block`, TS1063/TS1319 in a
    /// namespace body) wins over the modifier in *both* of those containers,
    /// not just a `Block` — the modifier diagnostic survives only at the
    /// source file's own top level (oracle-pinned, #16403).
    ExportAssignment,
    /// `export namespace N {}`, `export module M {}` — a nested module
    /// declaration is itself illegal inside a `Block` (TS1235, independent of
    /// any modifier), so that placement diagnostic wins there the same way
    /// the two forms above do; outside a `Block` this nests validly and takes
    /// the ordinary modified-declaration container split (#16403).
    ModuleDeclaration,
    /// `export const`, `export class`, `export function`, ... — an ordinary
    /// modified declaration, which takes the container split.
    ModifiedDeclaration,
}

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
    /// `abstract export default <expr>` where `default` decorates a value
    /// expression rather than a class/function declaration — an
    /// `ExportAssignment` node, on which `abstract` is illegal everywhere.
    /// Its own placement diagnostic wins in *both* a Block (TS1258) and a
    /// namespace body (TS1319), wider than `PositionErrorWins`'s Block-only
    /// silencing, so TS1242 survives only at the source file's own top
    /// level — the same shape the sibling `async`/`accessor` families give
    /// their own `ExportAssignment` variant.
    ExportAssignment,
}

/// Which grammar diagnostic `checkGrammarModifiers` picks for a stray `async`
/// before `export ...` — distinct from every sibling modifier family because
/// `async` is legal on a function declaration in *every* container, including
/// a Block (a nested async function declaration is ordinarily fine there),
/// so a Block cannot uniformly silence it the way it does for the other
/// families (#16403 slice 3).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncExportTarget {
    /// `async export function f() {}`, `async export default function
    /// (f)() {}` — `async` is legal here in every container, so the only
    /// violation is modifier order: TS1029 on `export`, unconditionally,
    /// outranking the container check that silences every other modifier
    /// family inside a Block.
    Function,
    /// `async export {}`, `async export * from "m"`, `async export { a }
    /// from "m"` — an `ExportDeclaration` node, which admits no modifiers at
    /// all (a structural mismatch, not an ordering one): TS1042 wherever the
    /// declaration's own placement diagnostic does not already win outright
    /// (a Block, where TS1233 wins alone).
    ExportDeclaration,
    /// `async export =`, `async export default <expr>` — an
    /// `ExportAssignment` node, the same structural mismatch as above but
    /// wider silencing: its own placement diagnostic (TS1231/TS1258 in a
    /// Block, TS1063/TS1319 in a namespace body) wins in *both* of those
    /// containers, and TS1042 survives only at the source file's own top
    /// level.
    ExportAssignment,
    /// `async export namespace N {}`, `async export module M {}` — `export`
    /// is a legal modifier position here, so this takes the same
    /// order-vs-container-split answer as `ModifierRun` below, except a
    /// Block silences it completely through the nested module declaration's
    /// own TS1235, the same way the sibling modifier families' own
    /// `ModuleDeclaration` handling does.
    ModuleDeclaration,
    /// `async export const/class/interface/type/enum ...` — an ordinary
    /// modified declaration. `async` is not legal on any of these, but
    /// `export` is a legal modifier at this container, so tsc's modifier
    /// order check reports TS1029 outside a Block; inside one the generic
    /// "modifiers not allowed on a block-scoped statement" TS1184 wins
    /// instead — the block gate runs first and does not special-case
    /// `async` the way it does for an actual function declaration.
    ModifierRun,
    /// `async export as namespace Foo;` — a `NamespaceExportDeclaration`,
    /// which like every other modifier family admits no modifiers in any
    /// container: TS1184 unconditionally.
    NamespaceExport,
}

impl ParserState {
    pub(crate) fn parse_statement_async_declaration_or_expression(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.look_ahead_is_async_function() {
            self.parse_async_function_declaration()
        } else if let Some(target) = self.look_ahead_async_before_export_target() {
            // A stray `async` directly before `export ...` (#16403 slice 3):
            // see `AsyncExportTarget` for the structural rule per form.
            let start_pos = self.token_pos();
            let async_start = self.token_pos();
            let in_block = self.in_block_context() || self.in_static_block_context();
            let mut emit_order_diagnostic = false;
            match target {
                AsyncExportTarget::Function => emit_order_diagnostic = true,
                AsyncExportTarget::NamespaceExport => {
                    self.parse_error_at_current_token(
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
                AsyncExportTarget::ExportDeclaration | AsyncExportTarget::ModuleDeclaration
                    if in_block => {}
                AsyncExportTarget::ExportAssignment
                    if in_block || self.in_module_body_context() => {}
                AsyncExportTarget::ModifierRun if in_block => {
                    self.parse_error_at_current_token(
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
                AsyncExportTarget::ExportDeclaration | AsyncExportTarget::ExportAssignment => {
                    self.parse_error_at_current_token(
                        "'async' modifier cannot be used here.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
                    );
                }
                AsyncExportTarget::ModuleDeclaration | AsyncExportTarget::ModifierRun => {
                    emit_order_diagnostic = true;
                }
            }
            self.parse_expected(SyntaxKind::AsyncKeyword);
            let async_end = self.token_end();
            if emit_order_diagnostic {
                // tsc anchors TS1029 on the *later* of the two modifiers,
                // i.e. the `export` keyword now sitting at the current token
                // (same anchor `abstract`'s sibling ordering check uses).
                let export_start = self.token_pos();
                let export_end = self.token_end();
                self.parse_error_at(
                    export_start,
                    export_end - export_start,
                    &tsz_common::diagnostics::diagnostic_messages::MODIFIER_MUST_PRECEDE_MODIFIER
                        .replace("{0}", "export")
                        .replace("{1}", "async"),
                    diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                );
            }
            let async_modifier =
                self.arena
                    .add_token(SyntaxKind::AsyncKeyword as u16, async_start, async_end);
            self.parse_accessor_modified_statement(start_pos, vec![async_modifier])
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
                // `export default <expr>`'s own placement diagnostic
                // (TS1258 in a Block, TS1319 in a namespace body) wins in
                // both containers, not just a Block.
                AbstractExportTarget::ExportAssignment
                    if in_block || self.in_module_body_context() => {}
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
            // on any non-property-declaration node (top-level class/interface/var/...) —
            // but like the `static`/`readonly` family (#16403 slices 1-2), a stray
            // `accessor` before `export ...` takes the SAME `ModifiedExportForm`
            // container split rather than reporting TS1275 unconditionally
            // (#16403 slice 5, oracle-pinned): `export {}`/`export *` and
            // `export namespace`/`export module` are silenced by their own
            // placement diagnostic inside a Block; `export =`/`export default`
            // are silenced there AND in a namespace body; `export as namespace`
            // gets the uniform TS1184 every sibling family reports, not TS1275;
            // every other export form (`export const`/`class`/`function`/...)
            // keeps TS1275 outside a Block and swaps to the generic TS1184
            // inside one, exactly like `static`/`readonly`.
            let start_pos = self.token_pos();
            let export_form = self.modified_export_form();
            let block_context = self.in_block_context() || self.in_static_block_context();
            let export_silences_modifier = match export_form {
                Some(
                    ModifiedExportForm::ExportDeclaration | ModifiedExportForm::ModuleDeclaration,
                ) => block_context,
                Some(ModifiedExportForm::ExportAssignment) => {
                    block_context || self.in_module_body_context()
                }
                _ => false,
            };
            if export_silences_modifier {
                self.next_token();
                return self.parse_statement();
            }
            if export_form == Some(ModifiedExportForm::NamespaceExport) || block_context {
                self.parse_error_at_current_token(
                    "Modifiers cannot appear here.",
                    diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                );
            } else {
                self.parse_error_at_current_token(
                    "'accessor' modifier can only appear on a property declaration.",
                    diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
                );
            }
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
                    return self.parse_export_declaration_from(start_pos);
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
        let result = match self.token() {
            SyntaxKind::OpenBraceToken
            | SyntaxKind::DefaultKeyword
            | SyntaxKind::AsteriskToken
            | SyntaxKind::EqualsToken
            | SyntaxKind::AsKeyword => true,
            // `export type { x } from "m"` / `export type * from "m"` is a
            // type-only export declaration and has to reach
            // `parse_export_declaration` like its untyped siblings above.
            // Without this the `export` is consumed as a modifier and the
            // recursion lands on `parse_type_alias_declaration_with_modifiers`,
            // which reads the `{` as a malformed alias body and adds TS1003 /
            // TS1005 noise tsc never reports. `export type T = ...` is the
            // genuine alias form and must stay on that path.
            SyntaxKind::TypeKeyword => {
                self.next_token();
                matches!(
                    self.token(),
                    SyntaxKind::OpenBraceToken | SyntaxKind::AsteriskToken
                )
            }
            _ => false,
        };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    pub(crate) fn parse_statement_top_level_modifier(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.next_token_is_on_new_line() {
            self.parse_expression_statement()
        } else if self.token() == SyntaxKind::OverrideKeyword {
            // `override` is never a valid statement/declaration modifier
            // outside a class member — tsc's parser does not recognize it as
            // starting a declaration there at all, so it reports a single,
            // unconditional "Unexpected keyword or identifier." at the
            // `override` token and continues parsing the remainder as if
            // `override` were never there. Unlike the sibling modifiers this
            // does not depend on the container or on what follows (`export`
            // or otherwise): oracle-pinned across 3 containers x 11 export
            // forms plus the non-export control, all 34 rows report TS1434
            // alone (#16403). Consuming the token and re-dispatching through
            // `parse_statement()` reproduces that: whatever follows parses as
            // an ordinary statement with no leftover modifier, so it draws no
            // diagnostic of its own — matching tsc, which still binds the
            // trailing declaration (confirmed: a reference to the declared
            // name after this statement resolves, not TS2304) while
            // suppressing its grammar diagnostics for the file, the same
            // file-wide effect #16367 documented for a genuine parse error.
            self.parse_error_at_current_token(
                "Unexpected keyword or identifier.",
                diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
            );
            self.next_token();
            self.parse_statement()
        } else if self.look_ahead_is_stacked_modifier_run_before_export_as_namespace() {
            // `<modifier> <modifier> ... export as namespace Foo;` — a run of
            // two or more stray modifiers before a `NamespaceExportDeclaration`.
            // This generalizes the single-modifier case (#16540) to a run of
            // any length: `modified_export_form` only looks one modifier past
            // the current token, so a run never reached that path and fell into
            // the `TS1128` recovery below instead. tsc reports a single TS1184
            // anchored at the first modifier and threads the declaration's span
            // from there so its own placement diagnostic (TS1314/TS1315/TS1316)
            // anchors at the first modifier too — regardless of the run's
            // length, the modifier kinds, their order, or the container, since a
            // `NamespaceExportDeclaration` admits no modifiers anywhere
            // (oracle-pinned, `typescript@7.0.2`, #16403). The lookahead already
            // excluded the two runs tsc does *not* answer this way (a repeated
            // `static`, and a run containing `override`), so every run reaching
            // here takes the uniform answer.
            let run_start = self.token_pos();
            self.parse_error_at_current_token(
                "Modifiers cannot appear here.",
                diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
            );
            self.next_token(); // consume the first modifier
            while Self::is_stacked_export_run_modifier(self.token())
                && !self.scanner.has_preceding_line_break()
            {
                self.next_token();
            }
            self.parse_export_declaration_from(run_start)
        } else if self.look_ahead_is_modifier_before_declaration() {
            // `static`/`public`/`protected`/`private`/`readonly` share the
            // same container split (#16403 slices 1-2): a Block body gets
            // the generic TS1184; a module/namespace body or the source
            // file's own top level, neither of which is a Block, keeps a
            // module/namespace-specific diagnostic — TS1044 for the first
            // four (formatted with the actual modifier text), TS1024 for
            // `readonly` (its own fixed message, oracle-pinned identical to
            // the TS1044 family's silencing shape in every other position).
            // `override` never reaches here (short-circuited above).
            let is_ts1044_family_modifier = matches!(
                self.token(),
                SyntaxKind::StaticKeyword
                    | SyntaxKind::PublicKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::PrivateKeyword
            );
            let is_readonly_modifier = self.token() == SyntaxKind::ReadonlyKeyword;
            let takes_container_split = is_ts1044_family_modifier || is_readonly_modifier;
            let export_form = self.modified_export_form();
            // `in_static_block_context()` covers the static-block case
            // directly rather than through `in_block_context()`
            // (`parse_static_block` does not set CONTEXT_FLAG_IN_BLOCK,
            // deliberately: doing so also makes the class-body nested-block
            // recovery heuristic a few lines up in `parse_statements` fire
            // inside static blocks, which is a separate, pre-existing bug —
            // confirmed it already misparses a plain method body the same
            // way, unrelated to this fix — and out of scope here).
            let block_context = self.in_block_context() || self.in_static_block_context();
            match export_form {
                Some(ModifiedExportForm::ExportDeclaration)
                    if !takes_container_split || block_context =>
                {
                    // `export {}` / `export * from` after a stray modifier,
                    // inside a Block: tsc reports the form's own placement
                    // diagnostic (TS1233) and no modifier diagnostic at all,
                    // so the modifier is dropped silently here.
                    self.next_token();
                    self.parse_statement()
                }
                Some(ModifiedExportForm::ExportAssignment)
                    if !takes_container_split || block_context || self.in_module_body_context() =>
                {
                    // `export =` / `export default` after a stray modifier,
                    // inside a Block *or* a namespace body: the assignment's
                    // own placement diagnostic (TS1231/TS1258 in a Block,
                    // TS1063/TS1319 in a namespace body) wins outright and
                    // the modifier is dropped silently — unlike the
                    // declaration form above, the namespace-body case is
                    // ALSO silent here, not just the Block one (#16403).
                    self.next_token();
                    self.parse_statement()
                }
                Some(ModifiedExportForm::ModuleDeclaration)
                    if takes_container_split && block_context =>
                {
                    // `export namespace N {}` / `export module M {}` inside
                    // a Block: a nested module declaration is itself illegal
                    // there (TS1235) independent of any modifier, and that
                    // placement diagnostic wins the same way the two forms
                    // above do. Gated to the modifiers that take the
                    // container split (#16403 slices 1-2) — `override` never
                    // reaches this match at all (short-circuited above).
                    self.next_token();
                    self.parse_statement()
                }
                Some(ModifiedExportForm::NamespaceExport) => {
                    // `export as namespace Foo;` after a stray modifier. Unlike
                    // every other shape in this branch the answer is not
                    // container-derived: a `NamespaceExportDeclaration` admits
                    // no modifiers at all, so tsc's grammar check reports
                    // TS1184 in a Block, in a namespace body, and at the
                    // source file's own top level alike — where a modified
                    // *declaration* would keep TS1044 in the latter two.
                    //
                    // The checker's own TS1314 (global-module-exports
                    // placement) reads the `NamespaceExportDeclaration` node's
                    // span, and tsc anchors that span at the first modifier,
                    // not at `export` (#16403 residual, oracle-confirmed for
                    // `static`/`public`/`protected`/`private`/`readonly`
                    // — the sibling `accessor`/`async` dispatch already
                    // threads `start_pos` through `parse_export_declaration_from`
                    // for this same reason). Capture the modifier's position
                    // before consuming it and hand the node construction to
                    // that shared helper instead of dropping straight to a
                    // fresh `parse_statement()`, which would re-anchor the
                    // node at `export`.
                    let start_pos = self.token_pos();
                    self.parse_error_at_current_token(
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                    self.next_token();
                    self.parse_export_declaration_from(start_pos)
                }
                _ => {
                    // Every other shape reaching this branch — a plain
                    // modified declaration (`export const`/`class`/
                    // `function`/`interface`/`type`/`enum`), or one of the
                    // three forms above once it is outside the container that
                    // silences its modifier diagnostic — takes the same
                    // container split tsc's grammar check uses for an
                    // ordinary modified declaration: a Block body gets the
                    // generic TS1184; a module/namespace body or the source
                    // file's own top level, neither of which is a Block,
                    // keeps a module/namespace-specific diagnostic — TS1044
                    // (formatted with the modifier text) for the
                    // `static`/`public`/`protected`/`private` family,
                    // `readonly`'s own fixed-message TS1024 otherwise
                    // (#16368, #16403).
                    let modifier_start = self.token_pos();
                    let modifier_text = self.scanner.get_token_text();
                    let modifier_kind = self.token();
                    if block_context {
                        self.parse_error_at_current_token(
                            "Modifiers cannot appear here.",
                            diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                        );
                    } else if modifier_kind == SyntaxKind::ReadonlyKeyword {
                        self.parse_error_at_current_token(
                            "'readonly' modifier can only appear on a property declaration or index signature.",
                            diagnostic_codes::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
                        );
                    } else {
                        self.parse_error_at_current_token(
                            &format!(
                                "'{modifier_text}' modifier cannot appear on a module or namespace element."
                            ),
                            diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
                        );
                    }
                    self.next_token();
                    let modifier = self.arena.add_token(
                        modifier_kind as u16,
                        modifier_start,
                        modifier_start + modifier_text.len() as u32,
                    );
                    self.parse_accessor_modified_statement(modifier_start, vec![modifier])
                }
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

    /// Look ahead past a stray modifier keyword to classify the `export` statement
    /// it prefixes, if the next token is `export` on the same line.
    ///
    /// Returns `None` when the modifier is not followed by `export`, which leaves
    /// the caller on its ordinary container-based path.
    pub(crate) fn modified_export_form(&mut self) -> Option<ModifiedExportForm> {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip the modifier keyword
        let form = if self.scanner.has_preceding_line_break()
            || self.token() != SyntaxKind::ExportKeyword
        {
            None
        } else {
            self.next_token(); // skip `export`
            Some(match self.token() {
                SyntaxKind::AsKeyword => ModifiedExportForm::NamespaceExport,
                SyntaxKind::OpenBraceToken | SyntaxKind::AsteriskToken => {
                    ModifiedExportForm::ExportDeclaration
                }
                SyntaxKind::EqualsToken => ModifiedExportForm::ExportAssignment,
                // `export default class ...` / `export default function ...`
                // are a `ClassDeclaration` / `FunctionDeclaration` carrying a
                // `default` modifier, not an `ExportAssignment` node — only a
                // bare `export default <expr>` is the assignment. The
                // distinction is load-bearing: an `ExportAssignment`'s own
                // placement diagnostic silences the modifier in a namespace
                // body (TS1319) and in a Block (TS1258), where a declaration
                // keeps the ordinary container split (TS1044/TS1024 outside a
                // Block, TS1184 inside one). Oracle-pinned across
                // `static`/`public`/`protected`/`private`/`readonly` x 3
                // containers (#16403); the sibling `async` dispatch already
                // draws this same line in `look_ahead_async_before_export_target`.
                SyntaxKind::DefaultKeyword => {
                    self.next_token(); // skip `default`
                    // `default` may carry further modifiers of its own before
                    // the declaration keyword (`export default abstract class`,
                    // `export default async function`), and those take the same
                    // declaration answer. Skipping them is safe precisely
                    // because the classification still turns on a `class`/
                    // `function` keyword actually following: `export default
                    // async () => 1` stops at `(` and stays an assignment.
                    while matches!(
                        self.token(),
                        SyntaxKind::AbstractKeyword
                            | SyntaxKind::AsyncKeyword
                            | SyntaxKind::DeclareKeyword
                    ) {
                        self.next_token();
                    }
                    match self.token() {
                        SyntaxKind::ClassKeyword | SyntaxKind::FunctionKeyword => {
                            ModifiedExportForm::ModifiedDeclaration
                        }
                        _ => ModifiedExportForm::ExportAssignment,
                    }
                }
                // `export type { x } from "m"` / `export type * from "m"` is a
                // type-only `ExportDeclaration`, which draws its own TS1233 in
                // a Block and silences the modifier there; only the type-alias
                // form (`export type T = ...`) is an ordinary declaration. The
                // `async` dispatch uses the identical lookahead.
                SyntaxKind::TypeKeyword => {
                    self.next_token(); // skip `type`
                    if matches!(
                        self.token(),
                        SyntaxKind::OpenBraceToken | SyntaxKind::AsteriskToken
                    ) {
                        ModifiedExportForm::ExportDeclaration
                    } else {
                        ModifiedExportForm::ModifiedDeclaration
                    }
                }
                SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::GlobalKeyword => ModifiedExportForm::ModuleDeclaration,
                _ => ModifiedExportForm::ModifiedDeclaration,
            })
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        form
    }

    /// The stray modifier keywords that may lead or fill a run before an
    /// `export as namespace` declaration and that tsc collapses into a single
    /// TS1184. `override` is deliberately excluded — it has its own
    /// unconditional TS1434 recovery (see `parse_statement_top_level_modifier`)
    /// — and so is `export` itself, which terminates the run.
    const fn is_stacked_export_run_modifier(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::StaticKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::DeclareKeyword
        )
    }

    /// True when a run of **two or more** modifier keywords (no intervening
    /// line break) immediately precedes `export as` — a stacked-modifier
    /// `NamespaceExportDeclaration` such as `static readonly export as
    /// namespace N;`. tsc collapses the whole run into one TS1184 at the first
    /// modifier and anchors the declaration's placement diagnostic
    /// (TS1314/TS1315/TS1316) there too, independent of the run's length, the
    /// modifier kinds, their order, or the container. This generalizes the
    /// single-modifier case (#16540) to a run.
    ///
    /// Two runs are deliberately excluded because tsc does **not** take the
    /// uniform path for them, so firing here would produce a *wrong* answer,
    /// not merely a different one (both oracle-pinned, `typescript@7.0.2`):
    /// - a run containing `override` — tsc reports its own TS1434/TS1128
    ///   recovery at the `override` token — which drops out naturally because
    ///   `override` is not an `is_stacked_export_run_modifier`, so the run ends
    ///   there and the `export` check below fails; and
    /// - a run with a repeated `static`, which tsc recovers as TS1146
    ///   ("Declaration expected.") at the second `static`.
    ///
    /// The single-modifier case is left to the ordinary `modified_export_form`
    /// path and excluded here by the two-or-more requirement. The decision
    /// turns only on token *kinds*, never on the exported name or the specific
    /// modifier text (anti-hardcoding).
    fn look_ahead_is_stacked_modifier_run_before_export_as_namespace(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // A first token that is not a run modifier leaves `count` at 0, which
        // the `count >= 2` guard below already rejects — no separate early
        // return is needed.
        let mut count = 0usize;
        let mut static_count = 0usize;
        // Consume the leading run of modifier keywords, requiring same-line
        // adjacency (a line break ends the run — ASI — so the leading modifier
        // is an expression statement instead, matching tsc).
        loop {
            let kind = self.token();
            if !Self::is_stacked_export_run_modifier(kind) {
                break;
            }
            if kind == SyntaxKind::StaticKeyword {
                static_count += 1;
            }
            count += 1;
            self.next_token();
            if self.scanner.has_preceding_line_break() {
                break;
            }
        }

        let matched = count >= 2
            && static_count <= 1
            && self.token() == SyntaxKind::ExportKeyword
            && !self.scanner.has_preceding_line_break()
            && {
                self.next_token(); // skip `export`
                self.token() == SyntaxKind::AsKeyword && !self.scanner.has_preceding_line_break()
            };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        matched
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
                // A named or star export declaration.
                SyntaxKind::OpenBraceToken | SyntaxKind::AsteriskToken => {
                    Some(AbstractExportTarget::PositionErrorWins)
                }
                // `abstract export = expr` — an `ExportAssignment` node, same
                // as `abstract export default <expr>` below: `abstract` is
                // never legal on it, so tsc reports the modifier's own
                // "can only appear on a class, method, or property
                // declaration" message (TS1242) rather than an ordering
                // violation, silenced the same wider way — a Block or a
                // namespace body, not just a Block — by the assignment's own
                // placement diagnostic (oracle-confirmed, #16403 residual).
                SyntaxKind::EqualsToken => Some(AbstractExportTarget::ExportAssignment),
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
                // one TS1029 (oracle-confirmed). `export default function`
                // (named, anonymous, or async) admits no `abstract` either,
                // but `export` is still a legal modifier position on a
                // function declaration, so it takes the ordinary
                // `ModifierRun` container split rather than `Class`'s
                // unconditional TS1029 (oracle-confirmed: TS1242 outside a
                // Block, TS1184 inside one, unaffected by a namespace body).
                // Every other `export default <expr>` form is a value
                // expression, which takes `ExportAssignment`'s wider
                // silencing instead (oracle-confirmed). A second `abstract`
                // is only ever legal directly before `class`; anywhere else
                // it is a separate, pre-existing gap (`abstract export
                // default abstract;`, #16425) and left untouched here.
                SyntaxKind::DefaultKeyword => {
                    self.next_token(); // skip `default`
                    match self.token() {
                        SyntaxKind::AbstractKeyword => {
                            self.next_token(); // skip a second, legal `abstract`
                            matches!(self.token(), SyntaxKind::ClassKeyword)
                                .then_some(AbstractExportTarget::Class)
                        }
                        SyntaxKind::ClassKeyword => Some(AbstractExportTarget::Class),
                        SyntaxKind::FunctionKeyword => Some(AbstractExportTarget::ModifierRun),
                        SyntaxKind::AsyncKeyword => self
                            .look_ahead_is_async_function()
                            .then_some(AbstractExportTarget::ModifierRun),
                        _ => Some(AbstractExportTarget::ExportAssignment),
                    }
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

    /// Classify `async export <declaration>` by the node kind that `export`
    /// decorates, mirroring `look_ahead_abstract_before_export_target` — see
    /// `AsyncExportTarget` for the structural rule per form. Returns `None`
    /// when the current token is not `async` immediately followed by
    /// `export` (every other `async ...` shape is left to its existing
    /// path).
    pub(crate) fn look_ahead_async_before_export_target(&mut self) -> Option<AsyncExportTarget> {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip `async`
        let mut target = None;
        if self.is_token(SyntaxKind::ExportKeyword) {
            self.next_token(); // skip `export`
            target = match self.token() {
                SyntaxKind::AsKeyword => Some(AsyncExportTarget::NamespaceExport),
                SyntaxKind::OpenBraceToken | SyntaxKind::AsteriskToken => {
                    Some(AsyncExportTarget::ExportDeclaration)
                }
                SyntaxKind::EqualsToken => Some(AsyncExportTarget::ExportAssignment),
                // `export default function ...` reads the same "async legal
                // in every container" answer as a bare `export function`.
                // `export default class ...` is the `ModifierRun` node kind
                // instead — `async` is not legal on a class either, so it
                // takes the ordinary container split, not the always-legal
                // answer. Only a bare, non-declaration `export default
                // <expr>` is the `ExportAssignment` node (#16403).
                SyntaxKind::DefaultKeyword => {
                    self.next_token(); // skip `default`
                    Some(match self.token() {
                        SyntaxKind::FunctionKeyword => AsyncExportTarget::Function,
                        SyntaxKind::ClassKeyword => AsyncExportTarget::ModifierRun,
                        _ => AsyncExportTarget::ExportAssignment,
                    })
                }
                SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::GlobalKeyword => self
                    .look_ahead_is_module_declaration()
                    .then_some(AsyncExportTarget::ModuleDeclaration),
                SyntaxKind::FunctionKeyword => Some(AsyncExportTarget::Function),
                // `export type { x }` / `export type * from "m"` is a
                // type-only export declaration, not the type-alias form —
                // same lookahead the `abstract` path above uses.
                SyntaxKind::TypeKeyword => {
                    self.next_token();
                    Some(
                        if self.is_token(SyntaxKind::OpenBraceToken)
                            || self.is_token(SyntaxKind::AsteriskToken)
                        {
                            AsyncExportTarget::ExportDeclaration
                        } else {
                            AsyncExportTarget::ModifierRun
                        },
                    )
                }
                SyntaxKind::ConstKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword => Some(AsyncExportTarget::ModifierRun),
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
    ///
    /// A `PrivateIdentifier` also counts (`let #x = 1;`, `for (let #x of arr)`):
    /// tsc still treats `let` as the declaration keyword there and reports
    /// TS18029 on the binding name, rather than falling back to parsing `let`
    /// as an identifier expression. `is_identifier_or_keyword` (the free
    /// function, not `ParserState::is_identifier_or_keyword`) does not cover
    /// `PrivateIdentifier`, so this needs its own arm.
    pub(crate) fn look_ahead_is_let_declaration(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            is_identifier_or_keyword(token)
                || token == SyntaxKind::OpenBraceToken
                || token == SyntaxKind::OpenBracketToken
                || token == SyntaxKind::PrivateIdentifier
        })
    }

    /// Look ahead to see if a statement-leading `using` starts a `using`
    /// declaration (tsc's `isUsingDeclaration` →
    /// `nextTokenIsBindingIdentifierOrStartOfObjectDestructuringOnSameLine`).
    ///
    /// The binding must be a binding identifier or object-destructuring `{` and
    /// must sit on the **same line** as `using`: `using\nx = 1;` is not a
    /// declaration — ASI ends the `using` expression statement, and `using` /
    /// `x` are two ordinary identifier references (tsc reports TS2304 on each).
    pub(crate) fn look_ahead_is_using_declaration(&mut self) -> bool {
        look_ahead_is_on_same_line(
            &mut self.scanner,
            self.current_token,
            is_using_declaration_binding_start,
        )
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
            is_using_declaration_binding_start(next) && !self.scanner.has_preceding_line_break()
        };

        self.scanner.restore_state(snapshot);
        result
    }

    /// Look ahead to see if a statement-leading `await using` starts an
    /// `await using` declaration (tsc's `isAwaitUsingDeclaration` →
    /// `nextIsUsingKeywordThenBindingIdentifierOrStartOfObjectDestructuringOnSameLine`).
    ///
    /// As with the plain `using` form, the binding after `using` must be a
    /// binding identifier or object-destructuring `{` on the **same line** as
    /// `using`: `await using\nx = 1;` is not a declaration (ASI splits it).
    pub(crate) fn look_ahead_is_await_using_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let t1 = self.scanner.scan();
        let t2 = self.scanner.scan();
        let result = t1 == SyntaxKind::UsingKeyword
            && is_using_declaration_binding_start(t2)
            && !self.scanner.has_preceding_line_break();
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
        // The binding must be on the same line as `using` (ASI): `for (await
        // using\n x of [])` is not a declaration — `using` ends an expression.
        let t2_same_line = !self.scanner.has_preceding_line_break();
        let result = if t1 != SyntaxKind::UsingKeyword {
            false
        } else if t2 == SyntaxKind::OfKeyword {
            // `await using of` — check if the next token is also `of`,
            // meaning the first `of` is the binding name (e.g., `await using of of [...]`).
            let t3 = self.scanner.scan();
            t3 == SyntaxKind::OfKeyword && t2_same_line
        } else if t2 == SyntaxKind::InKeyword {
            false
        } else {
            is_using_declaration_binding_start(t2) && t2_same_line
        };
        self.scanner.restore_state(snapshot);
        result
    }

    #[expect(dead_code)]
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
    #[expect(dead_code)]
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
        let statement = self.parse_embedded_statement();

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
