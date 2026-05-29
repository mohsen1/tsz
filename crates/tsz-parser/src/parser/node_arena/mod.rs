//! `NodeArena` construction APIs.
//!
//! The arena stores AST nodes in a single owning struct ([`NodeArena`], defined
//! in [`super::node`]). Mutation methods (`add_*`) are split across this
//! directory by AST family rather than living in one monolithic file:
//!
//! - [`mod@self`] — lifecycle, parent-pointer helpers, and the leaf node
//!   constructors (`add_token`, `add_identifier`, `add_literal`,
//!   `add_source_file`, modifier tokens).
//! - [`expressions`] — expression node constructors.
//! - [`declarations`] — top-level declarations (functions, classes, interfaces,
//!   type aliases, enums, modules, variable statements).
//! - [`statements`] — control-flow statement node constructors.
//! - [`types`] — type-syntax node constructors.
//! - [`members`] — class/interface/function member node constructors.
//! - [`patterns`] — binding patterns and object-literal property assignments.
//! - [`imports_exports`] — import / export node constructors.
//! - [`jsx`] — JSX node constructors.
//!
//! Storage stays centralized on [`NodeArena`]; each submodule contributes its
//! family's methods via `impl NodeArena { ... }`. The parent-pointer and
//! `len_u32` helpers are `pub(super)` so submodules can call them through
//! `self`.

mod declarations;
mod expressions;
mod imports_exports;
mod jsx;
mod members;
mod patterns;
mod statements;
mod types;

use super::base::{NodeIndex, NodeList};
use super::node::{ExtendedNodeInfo, IdentifierData, LiteralData, Node, NodeArena, SourceFileData};

use tsz_common::interner::{Atom, Interner};

impl NodeArena {
    /// Maximum pre-allocation to avoid capacity overflow in huge files.
    const MAX_NODE_PREALLOC: usize = 5_000_000;

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the interner (called after parsing to transfer ownership from scanner)
    pub fn set_interner(&mut self, interner: Interner) {
        self.interner = interner;
    }

    /// Get a reference to the interner
    #[must_use]
    pub const fn interner(&self) -> &Interner {
        &self.interner
    }

    /// Resolve identifier display text.
    ///
    /// The parser stores the source spelling in `escaped_text` when it builds
    /// an identifier. Prefer that spelling for user-visible text so a stale or
    /// mismatched atom cannot corrupt non-ASCII names; use the atom only for
    /// synthetic identifiers that have no parsed text.
    #[inline]
    #[must_use]
    pub fn resolve_identifier_text<'a>(&'a self, data: &'a IdentifierData) -> &'a str {
        if !data.escaped_text.is_empty() {
            return &data.escaped_text;
        }
        if data.atom == Atom::NONE {
            return &data.escaped_text;
        }
        self.interner.resolve(data.atom)
    }

