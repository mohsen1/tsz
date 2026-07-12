//! Binder-backed per-import-binding value-usage facts for JS-emit import
//! elision.
//!
//! tsc decides import elision through checker alias-reference marking: an
//! import binding survives JS emit only when it is referenced in at least one
//! *value* position. tsz historically approximated that question by scanning
//! raw source text (`crate::import_usage`), which is a standing parity-risk
//! surface (multi-line generics, strings, shadowing, identifiers named like
//! keywords).
//!
//! This module computes the same per-binding facts once per file from
//! `BinderState` symbol resolution plus a syntactic position classification of
//! every identifier reference. The facts are threaded into emit through
//! `PrinterOptions::import_usage_facts` (the same channel as
//! `type_only_nodes`) and consumed by the elision decision sites in
//! `emitter/module_emission/imports.rs` and `lowering/import_usage.rs` as
//! table lookups. Binder-less paths (transpile-style emit, direct `Printer`
//! construction) leave the facts unset and keep the conservative text-based
//! fallback.
//!
//! Classification policy: only positions that are *provably* erased at
//! runtime (type annotations, `typeof` type queries, interface/type-alias
//! bodies, `implements` clauses, ambient declarations) are treated as
//! non-value. Unknown or unresolvable references conservatively count as
//! value usages so the failure mode is over-preserving an import, never
//! eliding one that is required at runtime.

use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Per-file, per-import-binding value-usage facts.
///
/// Keys are the *binding name* identifier nodes of import declarations: the
/// default-import name, the namespace-import name, each named-import
/// specifier's local name, and the alias identifier of
/// `import X = ...` declarations.
#[derive(Debug, Default, Clone)]
pub struct ImportValueUsageFacts {
    /// Every import-binding name node that was analyzed.
    known_bindings: FxHashSet<NodeIndex>,
    /// The subset of `known_bindings` referenced in at least one value
    /// position.
    value_used: FxHashSet<NodeIndex>,
}

impl ImportValueUsageFacts {
    /// Whether the import binding declared by `name_idx` is referenced in a
    /// value position somewhere in the file.
    ///
    /// Returns `None` when `name_idx` is not a binding this analysis knows
    /// about (callers should fall back to their conservative path).
    #[must_use]
    pub fn binding_value_used(&self, name_idx: NodeIndex) -> Option<bool> {
        self.known_bindings
            .contains(&name_idx)
            .then(|| self.value_used.contains(&name_idx))
    }
}

/// Inputs that mirror the policy the text-based decision sites already apply.
#[derive(Debug, Default, Clone, Copy)]
pub struct ImportValueUsageInputs<'a> {
    /// Local binding names that refer to external const enums. Qualified
    /// accesses through these bindings (`E.Member`, `E["Member"]`) are inlined
    /// during emit and therefore do not keep the binding alive.
    pub external_const_enum_bindings: Option<&'a FxHashSet<String>>,
    /// Specifier nodes the checker marked as type-only (re-exports of
    /// interfaces/type aliases etc.). Export specifiers in this set do not
    /// count as value references.
    pub type_only_nodes: Option<&'a FxHashSet<NodeIndex>>,
}

/// Returns true when the file contains any import declaration whose elision
/// decision the facts can own. Lets callers skip binder construction for
/// files without imports.
#[must_use]
pub fn file_has_import_declarations(arena: &NodeArena) -> bool {
    arena.nodes.iter().any(|node| {
        node.kind == syntax_kind_ext::IMPORT_DECLARATION
            || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
    })
}

/// How an identifier occurrence relates to runtime code.
enum ReferencePosition {
    /// Not a reference at all (declaration name, member name, label, ...).
    NotAReference,
    /// A reference that is erased from JS output (type annotation, `typeof`
    /// query, interface body, ambient declaration, ...).
    Erased,
    /// A runtime value reference.
    Value,
    /// The root of the entity name on the right-hand side of
    /// `import A = X.Y;` — a value reference only if the alias itself
    /// survives emit. `0` is the `IMPORT_EQUALS_DECLARATION` node.
    ImportEqualsRhs(NodeIndex),
}

