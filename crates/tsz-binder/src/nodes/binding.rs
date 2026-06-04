use std::sync::Arc;

use crate::{ContainerKind, SymbolId, flow_flags, symbol_flags};

use tsz_parser::parser::node::{Node, NodeArena};

use tsz_parser::parser::node_flags;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

use crate::state::{BinderState, FileFeatures};

use smallvec::SmallVec;

type DeclSpan = Option<(u32, u32)>;

type PreservedDecl = (NodeIndex, DeclSpan);

type PreservedDeclArenaEntry = (NodeIndex, DeclSpan, SmallVec<[Arc<NodeArena>; 1]>);

/// Lib-symbol meaning carried over to a module-local shadowing symbol.
///
/// See [`BinderState::collect_preserved_lib_meaning`].
#[derive(Default)]
struct PreservedLibMeaning {
    /// Lib flags that belong to the namespace the local declaration does NOT
    /// occupy (e.g. lib's INTERFACE flag when shadowing with `const X = ...`).
    flags: u32,
    /// Lib declarations to copy onto the new shadow symbol's `declarations`
    /// vec. Each entry is `(decl_node_idx, span)`.
    declarations: Vec<PreservedDecl>,
    /// Per-declaration arena entries to copy into `declaration_arenas` so the
    /// checker can resolve each declaration back to its owning lib arena.
    declaration_arenas: Vec<PreservedDeclArenaEntry>,
    /// Lib's `value_declaration` to adopt when the local doesn't supply one.
    value_declaration: Option<PreservedDecl>,
}

include!("binding_parts/part1.rs");
include!("binding_parts/part2.rs");