    /// Create an arena with pre-allocated capacity.
    /// Uses heuristic ratios based on typical TypeScript AST composition.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let safe_capacity = capacity.min(Self::MAX_NODE_PREALLOC);
        // Use Default for all the new pools, just set capacity for main ones
        Self {
            nodes: Vec::with_capacity(safe_capacity),
            extended_info: Vec::with_capacity(safe_capacity),
            identifiers: Vec::with_capacity(safe_capacity / 4), // ~25% identifiers
            literals: Vec::with_capacity(safe_capacity / 8),    // ~12% literals
            binary_exprs: Vec::with_capacity(safe_capacity / 8), // ~12% binary
            call_exprs: Vec::with_capacity(safe_capacity / 8),  // ~12% calls
            access_exprs: Vec::with_capacity(safe_capacity / 8), // ~12% property access
            blocks: Vec::with_capacity(safe_capacity / 8),      // ~12% blocks
            variables: Vec::with_capacity(safe_capacity / 16),  // ~6% variables
            functions: Vec::with_capacity(safe_capacity / 16),  // ~6% functions
            type_refs: Vec::with_capacity(safe_capacity / 8),   // ~12% type refs
            source_files: Vec::with_capacity(1),                // Usually 1
            ..Default::default()
        }
    }

    pub fn clear(&mut self) {
        macro_rules! clear_vecs {
            ($($field:ident),+ $(,)?) => {
                $(self.$field.clear();)+
            };
        }

        clear_vecs!(
            nodes,
            identifiers,
            qualified_names,
            computed_properties,
            literals,
            binary_exprs,
            unary_exprs,
            call_exprs,
            access_exprs,
            conditional_exprs,
            literal_exprs,
            parenthesized,
            unary_exprs_ex,
            type_assertions,
            template_exprs,
            template_spans,
            tagged_templates,
            functions,
            classes,
            interfaces,
            type_aliases,
            enums,
            enum_members,
            modules,
            module_blocks,
            signatures,
            index_signatures,
            property_decls,
            method_decls,
            constructors,
            accessors,
            parameters,
            type_parameters,
            decorators,
            heritage_clauses,
            expr_with_type_args,
            if_statements,
            loops,
            blocks,
            variables,
            return_data,
            expr_statements,
            switch_data,
            case_clauses,
            try_data,
            catch_clauses,
            labeled_data,
            jump_data,
            with_data,
            type_refs,
            composite_types,
            function_types,
            type_queries,
            type_literals,
            array_types,
            tuple_types,
            wrapped_types,
            conditional_types,
            infer_types,
            type_operators,
            indexed_access_types,
            mapped_types,
            literal_types,
            template_literal_types,
            named_tuple_members,
            type_predicates,
            import_decls,
            import_clauses,
            named_imports,
            specifiers,
            export_decls,
            export_assignments,
            import_attributes,
            import_attribute,
            binding_patterns,
            binding_elements,
            property_assignments,
            shorthand_properties,
            spread_data,
            variable_declarations,
            for_in_of,
            jsx_elements,
            jsx_opening,
            jsx_closing,
            jsx_fragments,
            jsx_attributes,
            jsx_attribute,
            jsx_spread_attributes,
            jsx_expressions,
            jsx_text,
            jsx_namespaced_names,
            source_files,
            extended_info,
        );
    }

    #[inline]
    #[must_use]
    pub(super) fn len_u32(&self, len: usize) -> u32 {
        let _ = self;
        u32::try_from(len).expect(
            "node arena length exceeds u32::MAX; large AST support requires a larger span type",
        )
    }

    // ============================================================================
    // Parent Mapping Helpers
    // ============================================================================

    /// Set the parent for a single child node.
    /// This is called during node creation to maintain parent pointers.
    #[inline]
    pub(super) fn set_parent(&mut self, child: NodeIndex, parent: NodeIndex) {
        if child.is_some() {
            // Safety: child index is guaranteed to be valid and < current index
            // because we build bottom-up (children are created before parents).
            if let Some(info) = self.extended_info.get_mut(child.0 as usize) {
                info.parent = parent;
            }
        }
    }

    /// Set the parent for a list of children.
    #[inline]
    pub(super) fn set_parent_list(&mut self, list: &NodeList, parent: NodeIndex) {
        for &child in &list.nodes {
            self.set_parent(child, parent);
        }
    }

    /// Set the parent for an optional list of children.
    #[inline]
    pub(super) fn set_parent_opt_list(&mut self, list: Option<&NodeList>, parent: NodeIndex) {
        if let Some(l) = list {
            self.set_parent_list(l, parent);
        }
    }

    // ============================================================================
    // Leaf / Lifecycle Node Constructors
    // ============================================================================

    /// Add a token node (no additional data)
    pub fn add_token(&mut self, kind: u16, pos: u32, end: u32) -> NodeIndex {
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::new(kind, pos, end));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Create a modifier token (static, public, private, etc.)
    pub fn create_modifier(&mut self, kind: tsz_scanner::SyntaxKind, pos: u32) -> NodeIndex {
        // Modifiers are simple tokens, their kind IS the modifier type
        // End position is pos + keyword length
        let end = pos
            + match kind {
                tsz_scanner::SyntaxKind::AsyncKeyword | tsz_scanner::SyntaxKind::ConstKeyword => 5,
                tsz_scanner::SyntaxKind::StaticKeyword
                | tsz_scanner::SyntaxKind::PublicKeyword
                | tsz_scanner::SyntaxKind::ExportKeyword => 6,
                tsz_scanner::SyntaxKind::PrivateKeyword
                | tsz_scanner::SyntaxKind::DefaultKeyword
                | tsz_scanner::SyntaxKind::DeclareKeyword => 7,
                tsz_scanner::SyntaxKind::ReadonlyKeyword
                | tsz_scanner::SyntaxKind::AbstractKeyword
                | tsz_scanner::SyntaxKind::OverrideKeyword => 8,
                tsz_scanner::SyntaxKind::ProtectedKeyword => 9,
                _ => 0,
            };
        self.add_token(kind as u16, pos, end)
    }

    /// Add an identifier node
    pub fn add_identifier(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IdentifierData,
    ) -> NodeIndex {
        let data_index = self.len_u32(self.identifiers.len());
        self.identifiers.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a literal node
    pub fn add_literal(&mut self, kind: u16, pos: u32, end: u32, data: LiteralData) -> NodeIndex {
        let data_index = self.len_u32(self.literals.len());
        self.literals.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a source file node
    pub fn add_source_file(&mut self, pos: u32, end: u32, data: SourceFileData) -> NodeIndex {
        use super::syntax_kind_ext::SOURCE_FILE;
        let statements = data.statements.clone();
        let end_of_file_token = data.end_of_file_token;

        let data_index = self.len_u32(self.source_files.len());
        self.source_files.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes
            .push(Node::with_data(SOURCE_FILE, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&statements, parent);
        self.set_parent(end_of_file_token, parent);

        parent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimated_size_bytes_is_nonzero_for_empty_arena() {
        let arena = NodeArena::new();
        let size = arena.estimated_size_bytes();
        // Even an empty arena has struct overhead + vec capacities
        assert!(
            size > 0,
            "estimated_size_bytes should be nonzero for a fresh arena"
        );
    }

    #[test]
    fn estimated_size_bytes_grows_with_nodes() {
        let mut arena = NodeArena::new();
        let empty_size = arena.estimated_size_bytes();

        // Add some nodes
        for i in 0..100 {
            arena.add_token(1, i * 10, i * 10 + 5);
        }
        let populated_size = arena.estimated_size_bytes();

        assert!(
            populated_size > empty_size,
            "estimated_size_bytes should grow after adding nodes: {empty_size} -> {populated_size}"
        );
    }

    #[test]
    fn estimated_size_bytes_accounts_for_interner() {
        let mut arena = NodeArena::new();
        let before = arena.estimated_size_bytes();

        // Intern many strings
        for i in 0..200 {
            let _ = arena.interner.intern(&format!("identifier_{i}"));
        }
        let after = arena.estimated_size_bytes();

        assert!(
            after > before,
            "estimated_size_bytes should grow with interned strings: {before} -> {after}"
        );
    }

    #[test]
    #[should_panic(
        expected = "node arena length exceeds u32::MAX; large AST support requires a larger span type"
    )]
    fn len_u32_overflow_panics_with_expected_message() {
        let arena = NodeArena::new();
        let _ = arena.len_u32(usize::MAX);
    }

    /// Workstream-7 deliverable 3 ("Add a defensive identifier text
    /// resolution path only if it is consistent with the parser identity
    /// model"): when an `IdentifierData.atom` is set but the arena's
    /// interner returns `""` for it (the stale-interner regression PR #1205
    /// fixed for incremental parse), `resolve_identifier_text` must fall
    /// back to `escaped_text` rather than silently surface the empty
    /// string.
    #[test]
    fn resolve_identifier_text_falls_back_to_escaped_when_interner_stale() {
        let mut arena = NodeArena::new();
        // Use `Interner::new()` so Atom(0) is reserved for the empty
        // string (the production scanner setup); without this, the
        // default-constructed interner gives Atom(0) to the first
        // interned string, which the resolver classifies as Atom::NONE.
        arena.set_interner(Interner::new());
        // Construct an atom that the arena's freshly-created interner does
        // not have — Atom(99_999) is well past any populated index.
        let stale_atom = Atom(99_999);
        assert!(arena.interner().resolve(stale_atom).is_empty());

        let data = IdentifierData {
            atom: stale_atom,
            escaped_text: "uniquely_named_identifier".to_string(),
            original_text: None,
            type_arguments: None,
        };

        assert_eq!(
            arena.resolve_identifier_text(&data),
            "uniquely_named_identifier",
            "stale interner must not produce an empty identifier — fall back to escaped_text"
        );
    }

    /// Sanity check: parsed identifier text is authoritative for display even
    /// when an atom resolves to a different string.
    ///
    /// Use `Interner::new()` (which reserves Atom(0) for the empty string,
    /// matching the production scanner setup) rather than `Default::default`
    /// so the first interned string gets `Atom(1)`, not `Atom(0)` (which
    /// the resolver classifies as `Atom::NONE`).
    #[test]
    fn resolve_identifier_text_prefers_escaped_text_when_atom_resolves() {
        let mut arena = NodeArena::new();
        arena.set_interner(Interner::new());
        let atom = arena.interner.intern("canonical_text");
        assert_ne!(atom, Atom::NONE, "intern result must not be Atom::NONE");

        let data = IdentifierData {
            atom,
            // escaped_text intentionally differs from the canonical so we
            // can confirm which branch was taken.
            escaped_text: "stale_escaped_form".to_string(),
            original_text: None,
            type_arguments: None,
        };

        assert_eq!(arena.resolve_identifier_text(&data), "stale_escaped_form");
    }
}
