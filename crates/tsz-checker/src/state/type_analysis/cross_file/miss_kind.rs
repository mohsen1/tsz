//! Cross-arena symbol miss-kind classification helpers.
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;

impl<'a> CheckerState<'a> {
    pub(in crate::state_domain::type_analysis) fn cross_arena_symbol_miss_kind(
        &self,
        sym_id: SymbolId,
    ) -> CrossArenaSymbolMissKind {
        let Some(flags) = self
            .get_cross_file_symbol(sym_id)
            .map(|symbol| symbol.flags)
        else {
            return CrossArenaSymbolMissKind::Unresolved;
        };

        if flags & symbol_flags::TYPE_ALIAS != 0 {
            CrossArenaSymbolMissKind::TypeAlias
        } else if flags & symbol_flags::INTERFACE != 0 {
            CrossArenaSymbolMissKind::Interface
        } else if flags & symbol_flags::CLASS != 0 {
            CrossArenaSymbolMissKind::Class
        } else if flags & symbol_flags::FUNCTION != 0 {
            CrossArenaSymbolMissKind::Function
        } else if flags & symbol_flags::VARIABLE != 0 {
            CrossArenaSymbolMissKind::Variable
        } else if flags & symbol_flags::PROPERTY != 0 {
            CrossArenaSymbolMissKind::Property
        } else if flags & symbol_flags::METHOD != 0 {
            CrossArenaSymbolMissKind::Method
        } else if flags & symbol_flags::ACCESSOR != 0 {
            CrossArenaSymbolMissKind::Accessor
        } else if flags & symbol_flags::ENUM != 0 {
            CrossArenaSymbolMissKind::Enum
        } else if flags & symbol_flags::MODULE != 0 {
            CrossArenaSymbolMissKind::Module
        } else if flags & symbol_flags::ALIAS != 0 {
            CrossArenaSymbolMissKind::Alias
        } else if flags & symbol_flags::TYPE_PARAMETER != 0 {
            CrossArenaSymbolMissKind::TypeParameter
        } else if flags & symbol_flags::TYPE_LITERAL != 0 {
            CrossArenaSymbolMissKind::TypeLiteral
        } else if flags & symbol_flags::SIGNATURE != 0 {
            CrossArenaSymbolMissKind::Signature
        } else if flags & symbol_flags::CONSTRUCTOR != 0 {
            CrossArenaSymbolMissKind::Constructor
        } else if flags & symbol_flags::OBJECT_LITERAL != 0 {
            CrossArenaSymbolMissKind::ObjectLiteral
        } else {
            CrossArenaSymbolMissKind::Other
        }
    }
}
