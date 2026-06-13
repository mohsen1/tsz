//! Shared symbol-presentation classifiers for `tsz-lsp`.
//!
//! Hover, completions, document-symbols, navigation, and rename all need to map
//! a binder [`Symbol`](tsz_binder::Symbol) (or a variable declaration node) onto
//! a presentation kind. Historically each provider open-coded the same three
//! decisions independently and the copies drifted. This module owns those
//! decisions once:
//!
//! 1. [`classify_symbol_flags`] reduces a `symbol_flags` bitset to a single
//!    [`LspSymbolClass`]. Each provider derives its own enum/label from the
//!    class via the `to_*` accessors, so the flag-priority cascade lives here
//!    rather than in every provider.
//! 2. [`kind_modifiers`] builds the tsserver `kindModifiers` vocabulary from
//!    symbol flags plus declaration `modifier_flags` in one place.
//! 3. [`variable_decl_kind`] performs the `const`/`let`/`var` parent walk once.
//!
//! Call sites differ only in the presentation target they request; the
//! classification itself has one owner.

use tsz_binder::{Symbol, symbol_flags};
use tsz_parser::parser::flags::node_flags;
use tsz_parser::{NodeArena, NodeIndex, modifier_flags, syntax_kind_ext};

use crate::completions::CompletionItemKind;
use crate::rename::RenameSymbolKind;
use crate::symbols::document_symbols::SymbolKind;

/// Canonical presentation class for a binder symbol.
///
/// This is the single source of truth that every provider's presentation enum
/// or label is derived from. The cascade in [`classify_symbol_flags`] decides
/// the class once; the `to_*` accessors translate it to each provider's target
/// vocabulary without re-implementing the flag priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspSymbolClass {
    /// `import`/`export` alias re-export of another symbol.
    Alias,
    Function,
    Class,
    Interface,
    Enum,
    EnumMember,
    /// `type X = ...` alias.
    TypeAlias,
    TypeParameter,
    Module,
    Method,
    Property,
    Constructor,
    Accessor,
    /// `let`/`const` (block-scoped) binding.
    BlockScopedVariable,
    /// `var`/parameter (function-scoped) binding.
    FunctionScopedVariable,
    /// No recognised presentation flag.
    Other,
}

/// Reduce a `symbol_flags` bitset to a single [`LspSymbolClass`].
///
/// The priority cascade is the union of the historical per-provider cascades,
/// ordered most-specific first so that, e.g., a const-enum is classified as
/// `Enum` rather than a variable and an import alias wins over the value it
/// re-exports. Providers that intentionally do not surface a class (for
/// example, the document-symbol index does not surface aliases) collapse it in
/// their own accessor rather than by reordering this cascade.
#[must_use]
pub const fn classify_symbol_flags(flags: u32) -> LspSymbolClass {
    if flags & symbol_flags::ALIAS != 0 {
        LspSymbolClass::Alias
    } else if flags & symbol_flags::FUNCTION != 0 {
        LspSymbolClass::Function
    } else if flags & symbol_flags::CLASS != 0 {
        LspSymbolClass::Class
    } else if flags & symbol_flags::INTERFACE != 0 {
        LspSymbolClass::Interface
    } else if flags & symbol_flags::ENUM != 0 {
        LspSymbolClass::Enum
    } else if flags & symbol_flags::ENUM_MEMBER != 0 {
        LspSymbolClass::EnumMember
    } else if flags & symbol_flags::TYPE_ALIAS != 0 {
        LspSymbolClass::TypeAlias
    } else if flags & symbol_flags::TYPE_PARAMETER != 0 {
        LspSymbolClass::TypeParameter
    } else if flags & symbol_flags::MODULE != 0 {
        LspSymbolClass::Module
    } else if flags & symbol_flags::METHOD != 0 {
        LspSymbolClass::Method
    } else if flags & symbol_flags::CONSTRUCTOR != 0 {
        LspSymbolClass::Constructor
    } else if flags & symbol_flags::PROPERTY != 0 {
        LspSymbolClass::Property
    } else if flags & symbol_flags::ACCESSOR != 0 {
        LspSymbolClass::Accessor
    } else if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
        LspSymbolClass::BlockScopedVariable
    } else if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
        LspSymbolClass::FunctionScopedVariable
    } else {
        LspSymbolClass::Other
    }
}

