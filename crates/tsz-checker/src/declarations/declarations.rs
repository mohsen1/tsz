use crate::context::CheckerContext;

use crate::diagnostics::format_message;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::{NodeIndex, node_flags, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

/// Declaration type checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All declaration type checking goes through this checker.
pub struct DeclarationChecker<'a, 'ctx> {
    pub ctx: &'a mut CheckerContext<'ctx>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IsolatedEnumInitializerKind {
    LiteralNumeric,
    NonLiteralNumeric,
    LiteralString,
    NonLiteralString,
    Other,
}

include!("declarations_parts/part1.rs");
include!("declarations_parts/part2.rs");

#[cfg(test)]
#[path = "../../tests/declarations.rs"]
mod tests;