struct Binding {
    /// The binding's local-name identifier node.
    name_node: NodeIndex,
    /// The binder symbol for the binding, when known.
    symbol: Option<SymbolId>,
    /// Whether this is an `import A = ...` alias carrying an `export`
    /// modifier (always emitted, so its RHS reference is always live).
    exported_import_equals: bool,
    /// For `import A = ...` aliases, the AST parent of the declaration.
    /// Duplicate same-named aliases in one container resolve to a single
    /// declaration, but the emitter's duplicate suppression decides which
    /// one survives, so a value use must keep all of them alive.
    import_equals_container: Option<NodeIndex>,
}

/// Which import bindings a resolved reference denotes.
#[derive(Clone, Copy)]
enum ReferenceTarget {
    /// A shadowing local or unrelated symbol: no binding matches.
    None,
    /// Resolved to exactly this binding.
    Binding(usize),
    /// Unresolvable reference: every same-named binding conservatively
    /// matches so the import is preserved.
    AllSameName,
    /// Resolved to a foreign symbol: only same-named bindings whose own
    /// symbol is unknown conservatively match.
    SameNameWithUnknownSymbol,
}

/// Invoke `apply` with the binding index of every binding matched by
/// `target` among the same-named `candidates`. Free function (rather than a
/// `UsageScan` method) so callers can mutate sibling `UsageScan` fields from
/// `apply`.
fn for_each_matching_binding(
    target: ReferenceTarget,
    candidates: &[usize],
    bindings: &[Binding],
    mut apply: impl FnMut(usize, &Binding),
) {
    match target {
        ReferenceTarget::None => {}
        // Duplicate same-named `import A = ...` aliases in one container
        // resolve to a single declaration, but the emitter's duplicate
        // suppression decides which one survives, so a value use keeps every
        // sibling alias alive.
        ReferenceTarget::Binding(idx) => {
            apply(idx, &bindings[idx]);
            if let Some(container) = bindings[idx].import_equals_container {
                for &candidate in candidates {
                    if candidate != idx
                        && bindings[candidate].import_equals_container == Some(container)
                    {
                        apply(candidate, &bindings[candidate]);
                    }
                }
            }
        }
        ReferenceTarget::AllSameName | ReferenceTarget::SameNameWithUnknownSymbol => {
            let unknown_only = matches!(target, ReferenceTarget::SameNameWithUnknownSymbol);
            for &idx in candidates {
                if !unknown_only || bindings[idx].symbol.is_none() {
                    apply(idx, &bindings[idx]);
                }
            }
        }
    }
}

struct UsageScan<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    bindings: Vec<Binding>,
    /// All binding name nodes, for skipping the declarations themselves.
    binding_name_nodes: FxHashSet<NodeIndex>,
    /// Binding indices grouped by local name for the identifier pre-filter.
    by_name: FxHashMap<String, Vec<usize>>,
    /// Binding index by binder symbol for shadow-aware matching.
    by_symbol: FxHashMap<SymbolId, usize>,
    value_used: FxHashSet<NodeIndex>,
    /// `import A = X.Y;` edges: alias declaration node -> binding indices
    /// referenced by its right-hand side.
    alias_edges: Vec<(NodeIndex, usize)>,
    /// Alias declaration node -> binding index of the alias itself.
    alias_decl_to_binding: FxHashMap<NodeIndex, usize>,
    external_const_enum_bindings: Option<&'a FxHashSet<String>>,
    type_only_nodes: Option<&'a FxHashSet<NodeIndex>>,
}