impl LspSymbolClass {
    /// Document-outline / workspace-symbol [`SymbolKind`].
    ///
    /// The LSP `SymbolKind` enum has no alias/getter/setter distinction, so
    /// `Alias` and accessors collapse onto the nearest representable kind and
    /// `EnumMember`/`TypeParameter` map to their dedicated values. Block-scoped
    /// and function-scoped variables both report `Variable`; callers that need
    /// to distinguish `const` use [`variable_decl_kind`] on the declaration.
    #[must_use]
    pub const fn to_symbol_kind(self) -> SymbolKind {
        match self {
            Self::Function => SymbolKind::Function,
            Self::Class => SymbolKind::Class,
            Self::Interface => SymbolKind::Interface,
            Self::Enum => SymbolKind::Enum,
            Self::EnumMember => SymbolKind::EnumMember,
            Self::TypeAlias | Self::TypeParameter => SymbolKind::TypeParameter,
            Self::Module => SymbolKind::Module,
            Self::Method => SymbolKind::Method,
            Self::Property | Self::Accessor => SymbolKind::Property,
            Self::Constructor => SymbolKind::Constructor,
            Self::Alias
            | Self::BlockScopedVariable
            | Self::FunctionScopedVariable
            | Self::Other => SymbolKind::Variable,
        }
    }

    /// Completion-list [`CompletionItemKind`].
    ///
    /// `const`/`let` resolution for block-scoped variables is left to the
    /// caller (it needs arena access); this returns [`CompletionItemKind::Let`]
    /// as the block-scoped default and the caller upgrades to `Const` when the
    /// declaration is `const`. The completion vocabulary has no dedicated
    /// enum-member or accessor variant, so those fall back to the same kinds
    /// the completion provider already produced (`Variable`/`Property`).
    #[must_use]
    pub const fn to_completion_kind(self) -> CompletionItemKind {
        match self {
            Self::Alias => CompletionItemKind::Alias,
            Self::Constructor => CompletionItemKind::Constructor,
            Self::Function => CompletionItemKind::Function,
            Self::Class => CompletionItemKind::Class,
            Self::Interface => CompletionItemKind::Interface,
            Self::Enum => CompletionItemKind::Enum,
            Self::TypeAlias => CompletionItemKind::TypeAlias,
            Self::TypeParameter => CompletionItemKind::TypeParameter,
            Self::Method => CompletionItemKind::Method,
            Self::Property => CompletionItemKind::Property,
            Self::Module => CompletionItemKind::Module,
            Self::BlockScopedVariable => CompletionItemKind::Let,
            Self::Accessor | Self::EnumMember | Self::FunctionScopedVariable | Self::Other => {
                CompletionItemKind::Variable
            }
        }
    }

    /// Rename presentation [`RenameSymbolKind`].
    ///
    /// `const`/`let`/`parameter` resolution needs arena access and is performed
    /// by the caller; this returns [`RenameSymbolKind::Let`] for block-scoped
    /// and [`RenameSymbolKind::Var`] for function-scoped bindings as defaults.
    #[must_use]
    pub const fn to_rename_kind(self) -> RenameSymbolKind {
        match self {
            Self::Alias => RenameSymbolKind::Alias,
            Self::Function => RenameSymbolKind::Function,
            Self::Class => RenameSymbolKind::Class,
            Self::Interface => RenameSymbolKind::Interface,
            Self::Enum => RenameSymbolKind::Enum,
            Self::EnumMember => RenameSymbolKind::EnumMember,
            Self::TypeAlias => RenameSymbolKind::TypeAlias,
            Self::TypeParameter => RenameSymbolKind::TypeParameter,
            Self::Module => RenameSymbolKind::Module,
            Self::Method => RenameSymbolKind::Method,
            Self::Property => RenameSymbolKind::Property,
            // Rename has no constructor/accessor presentation kind.
            Self::Constructor | Self::Accessor | Self::Other => RenameSymbolKind::Unknown,
            Self::BlockScopedVariable => RenameSymbolKind::Let,
            Self::FunctionScopedVariable => RenameSymbolKind::Var,
        }
    }

    /// tsserver `ScriptElementKind`-style label (the kind string surfaced by
    /// hover and navigation).
    ///
    /// Block-scoped (`const`/`let`) and function-scoped (`var`/`parameter`)
    /// bindings, plus getter/setter disambiguation, need the declaration node
    /// and are resolved by the caller; this returns the block-scoped default
    /// `"let"`, the function-scoped default `"var"`, and `"getter"` for
    /// accessors.
    #[must_use]
    pub const fn tsserver_kind_str(self) -> &'static str {
        match self {
            Self::Alias => "alias",
            Self::Function => "function",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::EnumMember => "enum member",
            Self::TypeAlias => "type",
            Self::TypeParameter => "type parameter",
            Self::Module => "module",
            Self::Method => "method",
            Self::Property => "property",
            Self::Constructor => "constructor",
            Self::Accessor => "getter",
            Self::BlockScopedVariable => "let",
            Self::FunctionScopedVariable => "var",
            Self::Other => "",
        }
    }

    /// Hover detail label (the short noun shown in completion/hover details).
    ///
    /// Returns `None` for classes of symbol that the detail panel does not
    /// label.
    #[must_use]
    pub const fn detail_str(self) -> Option<&'static str> {
        match self {
            Self::Function => Some("function"),
            Self::Class => Some("class"),
            Self::Interface => Some("interface"),
            Self::Enum => Some("enum"),
            Self::TypeAlias => Some("type"),
            Self::TypeParameter => Some("type parameter"),
            Self::Method => Some("method"),
            Self::Property => Some("property"),
            Self::BlockScopedVariable => Some("let/const"),
            Self::FunctionScopedVariable => Some("var"),
            Self::Module => Some("module"),
            Self::Alias | Self::EnumMember | Self::Constructor | Self::Accessor | Self::Other => {
                None
            }
        }
    }
}

/// Resolution of a variable declaration's binding keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    Const,
    Let,
    Var,
}

/// Resolve whether a declaration node belongs to a `const`, `let`, or `var`
/// binding by walking up to the enclosing `VariableDeclarationList`.
///
/// `decl_idx` may be the identifier, the `VariableDeclaration`, or the
/// `VariableDeclarationList` itself; the walk climbs at most `max_hops` parents
/// looking for a `VariableDeclarationList` and reads its `CONST`/`LET` flags.
/// Falls back to [`VarKind::Var`] when no list is found.
#[must_use]
pub fn variable_decl_kind(arena: &NodeArena, decl_idx: NodeIndex, max_hops: usize) -> VarKind {
    let mut current = decl_idx;
    for hop in 0..=max_hops {
        let Some(node) = arena.get(current) else {
            break;
        };
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            let flags = node.flags as u32;
            if flags & node_flags::CONST != 0 {
                return VarKind::Const;
            }
            if flags & node_flags::LET != 0 {
                return VarKind::Let;
            }
            return VarKind::Var;
        }
        if hop == max_hops {
            break;
        }
        match arena.get_extended(current) {
            Some(ext) => current = ext.parent,
            None => break,
        }
    }
    VarKind::Var
}

/// Whether the declaration belongs to a `const` binding.
///
/// Convenience wrapper over [`variable_decl_kind`] for the common
/// `const`-vs-not decision.
#[must_use]
pub fn is_const_decl(arena: &NodeArena, decl_idx: NodeIndex, max_hops: usize) -> bool {
    matches!(
        variable_decl_kind(arena, decl_idx, max_hops),
        VarKind::Const
    )
}