/// Compute per-import-binding value-usage facts for one source file.
///
/// `binder` must be the binder for `arena`'s file (per-file binders built by
/// the CLI driver or a directly-bound `BinderState` in tests).
#[must_use]
pub fn compute_import_value_usage_facts(
    arena: &NodeArena,
    binder: &BinderState,
    inputs: ImportValueUsageInputs<'_>,
) -> ImportValueUsageFacts {
    let mut scan = UsageScan {
        arena,
        binder,
        bindings: Vec::new(),
        binding_name_nodes: FxHashSet::default(),
        by_name: FxHashMap::default(),
        by_symbol: FxHashMap::default(),
        value_used: FxHashSet::default(),
        alias_edges: Vec::new(),
        alias_decl_to_binding: FxHashMap::default(),
        external_const_enum_bindings: inputs.external_const_enum_bindings,
        type_only_nodes: inputs.type_only_nodes,
    };
    scan.collect_bindings();
    if scan.bindings.is_empty() {
        return ImportValueUsageFacts::default();
    }
    scan.scan_references();
    scan.propagate_alias_edges();

    ImportValueUsageFacts {
        known_bindings: scan.binding_name_nodes,
        value_used: scan.value_used,
    }
}

impl<'a> UsageScan<'a> {
    // =========================================================================
    // Binding collection
    // =========================================================================