/// Build the tsserver `kindModifiers` vocabulary for a symbol.
///
/// Reads symbol flags plus the `modifier_flags` of each declaration so every
/// provider emits the same modifier set. The full vocabulary is covered:
/// `export`, `declare`, `deprecated`, `abstract`, `static`, `private`,
/// `protected`, `readonly`, `async`, `optional`, `default`. Modifiers are
/// pushed in tsserver order and de-duplicated.
#[must_use]
pub fn kind_modifiers(arena: &NodeArena, symbol: &Symbol) -> Vec<&'static str> {
    let mut mods: Vec<&'static str> = Vec::new();
    let push = |mods: &mut Vec<&'static str>, m: &'static str| {
        if !mods.contains(&m) {
            mods.push(m);
        }
    };

    if symbol.is_exported || symbol.has_any_flags(symbol_flags::EXPORT_VALUE) {
        push(&mut mods, "export");
    }

    // Declaration-node modifier flags (declare/async/readonly/default), plus
    // the JSDoc `@deprecated` node flag.
    for decl_idx in symbol.all_declarations() {
        if let Some(node) = arena.get(decl_idx)
            && (node.flags as u32) & node_flags::DEPRECATED != 0
        {
            push(&mut mods, "deprecated");
        }
        if let Some(ext) = arena.get_extended(decl_idx) {
            let mf = ext.modifier_flags;
            if mf & modifier_flags::AMBIENT != 0 {
                push(&mut mods, "declare");
            }
            if mf & modifier_flags::EXPORT != 0 {
                push(&mut mods, "export");
            }
            if mf & modifier_flags::ASYNC != 0 {
                push(&mut mods, "async");
            }
            if mf & modifier_flags::READONLY != 0 {
                push(&mut mods, "readonly");
            }
            if mf & modifier_flags::DEFAULT != 0 {
                push(&mut mods, "default");
            }
        }
    }

    if symbol.has_any_flags(symbol_flags::ABSTRACT) {
        push(&mut mods, "abstract");
    }
    if symbol.has_any_flags(symbol_flags::STATIC) {
        push(&mut mods, "static");
    }
    if symbol.has_any_flags(symbol_flags::PRIVATE) {
        push(&mut mods, "private");
    }
    if symbol.has_any_flags(symbol_flags::PROTECTED) {
        push(&mut mods, "protected");
    }
    if symbol.has_any_flags(symbol_flags::OPTIONAL) {
        push(&mut mods, "optional");
    }

    mods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_core_flag_vocabulary() {
        assert_eq!(
            classify_symbol_flags(symbol_flags::FUNCTION),
            LspSymbolClass::Function
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::CLASS),
            LspSymbolClass::Class
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::INTERFACE),
            LspSymbolClass::Interface
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::REGULAR_ENUM),
            LspSymbolClass::Enum
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::CONST_ENUM),
            LspSymbolClass::Enum
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::ENUM_MEMBER),
            LspSymbolClass::EnumMember
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::TYPE_ALIAS),
            LspSymbolClass::TypeAlias
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::TYPE_PARAMETER),
            LspSymbolClass::TypeParameter
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::VALUE_MODULE),
            LspSymbolClass::Module
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::ALIAS),
            LspSymbolClass::Alias
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::BLOCK_SCOPED_VARIABLE),
            LspSymbolClass::BlockScopedVariable
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE),
            LspSymbolClass::FunctionScopedVariable
        );
        assert_eq!(classify_symbol_flags(0), LspSymbolClass::Other);
    }

    #[test]
    fn alias_wins_over_re_exported_value() {
        // An import/export alias is presented as an alias regardless of the
        // value flags it may also carry.
        let flags = symbol_flags::ALIAS | symbol_flags::FUNCTION;
        assert_eq!(classify_symbol_flags(flags), LspSymbolClass::Alias);
    }

    #[test]
    fn const_enum_is_enum_not_variable() {
        // Specificity: a const-enum must classify as `Enum`, never a variable.
        let flags = symbol_flags::CONST_ENUM | symbol_flags::BLOCK_SCOPED_VARIABLE;
        assert_eq!(classify_symbol_flags(flags), LspSymbolClass::Enum);
    }

    #[test]
    fn symbol_kind_collapses_non_lsp_classes() {
        assert_eq!(LspSymbolClass::Alias.to_symbol_kind(), SymbolKind::Variable);
        assert_eq!(
            LspSymbolClass::Accessor.to_symbol_kind(),
            SymbolKind::Property
        );
        assert_eq!(
            LspSymbolClass::TypeAlias.to_symbol_kind(),
            SymbolKind::TypeParameter
        );
        assert_eq!(
            LspSymbolClass::EnumMember.to_symbol_kind(),
            SymbolKind::EnumMember
        );
    }

    #[test]
    fn completion_kind_block_scoped_default_is_let() {
        // Const/let split is the caller's job; the class default is Let.
        assert_eq!(
            LspSymbolClass::BlockScopedVariable.to_completion_kind(),
            CompletionItemKind::Let
        );
        assert_eq!(
            LspSymbolClass::Alias.to_completion_kind(),
            CompletionItemKind::Alias
        );
        assert_eq!(
            LspSymbolClass::Accessor.to_completion_kind(),
            CompletionItemKind::Variable
        );
    }

    #[test]
    fn tsserver_kind_str_labels() {
        assert_eq!(LspSymbolClass::Alias.tsserver_kind_str(), "alias");
        assert_eq!(
            LspSymbolClass::EnumMember.tsserver_kind_str(),
            "enum member"
        );
        assert_eq!(
            LspSymbolClass::TypeParameter.tsserver_kind_str(),
            "type parameter"
        );
        assert_eq!(LspSymbolClass::Other.tsserver_kind_str(), "");
    }

    #[test]
    fn detail_str_labels_and_gaps() {
        assert_eq!(
            LspSymbolClass::BlockScopedVariable.detail_str(),
            Some("let/const")
        );
        assert_eq!(
            LspSymbolClass::FunctionScopedVariable.detail_str(),
            Some("var")
        );
        assert_eq!(LspSymbolClass::Alias.detail_str(), None);
        assert_eq!(LspSymbolClass::EnumMember.detail_str(), None);
    }

    #[test]
    fn rename_kind_has_no_constructor_or_accessor() {
        assert_eq!(
            LspSymbolClass::Constructor.to_rename_kind(),
            RenameSymbolKind::Unknown
        );
        assert_eq!(
            LspSymbolClass::Accessor.to_rename_kind(),
            RenameSymbolKind::Unknown
        );
        assert_eq!(
            LspSymbolClass::FunctionScopedVariable.to_rename_kind(),
            RenameSymbolKind::Var
        );
    }
}