    fn collect_bindings(&mut self) {
        for idx in 0..self.arena.nodes.len() {
            let node_idx = NodeIndex(idx as u32);
            let Some(node) = self.arena.get(node_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                self.collect_import_declaration_bindings(node_idx);
            } else if node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                self.collect_import_equals_binding(node_idx);
            }
        }
    }

    fn collect_import_declaration_bindings(&mut self, decl_idx: NodeIndex) {
        let Some(import) = self.arena.get_import_decl_at(decl_idx) else {
            return;
        };
        let Some(clause) = self.arena.get_import_clause_at(import.import_clause) else {
            return;
        };
        if clause.is_type_only {
            // `import type` clauses are elided unconditionally; the decision
            // sites never ask about them.
            return;
        }
        for (name_idx, name) in
            crate::transforms::emit_utils::collect_import_clause_value_binding_names(
                self.arena, clause,
            )
        {
            self.add_named_binding(name_idx, name, false, None);
        }
    }

    fn collect_import_equals_binding(&mut self, decl_idx: NodeIndex) {
        let Some(import) = self.arena.get_import_decl_at(decl_idx) else {
            return;
        };
        if import.is_type_only || import.import_clause.is_none() {
            return;
        }
        // For `import A = ...` the clause IS the alias identifier node.
        let Some(name) = self
            .arena
            .get_identifier_at(import.import_clause)
            .map(|ident| ident.escaped_text.clone())
            .filter(|name| !name.is_empty())
        else {
            return;
        };
        let exported = self
            .arena
            .has_modifier(&import.modifiers, SyntaxKind::ExportKeyword)
            || self.import_equals_is_export_declaration_clause(decl_idx);
        let container = self.arena.parent_of(decl_idx).filter(|idx| idx.is_some());
        let binding_idx = self.add_named_binding(import.import_clause, name, exported, container);
        // Some binder paths key the alias symbol on the declaration node
        // rather than the alias identifier; backfill so shadow-aware
        // matching can recognize references to this alias.
        if self.bindings[binding_idx].symbol.is_none()
            && let Some(sym) = self.binder.get_node_symbol(decl_idx)
        {
            self.bindings[binding_idx].symbol = Some(sym);
            self.by_symbol.insert(sym, binding_idx);
        }
        self.alias_decl_to_binding.insert(decl_idx, binding_idx);
    }

    /// `export import A = X` parses with the `export` keyword on an enclosing
    /// `EXPORT_DECLARATION` in some recovery shapes; treat that as exported.
    fn import_equals_is_export_declaration_clause(&self, decl_idx: NodeIndex) -> bool {
        self.arena
            .parent_of(decl_idx)
            .and_then(|p| self.arena.get(p))
            .is_some_and(|p| p.kind == syntax_kind_ext::EXPORT_DECLARATION)
    }

    fn add_named_binding(
        &mut self,
        name_idx: NodeIndex,
        name: String,
        exported_import_equals: bool,
        import_equals_container: Option<NodeIndex>,
    ) -> usize {
        let symbol = self.binder.get_node_symbol(name_idx);
        let binding_idx = self.bindings.len();
        if let Some(sym) = symbol {
            self.by_symbol.insert(sym, binding_idx);
        }
        self.binding_name_nodes.insert(name_idx);
        self.by_name.entry(name).or_default().push(binding_idx);
        self.bindings.push(Binding {
            name_node: name_idx,
            symbol,
            exported_import_equals,
            import_equals_container,
        });
        binding_idx
    }

    // =========================================================================
    // Reference scan
    // =========================================================================

    fn scan_references(&mut self) {
        let arena = self.arena;
        for idx in 0..arena.nodes.len() {
            let node_idx = NodeIndex(idx as u32);
            let Some(node) = arena.get(node_idx) else {
                continue;
            };
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(ident) = arena.get_identifier(node) else {
                continue;
            };
            let name = ident.escaped_text.as_str();
            if name.is_empty() {
                continue;
            }
            let Some(candidates) = self.by_name.get(name) else {
                continue;
            };
            // Skip the binding declarations themselves.
            if self.binding_name_nodes.contains(&node_idx) {
                continue;
            }
            match self.classify_reference(node_idx, name) {
                ReferencePosition::NotAReference | ReferencePosition::Erased => {}
                ReferencePosition::Value => {
                    let target = self.resolve_reference_target(node_idx, candidates);
                    let value_used = &mut self.value_used;
                    for_each_matching_binding(target, candidates, &self.bindings, |_, binding| {
                        value_used.insert(binding.name_node);
                    });
                }
                ReferencePosition::ImportEqualsRhs(alias_decl) => {
                    let target = self.resolve_reference_target(node_idx, candidates);
                    let alias_edges = &mut self.alias_edges;
                    for_each_matching_binding(
                        target,
                        candidates,
                        &self.bindings,
                        |binding_idx, _| {
                            alias_edges.push((alias_decl, binding_idx));
                        },
                    );
                }
            }
        }
    }

    /// Resolve a reference to the binding population it can denote, among
    /// the same-named `candidates`.
    ///
    /// Uses shadow-aware scope resolution; when the reference cannot be
    /// resolved (or a binding's own symbol is unknown), same-named bindings
    /// conservatively match so the import is preserved.
    fn resolve_reference_target(
        &self,
        ref_idx: NodeIndex,
        candidates: &[usize],
    ) -> ReferenceTarget {
        // Scope resolution, accepting only symbols that can answer a *value*
        // reference: one of our import bindings, or a genuine value-space
        // shadow. Pure type-space symbols (type parameters, interfaces, type
        // aliases) and foreign alias symbols (e.g. the alias a re-export
        // specifier declares for the same name) are resolver hits that do NOT
        // shadow value references, so the walk continues past them.
        // `node_symbols` keys *declaration* sites (e.g. `export default expr`
        // maps to the default-export symbol), so it is only a fallback for
        // references the scope walk cannot reach.
        let resolved = self
            .binder
            .resolve_identifier_with_filter(self.arena, ref_idx, &[], |sym| {
                self.by_symbol.contains_key(&sym) || self.is_value_shadow_symbol(sym)
            })
            .or_else(|| self.binder.get_node_symbol(ref_idx));
        match resolved {
            Some(sym) => {
                if let Some(&binding_idx) = self.by_symbol.get(&sym) {
                    return ReferenceTarget::Binding(binding_idx);
                }
                if !self.is_value_shadow_symbol(sym) {
                    // Resolver artifact (a foreign alias or a type-space
                    // symbol reached through the `node_symbols` fallback):
                    // shadowing is unproven, conservatively preserve.
                    return ReferenceTarget::AllSameName;
                }
                // The reference resolved to a genuine value shadow. Only
                // same-named bindings whose own symbol is unknown still
                // conservatively match.
                if candidates
                    .iter()
                    .any(|&idx| self.bindings[idx].symbol.is_none())
                {
                    ReferenceTarget::SameNameWithUnknownSymbol
                } else {
                    ReferenceTarget::None
                }
            }
            // Unresolvable reference: conservatively match every same-named
            // binding so the import is preserved.
            None => ReferenceTarget::AllSameName,
        }
    }

    /// Whether `sym` is a value-space, non-alias binding — the only kind of
    /// symbol that can shadow an import for a value reference.
    ///
    /// Namespace/module symbols are excluded: a namespace sharing an import's
    /// name *merges or collides* with it (e.g. the enclosing `namespace A.M`
    /// whose body declares `import M = ...`) rather than shadowing it, so
    /// resolving to one proves nothing about the import's liveness.
    fn is_value_shadow_symbol(&self, sym: SymbolId) -> bool {
        const SHADOW: u32 = symbol_flags::VALUE & !symbol_flags::MODULE;
        self.binder.get_symbol(sym).is_some_and(|symbol| {
            symbol.flags & SHADOW != 0 && symbol.flags & symbol_flags::ALIAS == 0
        })
    }

    // =========================================================================
    // Position classification
    // =========================================================================

    fn classify_reference(&self, ref_idx: NodeIndex, name: &str) -> ReferencePosition {
        let Some(parent_idx) = self.arena.parent_of(ref_idx) else {
            return ReferencePosition::NotAReference;
        };
        let Some(parent) = self.arena.get(parent_idx) else {
            return ReferencePosition::NotAReference;
        };

        if let Some(position) = self.classify_by_parent(ref_idx, parent_idx, parent.kind, name) {
            return position;
        }

        self.classify_by_ancestors(ref_idx)
    }

    /// Immediate-parent classification: positions where the identifier is a
    /// name being *introduced or labeled* rather than a reference, plus the
    /// import/export-specific reference shapes. The two halves cover disjoint
    /// parent kinds.
    fn classify_by_parent(
        &self,
        ref_idx: NodeIndex,
        parent_idx: NodeIndex,
        parent_kind: u16,
        name: &str,
    ) -> Option<ReferencePosition> {
        self.classify_name_position(ref_idx, parent_idx, parent_kind, name)
            .or_else(|| {
                self.classify_import_export_position(ref_idx, parent_idx, parent_kind, name)
            })
    }

    /// Declaration, member, and access-name positions: places where the
    /// identifier introduces or labels a name instead of referencing one.
    fn classify_name_position(
        &self,
        ref_idx: NodeIndex,
        parent_idx: NodeIndex,
        parent_kind: u16,
        name: &str,
    ) -> Option<ReferencePosition> {
        use ReferencePosition::NotAReference;
        match parent_kind {
            // `obj.prop` — the property name is not a scope reference.
            // (`obj[expr]` element-access arguments ARE references and fall
            // through to the default classification.)
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr_at(parent_idx)?;
                if access.name_or_argument == ref_idx {
                    return Some(NotAReference);
                }
                // The receiver of a qualified access through an external
                // const enum binding is inlined away during emit. Namespace
                // imports and import-equals register DOTTED binding paths
                // (`X.E`, `X.default`), so check `{name}.{member}` too —
                // `X.E.A` must not count the `X` receiver as a value use.
                if access.expression == ref_idx
                    && (self.is_external_const_enum_binding(name)
                        || self.qualified_access_is_const_enum(access.name_or_argument, name))
                {
                    return Some(NotAReference);
                }
                None
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr_at(parent_idx)?;
                if access.expression == ref_idx
                    && (self.is_external_const_enum_binding(name)
                        || self.qualified_access_is_const_enum(access.name_or_argument, name))
                {
                    return Some(NotAReference);
                }
                None
            }
            // `A.B` in entity-name position: only the leftmost identifier is a
            // scope reference.
            syntax_kind_ext::QUALIFIED_NAME => {
                let qualified = self.arena.get_qualified_name_at(parent_idx)?;
                if qualified.right == ref_idx {
                    return Some(NotAReference);
                }
                None
            }
            // `{ name: value }` — the (non-computed) key is not a reference.
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment_at(parent_idx)?;
                (prop.name == ref_idx).then_some(NotAReference)
            }
            // (`{ name }` shorthand property assignments fall through to the
            // wildcard: the shorthand name IS a reference in value position.)
            // Destructuring: `{ key: localName }` — `key` is not a reference
            // and `localName` is a declaration. Initializers are references.
            syntax_kind_ext::BINDING_ELEMENT => {
                let element = self.arena.get_binding_element_at(parent_idx)?;
                (element.property_name == ref_idx || element.name == ref_idx)
                    .then_some(NotAReference)
            }
            // Declaration names introduce bindings; they are not references.
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.arena.get_variable_declaration_at(parent_idx)?;
                (decl.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::PARAMETER => {
                let param = self.arena.get_parameter_at(parent_idx)?;
                (param.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::FUNCTION_DECLARATION | syntax_kind_ext::FUNCTION_EXPRESSION => {
                let func = self.arena.get_function_at(parent_idx)?;
                (func.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::CLASS_DECLARATION | syntax_kind_ext::CLASS_EXPRESSION => {
                let class = self.arena.get_class_at(parent_idx)?;
                (class.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                let interface = self.arena.get_interface_at(parent_idx)?;
                (interface.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let alias = self.arena.get_type_alias_at(parent_idx)?;
                (alias.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.arena.get_enum_at(parent_idx)?;
                (enum_decl.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                let module = self.arena.get_module_at(parent_idx)?;
                (module.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::TYPE_PARAMETER => {
                let type_param = self.arena.get_type_parameter_at(parent_idx)?;
                (type_param.name == ref_idx).then_some(NotAReference)
            }
            // Member names (non-computed) are not references.
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.arena.get_property_decl_at(parent_idx)?;
                (prop.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl_at(parent_idx)?;
                (method.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor_at(parent_idx)?;
                (accessor.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::PROPERTY_SIGNATURE | syntax_kind_ext::METHOD_SIGNATURE => {
                let signature = self.arena.get_signature_at(parent_idx)?;
                (signature.name == ref_idx).then_some(NotAReference)
            }
            syntax_kind_ext::ENUM_MEMBER => {
                let member = self.arena.get_enum_member_at(parent_idx)?;
                (member.name == ref_idx).then_some(NotAReference)
            }
            _ => None,
        }
    }

    /// Label, JSX, and import/export-construct positions, including the
    /// import-equals and export-specifier reference shapes.
    fn classify_import_export_position(
        &self,
        ref_idx: NodeIndex,
        parent_idx: NodeIndex,
        parent_kind: u16,
        name: &str,
    ) -> Option<ReferencePosition> {
        use ReferencePosition::{NotAReference, Value};
        match parent_kind {
            // Statement labels, JSX namespaced/closing-tag names, and the
            // identifiers inside import binding constructs (e.g. the `a` in
            // `import { a as b }`) are never symbol references.
            syntax_kind_ext::LABELED_STATEMENT
            | syntax_kind_ext::BREAK_STATEMENT
            | syntax_kind_ext::CONTINUE_STATEMENT
            | syntax_kind_ext::JSX_NAMESPACED_NAME
            | syntax_kind_ext::JSX_CLOSING_ELEMENT
            | syntax_kind_ext::IMPORT_CLAUSE
            | syntax_kind_ext::NAMESPACE_IMPORT
            | syntax_kind_ext::IMPORT_SPECIFIER
            | syntax_kind_ext::NAMESPACE_EXPORT => Some(NotAReference),
            // JSX attribute names are not references.
            syntax_kind_ext::JSX_ATTRIBUTE => {
                let attribute = self.arena.get_jsx_attribute_at(parent_idx)?;
                (attribute.name == ref_idx).then_some(NotAReference)
            }
            // JSX tags: lower-case identifier tags are intrinsic elements,
            // not references; everything else references the component value.
            syntax_kind_ext::JSX_OPENING_ELEMENT | syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                if name.chars().next().is_some_and(char::is_lowercase) {
                    Some(NotAReference)
                } else {
                    Some(Value)
                }
            }
            // `import A = X;` — the RHS root is a deferred reference that is
            // live only when the alias survives emit. The alias name itself
            // was filtered out as a binding declaration before this point.
            // (Qualified RHS roots — `import A = X.Y;` — reach the
            // import-equals node through the ancestor walk instead.)
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                Some(self.classify_import_equals_rhs(parent_idx))
            }
            // Export specifiers: `export { local }` / `export { local as out }`
            // reference `local` as a value unless the export is type-only.
            syntax_kind_ext::EXPORT_SPECIFIER => {
                Some(self.classify_export_specifier(ref_idx, parent_idx))
            }
            _ => None,
        }
    }

    /// References on the right-hand side of `import A = ...` are live only
    /// when the alias itself survives emit; type-only aliases are erased
    /// unconditionally.
    fn classify_import_equals_rhs(&self, decl_idx: NodeIndex) -> ReferencePosition {
        if self
            .arena
            .get_import_decl_at(decl_idx)
            .is_some_and(|import| import.is_type_only)
        {
            return ReferencePosition::Erased;
        }
        ReferencePosition::ImportEqualsRhs(decl_idx)
    }

    fn classify_export_specifier(
        &self,
        ref_idx: NodeIndex,
        spec_idx: NodeIndex,
    ) -> ReferencePosition {
        let Some(spec) = self.arena.get_specifier_at(spec_idx) else {
            return ReferencePosition::NotAReference;
        };
        if spec.is_type_only
            || self
                .type_only_nodes
                .is_some_and(|set| set.contains(&spec_idx))
        {
            return ReferencePosition::NotAReference;
        }
        // The local (referenced) side is `property_name` when present
        // (`export { local as out }`), otherwise `name` (`export { local }`).
        let local_side = if spec.property_name.is_some() {
            spec.property_name
        } else {
            spec.name
        };
        if local_side != ref_idx {
            return ReferencePosition::NotAReference;
        }
        // Re-exports from another module (`export { x } from "m"`) do not
        // reference local bindings; find the enclosing export declaration.
        let mut current = self.arena.parent_of(spec_idx);
        let mut steps = 0usize;
        while let Some(idx) = current {
            if idx.is_none() || steps > 8 {
                break;
            }
            steps += 1;
            let Some(node) = self.arena.get(idx) else {
                break;
            };
            if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                let Some(export) = self.arena.get_export_decl_at(idx) else {
                    break;
                };
                if export.is_type_only || export.module_specifier.is_some() {
                    return ReferencePosition::NotAReference;
                }
                return ReferencePosition::Value;
            }
            current = self.arena.parent_of(idx);
        }
        ReferencePosition::Value
    }

    /// Ancestor classification: erased (type/ambient) context vs runtime
    /// value context. Unknown shapes default to value context so the failure
    /// mode is preserving the import.
    fn classify_by_ancestors(&self, ref_idx: NodeIndex) -> ReferencePosition {
        let mut current = self.arena.parent_of(ref_idx);
        // Generous bound; AST depth is far smaller in practice.
        let mut steps = 0usize;
        while let Some(idx) = current {
            if idx.is_none() || steps > 4096 {
                break;
            }
            steps += 1;
            let Some(node) = self.arena.get(idx) else {
                break;
            };
            // Any type-node ancestor (TypeReference, TypeQuery, ImportType,
            // mapped/conditional/etc.) erases the reference from JS output.
            // `typeof x` in a *type* position is TYPE_QUERY (erased); the
            // runtime `typeof x` is TYPE_OF_EXPRESSION (a value ancestor).
            if node.is_type_node() {
                return ReferencePosition::Erased;
            }
            match node.kind {
                syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    return ReferencePosition::Erased;
                }
                // The left root of a qualified `import A = X.Y;` right-hand
                // side (the QUALIFIED_NAME right sides were already filtered
                // out as non-references).
                syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    return self.classify_import_equals_rhs(idx);
                }
                // `implements` clauses are type-only; `extends` on classes is
                // a runtime expression (interface `extends` is unreachable
                // here because the interface ancestor matches first).
                syntax_kind_ext::HERITAGE_CLAUSE => {
                    if let Some(heritage) = self.arena.get_heritage_clause_at(idx)
                        && heritage.token == SyntaxKind::ImplementsKeyword as u16
                    {
                        return ReferencePosition::Erased;
                    }
                }
                // Ambient declarations never produce runtime references.
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    if let Some(var_stmt) = self.arena.get_variable_at(idx)
                        && self.arena.is_declare(&var_stmt.modifiers)
                    {
                        return ReferencePosition::Erased;
                    }
                }
                syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function_at(idx)
                        && self.arena.is_declare(&func.modifiers)
                    {
                        return ReferencePosition::Erased;
                    }
                }
                syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class_at(idx)
                        && self.arena.is_declare(&class.modifiers)
                    {
                        return ReferencePosition::Erased;
                    }
                }
                syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(enum_decl) = self.arena.get_enum_at(idx)
                        && self.arena.is_declare(&enum_decl.modifiers)
                    {
                        return ReferencePosition::Erased;
                    }
                }
                syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = self.arena.get_module_at(idx)
                        && self.arena.is_declare(&module.modifiers)
                    {
                        return ReferencePosition::Erased;
                    }
                }
                _ => {}
            }
            current = self.arena.parent_of(idx);
        }
        // Unknown context — including a walk cut short by the step bound —
        // conservatively counts as a value usage (over-preserve, never elide
        // a runtime-required import).
        ReferencePosition::Value
    }

    fn is_external_const_enum_binding(&self, name: &str) -> bool {
        self.external_const_enum_bindings
            .is_some_and(|set| set.contains(name))
    }

    /// Whether `{receiver_name}.{member}` names an external const enum, where
    /// `member` comes from the access's name (or its string-literal argument
    /// for element access). Matches the dotted paths the driver registers for
    /// namespace imports and import-equals bindings.
    fn qualified_access_is_const_enum(
        &self,
        name_or_argument: NodeIndex,
        receiver_name: &str,
    ) -> bool {
        if self.external_const_enum_bindings.is_none() {
            return false;
        }
        let Some(member_node) = self.arena.get(name_or_argument) else {
            return false;
        };
        let member = if let Some(ident) = self.arena.get_identifier(member_node) {
            ident.escaped_text.clone()
        } else if let Some(lit) = self.arena.get_literal(member_node) {
            lit.text.clone()
        } else {
            return false;
        };
        self.is_external_const_enum_binding(&format!("{receiver_name}.{member}"))
    }

    // =========================================================================
    // Alias-edge fixpoint
    // =========================================================================

    /// `import A = X.Y;` keeps `X` alive only when `A` itself survives emit
    /// (value-used or exported). Aliases can chain, so iterate to fixpoint.
    fn propagate_alias_edges(&mut self) {
        if self.alias_edges.is_empty() {
            return;
        }
        let mut changed = true;
        while changed {
            changed = false;
            for &(alias_decl, target_binding) in &self.alias_edges {
                let alias_live = match self.alias_decl_to_binding.get(&alias_decl) {
                    Some(&alias_binding) => {
                        let alias = &self.bindings[alias_binding];
                        alias.exported_import_equals || self.value_used.contains(&alias.name_node)
                    }
                    // The alias declaration was not collected as a binding
                    // (type-only or malformed); conservatively treat the
                    // reference as live.
                    None => true,
                };
                if alias_live {
                    let target = self.bindings[target_binding].name_node;
                    if self.value_used.insert(target) {
                        changed = true;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/import_value_usage.rs"]
mod tests;
