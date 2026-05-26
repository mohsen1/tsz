//! Document Symbols implementation for LSP.
//!
//! Provides an outline/structure view of a TypeScript file showing all
//! functions, classes, interfaces, types, variables, etc.
//!
//! The output is designed to match tsserver's `navtree` response format:
//! - `name` corresponds to tsserver's `text`
//! - `kind` corresponds to tsserver's `kind` (`ScriptElementKind`)
//! - `kind_modifiers` corresponds to tsserver's `kindModifiers`
//! - `range` corresponds to tsserver's `spans[0]`
//! - `selection_range` corresponds to tsserver's `nameSpan`
//! - `children` corresponds to tsserver's `childItems`
//! - `container_name` provides the parent container for flat symbol lists

use std::cell::Cell;

use crate::utils::node_range;
use tsz_common::position::{Position, Range};
use tsz_parser::parser::node::Node;
use tsz_parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

mod expando;
mod imports;
mod model;
mod support;

pub use model::{DocumentSymbol, SymbolKind};
use model::{DocumentSymbolEntry, document_symbols_from_entries};
use support::*;

const MAX_DOCUMENT_SYMBOL_ENTRIES: usize = 3000;
const MAX_DOCUMENT_SYMBOL_DEPTH: usize = 64;
const MORE_DOCUMENT_SYMBOL_NAME: &str = "more...";

thread_local! {
    static DOCUMENT_SYMBOL_REMAINING: Cell<usize> = const { Cell::new(usize::MAX) };
    static DOCUMENT_SYMBOL_DEPTH: Cell<usize> = const { Cell::new(0) };
}

fn with_document_symbol_collection_limit<F>(f: F) -> Vec<DocumentSymbolEntry>
where
    F: FnOnce() -> Vec<DocumentSymbolEntry>,
{
    DOCUMENT_SYMBOL_REMAINING.with(|remaining| {
        DOCUMENT_SYMBOL_DEPTH.with(|depth| {
            let previous_remaining = remaining.replace(MAX_DOCUMENT_SYMBOL_ENTRIES);
            let previous_depth = depth.replace(0);
            let symbols = f();
            remaining.set(previous_remaining);
            depth.set(previous_depth);
            symbols
        })
    })
}

struct DocumentSymbolDepthGuard {
    active: bool,
}

impl Drop for DocumentSymbolDepthGuard {
    fn drop(&mut self) {
        if self.active {
            DOCUMENT_SYMBOL_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        }
    }
}

fn document_symbol_depth_guard(kind: u16) -> DocumentSymbolDepthGuard {
    let active = document_symbol_node_may_emit_direct(kind);
    if active {
        DOCUMENT_SYMBOL_DEPTH.with(|depth| depth.set(depth.get() + 1));
    }
    DocumentSymbolDepthGuard { active }
}

fn document_symbol_budget_precheck(kind: u16, range: Range) -> Option<Vec<DocumentSymbolEntry>> {
    let may_emit = document_symbol_node_may_emit_direct(kind);
    let exhausted = DOCUMENT_SYMBOL_REMAINING.with(|remaining| remaining.get() == 0);
    if exhausted {
        return Some(Vec::new());
    }

    if !may_emit {
        return None;
    }

    let at_depth_limit =
        DOCUMENT_SYMBOL_DEPTH.with(|depth| depth.get() >= MAX_DOCUMENT_SYMBOL_DEPTH);
    let must_emit_more = DOCUMENT_SYMBOL_REMAINING.with(|remaining| remaining.get() == 1);
    if at_depth_limit || must_emit_more {
        DOCUMENT_SYMBOL_REMAINING
            .with(|remaining| remaining.set(remaining.get().saturating_sub(1)));
        return Some(vec![more_document_symbol(range)]);
    }

    DOCUMENT_SYMBOL_REMAINING.with(|remaining| remaining.set(remaining.get().saturating_sub(1)));
    None
}

fn document_symbol_budget_account(symbols: &mut Vec<DocumentSymbolEntry>) {
    if symbols.is_empty() {
        DOCUMENT_SYMBOL_REMAINING.with(|remaining| remaining.set(remaining.get() + 1));
        return;
    }

    DOCUMENT_SYMBOL_REMAINING.with(|remaining| {
        let available = remaining.get();
        let extra_symbols = symbols.len().saturating_sub(1);
        if extra_symbols > available {
            let keep = available + 1;
            let sentinel_range = symbols[keep - 1].range;
            symbols.truncate(keep);
            symbols[keep - 1] = more_document_symbol(sentinel_range);
            remaining.set(0);
        } else {
            remaining.set(available - extra_symbols);
        }
    });
}

const fn document_symbol_node_may_emit_direct(kind: u16) -> bool {
    matches!(
        kind,
        k if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::CLASS_DECLARATION
            || k == syntax_kind_ext::CLASS_EXPRESSION
            || k == syntax_kind_ext::INTERFACE_DECLARATION
            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || k == syntax_kind_ext::VARIABLE_STATEMENT
            || k == syntax_kind_ext::ENUM_DECLARATION
            || k == syntax_kind_ext::ENUM_MEMBER
            || k == syntax_kind_ext::METHOD_DECLARATION
            || k == syntax_kind_ext::PROPERTY_DECLARATION
            || k == syntax_kind_ext::PROPERTY_SIGNATURE
            || k == syntax_kind_ext::CALL_SIGNATURE
            || k == syntax_kind_ext::CONSTRUCT_SIGNATURE
            || k == syntax_kind_ext::INDEX_SIGNATURE
            || k == syntax_kind_ext::METHOD_SIGNATURE
            || k == syntax_kind_ext::CONSTRUCTOR
            || k == syntax_kind_ext::GET_ACCESSOR
            || k == syntax_kind_ext::SET_ACCESSOR
            || k == syntax_kind_ext::MODULE_DECLARATION
            || k == syntax_kind_ext::IMPORT_DECLARATION
            || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            || k == syntax_kind_ext::EXPORT_ASSIGNMENT
            || k == syntax_kind_ext::EXPORT_DECLARATION
    )
}

define_lsp_provider!(minimal DocumentSymbolProvider, "Document symbol provider.");

impl<'a> DocumentSymbolProvider<'a> {
    /// Get all symbols in the document.
    pub fn get_document_symbols(&self, root: NodeIndex) -> Vec<DocumentSymbol> {
        let mut symbols =
            with_document_symbol_collection_limit(|| self.collect_symbols(root, None));
        cap_document_symbols(&mut symbols);
        document_symbols_from_entries(symbols)
    }

    /// Extract kind modifiers from a modifier node list.
    fn get_kind_modifiers_from_list(
        &self,
        modifiers: &Option<tsz_parser::parser::base::NodeList>,
    ) -> String {
        let Some(mod_list) = modifiers else {
            return String::new();
        };
        let mut result = String::new();
        for &mod_idx in &mod_list.nodes {
            if let Some(mod_node) = self.arena.get(mod_idx) {
                // Mirror tsc's `getNodeModifiers` output. `const`,
                // `readonly`, `async`, and `override` are not
                // ScriptElementKindModifier values — they affect the
                // declaration's kind or its signature but don't appear as
                // kindModifier strings. Including them here pollutes
                // navtree output (e.g. `const enum E` gained a spurious
                // `kindModifiers: "const"` and diverged from tsc).
                let modifier_str = match mod_node.kind {
                    k if k == SyntaxKind::ExportKeyword as u16 => Some("export"),
                    k if k == SyntaxKind::DeclareKeyword as u16 => Some("declare"),
                    k if k == SyntaxKind::AbstractKeyword as u16 => Some("abstract"),
                    k if k == SyntaxKind::StaticKeyword as u16 => Some("static"),
                    k if k == SyntaxKind::DefaultKeyword as u16 => Some("default"),
                    k if k == SyntaxKind::PublicKeyword as u16 => Some("public"),
                    k if k == SyntaxKind::PrivateKeyword as u16 => Some("private"),
                    k if k == SyntaxKind::ProtectedKeyword as u16 => Some("protected"),
                    _ => None,
                };
                if let Some(s) = modifier_str {
                    append_modifier(&mut result, s);
                }
            }
        }
        result
    }

    /// Recursively collect symbols from a node.
    fn collect_symbols(
        &self,
        node_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbolEntry> {
        let Some(node) = self.arena.get(node_idx) else {
            return Vec::new();
        };

        let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
        if let Some(symbols) = document_symbol_budget_precheck(node.kind, range) {
            return symbols;
        }

        let _depth_guard = document_symbol_depth_guard(node.kind);
        let mut symbols = match node.kind {
            // Source File: Recurse into statements
            k if k == syntax_kind_ext::SOURCE_FILE => {
                let mut symbols = Vec::new();
                if let Some(sf) = self.arena.get_source_file(node) {
                    symbols.reserve(sf.statements.nodes.len());
                    for &stmt in &sf.statements.nodes {
                        symbols.extend(self.collect_symbols(stmt, container_name));
                    }
                    // JS expando / prototype assignments: patterns like
                    // `A.prototype.x = fn`, `A.y = fn`, and
                    // `Object.defineProperty(A, 'p', …)` turn a plain
                    // function / var declaration into a class-shaped
                    // entry with the assigned names as its members.
                    // Match tsc's navigation-bar behavior for JS files.
                    self.apply_expando_assignments(&sf.statements.nodes, &mut symbols);
                    // Top-level assignment `x = { … }` where `x` is a
                    // previously-declared var — treat the RHS object
                    // literal as `x`'s children. Handles patterns like
                    // `var b; b = { foo: function() {} }`.
                    self.apply_identifier_object_assignments(&sf.statements.nodes, &mut symbols);
                    // Walk top-level expression statements for named
                    // class expressions nested inside call arguments
                    // (e.g. `console.log(class Foo {})`). tsc surfaces
                    // each named class/function expression as a
                    // top-level nav entry regardless of nesting depth.
                    self.apply_nested_named_expressions(&sf.statements.nodes, &mut symbols);
                    // CommonJS `exports.a = exports.b = exports.c = 0`
                    // → tsc surfaces a nested `a > b > c` tree rather
                    // than three siblings. Detect chained
                    // `exports.X = …` assignments and emit them nested.
                    self.apply_commonjs_exports_chain(&sf.statements.nodes, &mut symbols);
                    // Multiple `namespace A {}` / `namespace A.B {}`
                    // declarations merge into a single nested nav
                    // entry (matches tsc's `mergeChildren`).
                    merge_same_name_modules(&mut symbols);
                    // JS files can declare types via JSDoc
                    // `@typedef` tags on any top-level statement.
                    // Scan them so they surface as `type` nav entries.
                    Self::apply_jsdoc_typedefs(&sf.statements.nodes, &mut symbols);
                }
                symbols
            }

            // Function Declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    let name_node = func.name;
                    // tsc uses the literal `<function>` placeholder for
                    // name-less function declarations (parser error
                    // recovery cases like `function;`). Emit the same
                    // placeholder so the LSP wire format matches tsserver.
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<function>".to_string());

                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range = if name_node.is_some() {
                        node_range(self.arena, self.line_map, self.source_text, name_node)
                    } else {
                        self.get_range_keyword(node_idx, 8) // "function".len()
                    };

                    let modifiers = self.get_kind_modifiers_from_list(&func.modifiers);

                    // Collect nested symbols (functions/classes inside this function)
                    let mut children = self.collect_children_from_block(func.body, Some(&name));
                    // Also surface members of a returned object literal —
                    // tsc treats `function F() { return { a, b } }` as if
                    // `a` and `b` were direct children of F.
                    if children.is_empty() {
                        children = self.collect_returned_object_members(func.body, Some(&name));
                    }

                    let mut sym = DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Function,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    };

                    // If async, add to detail
                    if func.is_async {
                        sym.detail = Some("async".to_string());
                    }

                    vec![sym]
                } else {
                    vec![]
                }
            }

            // Class Declaration / Class Expression share the same
            // ClassData shape; tsc surfaces both as a `class` nav node
            // with their members as children. Anonymous class
            // expressions fall back to `<class>` as their text.
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                if let Some(class) = self.arena.get_class(node) {
                    let name_node = class.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<class>".to_string());

                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range = if name_node.is_some() {
                        node_range(self.arena, self.line_map, self.source_text, name_node)
                    } else {
                        self.get_range_keyword(node_idx, 5) // "class".len()
                    };

                    let modifiers = self.get_kind_modifiers_from_list(&class.modifiers);

                    let mut children = Vec::with_capacity(class.members.nodes.len());
                    for &member in &class.members.nodes {
                        children.extend(self.collect_symbols(member, Some(&name)));
                    }

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Class,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Interface Declaration
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = self.arena.get_interface(node) {
                    let name_node = iface.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<interface>".to_string());

                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range = if name_node.is_some() {
                        node_range(self.arena, self.line_map, self.source_text, name_node)
                    } else {
                        self.get_range_keyword(node_idx, 9) // "interface".len()
                    };

                    let modifiers = self.get_kind_modifiers_from_list(&iface.modifiers);

                    let mut children = Vec::with_capacity(iface.members.nodes.len());
                    for &member in &iface.members.nodes {
                        children.extend(self.collect_symbols(member, Some(&name)));
                    }

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Interface,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Type Alias Declaration
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = self.arena.get_type_alias(node) {
                    let name_node = alias.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<type>".to_string());

                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range = if name_node.is_some() {
                        node_range(self.arena, self.line_map, self.source_text, name_node)
                    } else {
                        self.get_range_keyword(node_idx, 4) // "type".len()
                    };

                    let modifiers = self.get_kind_modifiers_from_list(&alias.modifiers);

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        // Use Struct as a marker for type aliases.
                        // TypeParameter is reserved for generic type params like <T>.
                        kind: SymbolKind::Struct,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children: vec![],
                    }]
                } else {
                    vec![]
                }
            }

            // Variable Statement (can contain multiple declarations)
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                let mut symbols = Vec::new();
                if let Some(var) = self.arena.get_variable(node) {
                    // Get statement-level modifiers (export, declare)
                    let stmt_modifiers = self.get_kind_modifiers_from_list(&var.modifiers);

                    // VARIABLE_STATEMENT -> VARIABLE_DECLARATION_LIST -> declarations
                    for &decl_list_idx in &var.declarations.nodes {
                        if let Some(list_node) = self.arena.get(decl_list_idx) {
                            // Check if this is const/let/var based on list node flags
                            let flags32 = list_node.flags as u32;
                            let is_const = (flags32 & node_flags::CONST) != 0;
                            let is_let = (flags32 & node_flags::LET) != 0;
                            let kind = if is_const {
                                SymbolKind::Constant
                            } else {
                                SymbolKind::Variable
                            };

                            if let Some(list) = self.arena.get_variable(list_node) {
                                for &decl_idx in &list.declarations.nodes {
                                    if let Some(decl_node) = self.arena.get(decl_idx)
                                        && let Some(decl) =
                                            self.arena.get_variable_declaration(decl_node)
                                    {
                                        let var_modifiers = if is_let {
                                            if stmt_modifiers.is_empty() {
                                                "let".to_string()
                                            } else {
                                                format!("{stmt_modifiers},let")
                                            }
                                        } else {
                                            stmt_modifiers.clone()
                                        };
                                        if let Some(name) = self.get_name(decl.name) {
                                            let range = node_range(
                                                self.arena,
                                                self.line_map,
                                                self.source_text,
                                                decl_idx,
                                            );
                                            let selection_range = node_range(
                                                self.arena,
                                                self.line_map,
                                                self.source_text,
                                                decl.name,
                                            );
                                            let children = self.collect_initializer_children(
                                                decl.initializer,
                                                Some(&name),
                                            );
                                            symbols.push(DocumentSymbolEntry {
                                                name,
                                                detail: None,
                                                kind,
                                                kind_modifiers: var_modifiers,
                                                range,
                                                selection_range,
                                                container_name: container_name
                                                    .map(std::string::ToString::to_string),
                                                children,
                                            });
                                        } else if let Some(name_node) = self.arena.get(decl.name)
                                            && (name_node.kind
                                                == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                                || name_node.kind
                                                    == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                                        {
                                            // `const { a, b } = …` / `const [c, d] = …`
                                            // — surface each bound name as its own nav
                                            // entry matching tsc's `navigationBar`.
                                            self.collect_binding_pattern(
                                                decl.name,
                                                kind,
                                                &var_modifiers,
                                                container_name,
                                                &mut symbols,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                symbols
            }

            // Enum Declaration
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    let name_node = enum_decl.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<enum>".to_string());

                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, name_node);

                    let modifiers = self.get_kind_modifiers_from_list(&enum_decl.modifiers);

                    let mut children = Vec::with_capacity(enum_decl.members.nodes.len());
                    for &member in &enum_decl.members.nodes {
                        children.extend(self.collect_symbols(member, Some(&name)));
                    }

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Enum,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Enum Member — tsc only surfaces members whose name is a
            // plain identifier or string/numeric literal. Computed names
            // like `[Symbol.isRegExp]` are dropped from the navtree; emit
            // nothing instead of a `<member>` placeholder to match.
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                if let Some(member) = self.arena.get_enum_member(node) {
                    let name_node = member.name;
                    // Reject computed property names even though
                    // `get_name` can stringify them to `[…]` — tsc
                    // leaves enum members with computed keys out of the
                    // navbar entirely.
                    if let Some(name_inner) = self.arena.get(name_node)
                        && name_inner.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    {
                        return vec![];
                    }
                    let Some(name) = self.get_name(name_node) else {
                        return vec![];
                    };

                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, name_node);

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::EnumMember,
                        kind_modifiers: String::new(),
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children: vec![],
                    }]
                } else {
                    vec![]
                }
            }

            // Method Declaration (Class Member)
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    let name = self
                        .get_name(method.name)
                        .unwrap_or_else(|| "<method>".to_string());
                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, method.name);
                    let modifiers = self.get_kind_modifiers_from_list(&method.modifiers);
                    // Walk the method body like we do for functions and
                    // constructors — tsc surfaces locally-declared
                    // classes/functions/interfaces/enums/type aliases.
                    let children = self.collect_children_from_block(method.body, Some(&name));

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Method,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Property Declaration (Class Member)
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    // Class properties with computed names whose inner
                    // expression isn't a simple literal (e.g. `[1+1]`
                    // or `[expr()]`) are dropped from navtree — tsc
                    // leaves them out entirely.
                    if self.is_complex_computed_name(prop.name) {
                        return Vec::new();
                    }
                    let name = self
                        .get_name(prop.name)
                        .unwrap_or_else(|| "<property>".to_string());
                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, prop.name);
                    let modifiers = self.get_kind_modifiers_from_list(&prop.modifiers);
                    // Class property initializers behave like variable
                    // initializers for navtree purposes — `x = class {…}`
                    // surfaces inner members, `y = function() {…}` walks
                    // the body, `z = { a, b }` surfaces object members.
                    let children = self.collect_initializer_children(prop.initializer, Some(&name));

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Property,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Property Signature (Interface Member)
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(node) {
                    let name = self
                        .get_name(sig.name)
                        .unwrap_or_else(|| "<property>".to_string());
                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, sig.name);
                    let modifiers = self.get_kind_modifiers_from_list(&sig.modifiers);

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Property,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children: vec![],
                    }]
                } else {
                    vec![]
                }
            }

            // Call signature on an interface/object type: `(): any`.
            // tsc surfaces these as nameless entries with text `()` and
            // ScriptElementKind "call".
            k if k == syntax_kind_ext::CALL_SIGNATURE => {
                let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                vec![DocumentSymbolEntry {
                    name: "()".to_string(),
                    detail: None,
                    kind: SymbolKind::CallSignature,
                    kind_modifiers: String::new(),
                    range,
                    selection_range: range,
                    container_name: container_name.map(std::string::ToString::to_string),
                    children: vec![],
                }]
            }

            // Construct signature: `new(): IPoint` — text `new()`.
            k if k == syntax_kind_ext::CONSTRUCT_SIGNATURE => {
                let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                vec![DocumentSymbolEntry {
                    name: "new()".to_string(),
                    detail: None,
                    kind: SymbolKind::ConstructSignature,
                    kind_modifiers: String::new(),
                    range,
                    selection_range: range,
                    container_name: container_name.map(std::string::ToString::to_string),
                    children: vec![],
                }]
            }

            // Index signature: `[key: string]: number` — text `[]`.
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                vec![DocumentSymbolEntry {
                    name: "[]".to_string(),
                    detail: None,
                    kind: SymbolKind::IndexSignature,
                    kind_modifiers: String::new(),
                    range,
                    selection_range: range,
                    container_name: container_name.map(std::string::ToString::to_string),
                    children: vec![],
                }]
            }

            // Method Signature (Interface Member)
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(node) {
                    let name = self
                        .get_name(sig.name)
                        .unwrap_or_else(|| "<method>".to_string());
                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, sig.name);
                    let modifiers = self.get_kind_modifiers_from_list(&sig.modifiers);

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Method,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children: vec![],
                    }]
                } else {
                    vec![]
                }
            }

            // Constructor (Class Member). Parameter properties
            // (`constructor(public x: number)`) are hoisted into the
            // enclosing class as siblings of the constructor — tsc treats
            // them as class members, not as children of the constructor.
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                let mut out = Vec::new();
                if let Some(ctor) = self.arena.get_constructor(node) {
                    out.reserve(1 + ctor.parameters.nodes.len());
                    let children = self.collect_children_from_block(ctor.body, container_name);
                    let modifiers = self.get_kind_modifiers_from_list(&ctor.modifiers);
                    out.push(DocumentSymbolEntry {
                        name: "constructor".to_string(),
                        detail: None,
                        kind: SymbolKind::Constructor,
                        kind_modifiers: modifiers,
                        range: node_range(self.arena, self.line_map, self.source_text, node_idx),
                        selection_range: self.get_range_keyword(node_idx, 11), // "constructor".len()
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    });

                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.arena.get_parameter(param_node) else {
                            continue;
                        };
                        let param_mods = self.get_kind_modifiers_from_list(&param.modifiers);
                        // A parameter becomes a class property only when
                        // it carries an access modifier or `readonly`.
                        // Readonly isn't surfaced in `kindModifiers` (per
                        // tsc) but does upgrade the parameter to a
                        // property. Check both the emitted string and
                        // the raw modifier nodes for readonly.
                        let has_access = param_mods.contains("public")
                            || param_mods.contains("private")
                            || param_mods.contains("protected");
                        let has_readonly = param.modifiers.as_ref().is_some_and(|ml| {
                            ml.nodes.iter().any(|&m| {
                                self.arena
                                    .get(m)
                                    .is_some_and(|n| n.kind == SyntaxKind::ReadonlyKeyword as u16)
                            })
                        });
                        if !has_access && !has_readonly {
                            continue;
                        }
                        let Some(name) = self.get_name(param.name) else {
                            continue;
                        };
                        let range =
                            node_range(self.arena, self.line_map, self.source_text, param_idx);
                        let selection_range =
                            node_range(self.arena, self.line_map, self.source_text, param.name);
                        out.push(DocumentSymbolEntry {
                            name,
                            detail: None,
                            kind: SymbolKind::Property,
                            kind_modifiers: param_mods,
                            range,
                            selection_range,
                            container_name: container_name.map(std::string::ToString::to_string),
                            children: vec![],
                        });
                    }
                }
                out
            }

            // Class Static Block (`static { ... }`). tsc doesn't emit an
            // entry for the block itself; instead the block's top-level
            // variable declarations (and nested function/class/etc. forms
            // that `collect_symbols` already recognizes) bubble up as
            // siblings of the class's members.
            k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                let mut symbols = Vec::new();
                if let Some(block) = self.arena.get_block(node) {
                    symbols.reserve(block.statements.nodes.len());
                    for &stmt in &block.statements.nodes {
                        symbols.extend(self.collect_symbols(stmt, container_name));
                    }
                }
                symbols
            }

            // Get Accessor (Class Member)
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let name_node = accessor.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<accessor>".to_string());
                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, name_node);
                    let modifiers = self.get_kind_modifiers_from_list(&accessor.modifiers);
                    let children = self.collect_children_from_block(accessor.body, Some(&name));

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Getter,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Set Accessor (Class Member)
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let name_node = accessor.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<accessor>".to_string());
                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, name_node);
                    let modifiers = self.get_kind_modifiers_from_list(&accessor.modifiers);
                    let children = self.collect_children_from_block(accessor.body, Some(&name));

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Setter,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Module / Namespace Declaration
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    // Build the fully-qualified module name the way tsc
                    // does: `namespace A.B.C {}` is modeled as nested
                    // MODULE_DECLARATION nodes; tsc flattens them into a
                    // single "A.B.C" nav entry whose children belong to
                    // the innermost body. String-literal module names
                    // (`declare module 'foo'`) keep their surrounding
                    // quotes. `clean_module_text` strips line
                    // continuations and truncates at 150 chars.
                    let is_string_literal = self
                        .arena
                        .get(module.name)
                        .is_some_and(|n| n.kind == SyntaxKind::StringLiteral as u16);
                    let mut name_parts = Vec::new();
                    let first_part = if is_string_literal {
                        let start = module.name.0 as usize;
                        let end = self
                            .arena
                            .get(module.name)
                            .map_or(start, |n| n.end as usize);
                        // Use raw source text so the quote character is
                        // preserved exactly.
                        self.source_text
                            .get(
                                self.arena
                                    .get(module.name)
                                    .map(|n| n.pos as usize)
                                    .unwrap_or(0)..end,
                            )
                            .unwrap_or("")
                            .to_string()
                    } else {
                        self.get_name(module.name).unwrap_or_default()
                    };
                    name_parts.push(first_part);

                    let mut innermost = node_idx;
                    let mut innermost_body = module.body;
                    if !is_string_literal {
                        let mut body = module.body;
                        while body.is_some() {
                            let Some(body_node) = self.arena.get(body) else {
                                break;
                            };
                            if body_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                                break;
                            }
                            let Some(inner) = self.arena.get_module(body_node) else {
                                break;
                            };
                            if let Some(part) = self.get_name(inner.name) {
                                name_parts.push(part);
                            }
                            innermost = body;
                            innermost_body = inner.body;
                            body = inner.body;
                        }
                    }

                    let name = clean_module_text(&name_parts.join("."));
                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, module.name);

                    let modifiers = self.get_kind_modifiers_from_list(&module.modifiers);
                    let is_ambient = modifiers.split(',').any(|m| m == "declare");

                    let mut children = if innermost_body.is_some() {
                        self.collect_symbols(innermost_body, Some(&name))
                    } else {
                        vec![]
                    };
                    let _ = innermost;

                    // tsc's `getModifiers` walks ancestors and returns
                    // `declare` on every declaration that lives inside
                    // an ambient namespace/module. Propagate manually
                    // by appending `declare` to each descendant's
                    // kindModifiers (recursively).
                    if is_ambient {
                        propagate_ambient_modifier(&mut children);
                    }

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Module,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Module Block (body of a namespace)
            k if k == syntax_kind_ext::MODULE_BLOCK => {
                if let Some(block) = self.arena.get_module_block(node) {
                    let mut symbols = Vec::new();
                    if let Some(stmts) = &block.statements {
                        symbols.reserve(stmts.nodes.len());
                        for &stmt in &stmts.nodes {
                            symbols.extend(self.collect_symbols(stmt, container_name));
                        }
                    }
                    symbols
                } else {
                    vec![]
                }
            }

            // Export Declaration - recurse into the exported clause
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(node) {
                    let is_default = export.is_default_export;
                    let export_clause = export.export_clause;

                    if export_clause.is_some()
                        && let Some(clause_node) = self.arena.get(export_clause)
                    {
                        // `export import e = require(...)` is parsed as an
                        // EXPORT_DECLARATION wrapping IMPORT_EQUALS_DECLARATION.
                        // The inner alias should carry the `export` modifier.
                        if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                            || self.is_declaration(clause_node.kind)
                        {
                            // Collect the inner declaration/alias and add the
                            // `export` modifier. `append_modifier` de-duplicates
                            // when the inner declaration already reports its
                            // own `export` — e.g. when a `VARIABLE_STATEMENT`
                            // with an `export` modifier is nested under an
                            // EXPORT_DECLARATION wrapper — so the emitted
                            // kindModifiers doesn't end up as `"export,export"`.
                            //
                            // tsc does NOT append a `default` kindModifier:
                            // named default exports (`export default class C`)
                            // keep just `export`, anonymous ones
                            // (`export default class { }`) get their name
                            // replaced with `default` and still only carry
                            // `export` — no `default` modifier at either site.
                            let mut symbols = self.collect_symbols(export_clause, container_name);
                            for sym in &mut symbols {
                                if is_default && self.is_synthetic_placeholder_name(&sym.name) {
                                    sym.name = "default".to_string();
                                }
                                let mut mods = String::from("export");
                                for existing in
                                    sym.kind_modifiers.split(',').filter(|m| !m.is_empty())
                                {
                                    append_modifier(&mut mods, existing);
                                }
                                sym.kind_modifiers = mods;
                            }
                            return symbols;
                        }

                        // `export { a, b as B } from "mod"` — emit one alias
                        // entry per specifier (tsc's navtree collapses these
                        // down to their exported names).
                        if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                            return self.collect_import_export_specifiers(
                                export_clause,
                                container_name,
                                /* treat_as_export */ false,
                            );
                        }
                    }

                    // export default <expression> (non-declaration). tsc
                    // labels these with `default` as the text and only
                    // `export` as the modifier (no `default` modifier).
                    // The kind depends on the expression shape:
                    //   - function / arrow expression → `function` (with
                    //     body-walked children)
                    //   - object literal / call expression → `const`
                    //     (with property / argument members)
                    //   - identifier referencing an existing decl →
                    //     entry is dropped (tsc doesn't show `export
                    //     default identifier` as its own nav entry).
                    //   - everything else → `var`.
                    if is_default {
                        let range =
                            node_range(self.arena, self.line_map, self.source_text, node_idx);
                        let selection_range = self.get_range_keyword(node_idx, 6); // "export".len()
                        let expr_idx = export_clause;
                        let Some(expr_node) = self.arena.get(expr_idx) else {
                            return vec![];
                        };
                        let (kind, children) = match expr_node.kind {
                            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                                || k == syntax_kind_ext::ARROW_FUNCTION =>
                            {
                                let body = self
                                    .arena
                                    .get_function(expr_node)
                                    .map_or(NodeIndex::NONE, |f| f.body);
                                (
                                    SymbolKind::Function,
                                    self.collect_children_from_block(body, Some("default")),
                                )
                            }
                            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => (
                                SymbolKind::Constant,
                                self.collect_object_literal_members(expr_idx, Some("default")),
                            ),
                            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                                // `export default foo({ x: 1, y: 1 })` →
                                // tsc surfaces the argument object's
                                // members as children of the default
                                // entry.
                                let mut children = Vec::new();
                                if let Some(call) = self.arena.get_call_expr(expr_node)
                                    && let Some(args) = call.arguments.as_ref()
                                {
                                    children.reserve(args.nodes.len());
                                    for &arg_idx in &args.nodes {
                                        let Some(arg_node) = self.arena.get(arg_idx) else {
                                            continue;
                                        };
                                        if arg_node.kind
                                            == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                        {
                                            children.extend(self.collect_object_literal_members(
                                                arg_idx,
                                                Some("default"),
                                            ));
                                        }
                                    }
                                }
                                (SymbolKind::Constant, children)
                            }
                            k if k == SyntaxKind::Identifier as u16 => {
                                // `export default someName` — tsc
                                // drops this from the navbar since
                                // `someName` already has its own entry.
                                return vec![];
                            }
                            _ => (SymbolKind::Variable, Vec::new()),
                        };
                        return vec![DocumentSymbolEntry {
                            name: "default".to_string(),
                            detail: None,
                            kind,
                            kind_modifiers: "export".to_string(),
                            range,
                            selection_range,
                            container_name: container_name.map(std::string::ToString::to_string),
                            children,
                        }];
                    }
                }
                vec![]
            }

            // Import Declaration: `import x from "mod"`, `import { a, b as B } from "mod"`,
            // `import * as ns from "mod"`, `import "mod"` (no bindings).
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.collect_import_decl(node, node_idx, container_name)
            }

            // `import e = require("mod")` / `import e = ns.x`. The `export`
            // modifier (when present) becomes a kindModifier on the alias.
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.collect_import_equals(node, node_idx, container_name)
            }

            // Export Assignment (export default ...)
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                if let Some(export_assign) = self.arena.get_export_assignment(node) {
                    let name = if export_assign.is_export_equals {
                        "export=".to_string()
                    } else {
                        "default".to_string()
                    };

                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range = self.get_range_keyword(node_idx, 6); // "export".len()
                    // tsc always emits `export` as the kindModifier for
                    // `export = <expr>` regardless of whether the
                    // declaration's modifier list carries anything.
                    let mut modifiers = self.get_kind_modifiers_from_list(&export_assign.modifiers);
                    append_modifier(&mut modifiers, "export");

                    // Classify by the RHS expression shape, matching
                    // tsc's `getNodeKind` behavior for export= /
                    // export default.
                    let expr_idx = export_assign.expression;
                    let (kind, children) = self.classify_export_expression(expr_idx);

                    vec![DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind,
                        kind_modifiers: modifiers,
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children,
                    }]
                } else {
                    vec![]
                }
            }

            // Default fallback
            _ => vec![],
        };

        if document_symbol_node_may_emit_direct(node.kind) {
            document_symbol_budget_account(&mut symbols);
        }
        symbols
    }

    /// Walk a variable / property initializer and produce nav-item
    /// children for object-literal properties, class expressions, and
    /// arrow / function expressions with a block body. Mirrors tsc's
    /// behavior for entries like `const o = { a: function() {} }` and
    /// `const x = () => { function inner() {} }`.
    fn collect_initializer_children(
        &self,
        init_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbolEntry> {
        if init_idx.is_none() {
            return Vec::new();
        }
        let Some(init_node) = self.arena.get(init_idx) else {
            return Vec::new();
        };

        // `{ a: ..., b() {}, c }` — walk each property.
        if init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return self.collect_object_literal_members(init_idx, container_name);
        }

        // `class Foo {}` as an initializer — unwrap to the class's
        // members so `prop = class { x, y() }` emits x/y as direct
        // children of `prop` rather than wrapping in a class entry.
        if init_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            let wrapper = self.collect_symbols(init_idx, container_name);
            if wrapper.len() == 1 {
                return wrapper.into_iter().next().unwrap().children;
            }
            return wrapper;
        }

        // Arrow and function expressions: only surface nested
        // declarations from their block body, matching tsc's
        // "inner function causes the var to be a top-level function"
        // behavior.
        if (init_node.kind == syntax_kind_ext::ARROW_FUNCTION
            || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
            && let Some(func) = self.arena.get_function(init_node)
        {
            return self.collect_children_from_block(func.body, container_name);
        }

        Vec::new()
    }

    /// Emit a child entry for each property in an `OBJECT_LITERAL_EXPRESSION`.
    /// `PROPERTY_ASSIGNMENT` → property / nested object / method depending
    /// on the initializer; `SHORTHAND_PROPERTY_ASSIGNMENT` → property;
    /// `METHOD_DECLARATION` (`m() {}` shorthand) → method. Computed
    /// property names retain their bracket form via `get_name`.
    fn collect_object_literal_members(
        &self,
        obj_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbolEntry> {
        let Some(obj_node) = self.arena.get(obj_idx) else {
            return Vec::new();
        };
        let Some(obj) = self.arena.get_literal_expr(obj_node) else {
            return Vec::new();
        };
        let mut symbols = Vec::new();
        for &prop_idx in &obj.elements.nodes {
            let Some(prop_node) = self.arena.get(prop_idx) else {
                continue;
            };
            if prop_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let Some(prop) = self.arena.get_property_assignment(prop_node) else {
                    continue;
                };
                let Some(name) = self.get_name(prop.name) else {
                    continue;
                };
                let range = node_range(self.arena, self.line_map, self.source_text, prop_idx);
                let selection_range =
                    node_range(self.arena, self.line_map, self.source_text, prop.name);
                // Classify by initializer shape: function-like inits
                // become methods, everything else stays a property.
                let (kind, children) =
                    self.classify_property_initializer(prop.initializer, Some(&name));
                symbols.push(DocumentSymbolEntry {
                    name,
                    detail: None,
                    kind,
                    kind_modifiers: String::new(),
                    range,
                    selection_range,
                    container_name: container_name.map(std::string::ToString::to_string),
                    children,
                });
            } else if prop_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                if let Some(short) = self.arena.get_shorthand_property(prop_node)
                    && let Some(name) = self.get_name(short.name)
                {
                    let range = node_range(self.arena, self.line_map, self.source_text, prop_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, short.name);
                    symbols.push(DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Property,
                        kind_modifiers: String::new(),
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children: vec![],
                    });
                }
            } else if prop_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                // Object-literal shorthand method `m() {}` — the
                // existing METHOD_DECLARATION arm already produces the
                // right shape.
                symbols.extend(self.collect_symbols(prop_idx, container_name));
            } else if prop_node.kind == syntax_kind_ext::GET_ACCESSOR
                || prop_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                symbols.extend(self.collect_symbols(prop_idx, container_name));
            } else if prop_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
                // `...b` — tsc surfaces the spread expression's name as
                // a property entry when it's an identifier. Anything
                // more complex is skipped.
                if let Some(spread) = self.arena.get_spread(prop_node)
                    && let Some(name) = self.get_name(spread.expression)
                {
                    let range = node_range(self.arena, self.line_map, self.source_text, prop_idx);
                    let selection_range = node_range(
                        self.arena,
                        self.line_map,
                        self.source_text,
                        spread.expression,
                    );
                    symbols.push(DocumentSymbolEntry {
                        name,
                        detail: None,
                        kind: SymbolKind::Property,
                        kind_modifiers: String::new(),
                        range,
                        selection_range,
                        container_name: container_name.map(std::string::ToString::to_string),
                        children: vec![],
                    });
                }
            }
        }
        symbols
    }

    /// Classify an `export = <expr>` / `export default <expr>`
    /// right-hand side for navbar display.
    ///   - function / arrow → `function` with body-walked children
    ///   - class expression → `class` with class members
    ///   - object literal → `const` with members
    ///   - call expression → `const`, members come from an
    ///     object-literal argument if present
    ///   - anything else → `var`
    fn classify_export_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> (SymbolKind, Vec<DocumentSymbolEntry>) {
        if expr_idx.is_none() {
            return (SymbolKind::Variable, Vec::new());
        }
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return (SymbolKind::Variable, Vec::new());
        };
        match expr_node.kind {
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                let body = self
                    .arena
                    .get_function(expr_node)
                    .map_or(NodeIndex::NONE, |f| f.body);
                let children = self.collect_children_from_block(body, None);
                (SymbolKind::Function, children)
            }
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                // Walk the class's members via the CLASS_EXPRESSION
                // collect_symbols path and unwrap the single class
                // wrapper to inline its children under the export= /
                // default entry.
                let wrapper = self.collect_symbols(expr_idx, None);
                if wrapper.len() == 1 {
                    return (
                        SymbolKind::Class,
                        wrapper.into_iter().next().unwrap().children,
                    );
                }
                (SymbolKind::Variable, Vec::new())
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                let members = self.collect_object_literal_members(expr_idx, None);
                (SymbolKind::Constant, members)
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let mut members = Vec::new();
                if let Some(call) = self.arena.get_call_expr(expr_node)
                    && let Some(args) = call.arguments.as_ref()
                {
                    for &arg_idx in &args.nodes {
                        let Some(arg_node) = self.arena.get(arg_idx) else {
                            continue;
                        };
                        if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                            members.extend(self.collect_object_literal_members(arg_idx, None));
                        }
                    }
                }
                (SymbolKind::Constant, members)
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                // `export = (class Foo {})` — unwrap the parens and
                // reclassify the inner expression.
                if let Some(paren) = self.arena.get_parenthesized(expr_node) {
                    return self.classify_export_expression(paren.expression);
                }
                (SymbolKind::Variable, Vec::new())
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                // `X as Type` / `X satisfies Type` — unwrap and
                // reclassify the inner.
                if let Some(ass) = self.arena.get_binary_expr(expr_node) {
                    return self.classify_export_expression(ass.left);
                }
                (SymbolKind::Variable, Vec::new())
            }
            _ => (SymbolKind::Variable, Vec::new()),
        }
    }

    /// Classify a `PROPERTY_ASSIGNMENT`'s initializer for navbar display.
    /// Function / arrow initializers are methods (optionally with a
    /// body-walked child list); object literals become nested objects;
    /// class expressions become class entries; everything else is a
    /// plain property leaf.
    fn classify_property_initializer(
        &self,
        init_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> (SymbolKind, Vec<DocumentSymbolEntry>) {
        if init_idx.is_none() {
            return (SymbolKind::Property, Vec::new());
        }
        let Some(init_node) = self.arena.get(init_idx) else {
            return (SymbolKind::Property, Vec::new());
        };
        match init_node.kind {
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                let children = self
                    .arena
                    .get_function(init_node)
                    .map(|f| self.collect_children_from_block(f.body, container_name))
                    .unwrap_or_default();
                (SymbolKind::Method, children)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                let children = self.collect_object_literal_members(init_idx, container_name);
                (SymbolKind::Property, children)
            }
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                let children = self.collect_symbols(init_idx, container_name);
                // The CLASS_EXPRESSION arm returns a class entry; when
                // used as a property initializer, we usually want to
                // inline its members. But tsc keeps the wrapper — mirror
                // that by taking the class entry's children as the
                // property's children and treating the property as
                // a class-shaped entry.
                if children.len() == 1 {
                    let class_entry = &children[0];
                    return (SymbolKind::Class, class_entry.children.clone());
                }
                (SymbolKind::Property, children)
            }
            _ => (SymbolKind::Property, Vec::new()),
        }
    }

    /// Recursively walk an `OBJECT_BINDING_PATTERN` or
    /// `ARRAY_BINDING_PATTERN` and append a nav entry per bound name.
    /// Nested patterns (`{ x: [a, b] }`) recurse so every terminal
    /// identifier in the destructure surfaces. Uses the declaration's
    /// inherited `kind` / `kind_modifiers` so `const [a, b] = ...`
    /// gives two `const` leaves, etc. Function-shaped initializers on
    /// binding elements (`{ h: i = function j() {} }`) additionally
    /// surface the inner function name.
    fn collect_binding_pattern(
        &self,
        pattern_idx: NodeIndex,
        kind: SymbolKind,
        modifiers: &str,
        container_name: Option<&str>,
        out: &mut Vec<DocumentSymbolEntry>,
    ) {
        if pattern_idx.is_none() {
            return;
        }
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        for &elem_idx in &pattern.elements.nodes {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                continue;
            }
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };
            let name_idx = elem.name;
            if name_idx.is_none() {
                continue;
            }
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                // Nested destructure: recurse into the inner pattern.
                self.collect_binding_pattern(name_idx, kind, modifiers, container_name, out);
                continue;
            }
            let Some(name) = self.get_name(name_idx) else {
                continue;
            };
            let range = node_range(self.arena, self.line_map, self.source_text, elem_idx);
            let selection_range = node_range(self.arena, self.line_map, self.source_text, name_idx);
            out.push(DocumentSymbolEntry {
                name: name.clone(),
                detail: None,
                kind,
                kind_modifiers: modifiers.to_string(),
                range,
                selection_range,
                container_name: container_name.map(std::string::ToString::to_string),
                children: vec![],
            });
        }
    }

    /// Walk top-level statements for `@typedef` / `@callback` JSDoc tags and surface
    /// their names as `type` nav entries. Stub until JSDoc AST nodes flow through the parser.
    const fn apply_jsdoc_typedefs(_statements: &[NodeIndex], _symbols: &mut [DocumentSymbolEntry]) {
        // TODO: when the parser exposes JSDoc nodes, walk them for
        // `@typedef T` and append `DocumentSymbolEntry { name: T, kind:
        // SymbolKind::Struct }` entries. Until then this is a no-op.
    }

    /// Post-process: scan top-level expression statements for
    /// `identifier = { … }` and attach the RHS object literal's
    /// members as children of the matching var / const entry. Skips
    /// owners that already have children (from an initializer or an
    /// expando promotion).
    /// Detect CommonJS chained `exports.X = exports.Y = … = value`
    /// assignments and emit a nested nav tree (X → Y → …). tsc models
    /// these as declaration merging for the CommonJS module
    /// namespace. Handles only simple `exports.<name>` LHS forms.
    fn apply_commonjs_exports_chain(
        &self,
        statements: &[NodeIndex],
        symbols: &mut Vec<DocumentSymbolEntry>,
    ) {
        // Walk an assignment, collecting (name, stmt_idx) in order.
        // Returns None if the chain breaks (non-exports LHS or wrong
        // shape). `value_idx` is the innermost RHS for span purposes.
        fn walk(
            provider: &DocumentSymbolProvider,
            expr_idx: NodeIndex,
            out: &mut Vec<String>,
        ) -> bool {
            let Some(expr) = provider.arena.get(expr_idx) else {
                return false;
            };
            if expr.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return true; // non-assignment terminator — OK (end of chain)
            }
            let Some(bin) = provider.arena.get_binary_expr(expr) else {
                return false;
            };
            if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                return false;
            }
            // LHS must be exports.<name>
            let Some(lhs) = provider.arena.get(bin.left) else {
                return false;
            };
            if lhs.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                return false;
            }
            let Some(access) = provider.arena.get_access_expr(lhs) else {
                return false;
            };
            let Some(root) = provider.arena.get(access.expression) else {
                return false;
            };
            if root.kind != SyntaxKind::Identifier as u16 {
                return false;
            }
            if provider.get_name(access.expression).as_deref() != Some("exports") {
                return false;
            }
            let Some(name) = provider.get_name(access.name_or_argument) else {
                return false;
            };
            out.push(name);
            walk(provider, bin.right, out)
        }

        for &stmt_idx in statements {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(exp_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let mut names: Vec<String> = Vec::new();
            if !walk(self, exp_stmt.expression, &mut names) || names.is_empty() {
                continue;
            }
            // Build nested chain: names[0] is outermost, names[n-1]
            // innermost. tsc renders them all as `const`.
            let range = node_range(self.arena, self.line_map, self.source_text, stmt_idx);
            let mut inner: Option<DocumentSymbolEntry> = None;
            for name in names.iter().rev() {
                let mut children = Vec::new();
                if let Some(child) = inner.take() {
                    children.push(child);
                }
                inner = Some(DocumentSymbolEntry {
                    name: name.clone(),
                    detail: None,
                    kind: SymbolKind::Constant,
                    kind_modifiers: String::new(),
                    range,
                    selection_range: range,
                    container_name: None,
                    children,
                });
            }
            if let Some(top) = inner {
                symbols.push(top);
            }
        }
    }

    /// Walk top-level expression statements for named class / function
    /// expressions at any nesting depth (most commonly inside call
    /// arguments like `console.log(class Foo {})`). Each named class /
    /// function expression becomes a top-level nav entry matching
    /// tsc's behavior in `navigationBarAnonymousClassAndFunctionExpressions2`.
    fn apply_nested_named_expressions(
        &self,
        statements: &[NodeIndex],
        symbols: &mut Vec<DocumentSymbolEntry>,
    ) {
        fn walk(
            provider: &DocumentSymbolProvider,
            expr_idx: NodeIndex,
            out: &mut Vec<DocumentSymbolEntry>,
        ) {
            if expr_idx.is_none() {
                return;
            }
            let Some(node) = provider.arena.get(expr_idx) else {
                return;
            };
            match node.kind {
                k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                    // Only named class expressions surface; anonymous
                    // ones are skipped (expected behavior per tsc).
                    if let Some(class) = provider.arena.get_class(node)
                        && class.name.is_some()
                        && let Some(name) = provider.get_name(class.name)
                    {
                        let range = node_range(
                            provider.arena,
                            provider.line_map,
                            provider.source_text,
                            expr_idx,
                        );
                        let selection_range = node_range(
                            provider.arena,
                            provider.line_map,
                            provider.source_text,
                            class.name,
                        );
                        let mut children = Vec::new();
                        for &member in &class.members.nodes {
                            children.extend(provider.collect_symbols(member, Some(&name)));
                        }
                        out.push(DocumentSymbolEntry {
                            name,
                            detail: None,
                            kind: SymbolKind::Class,
                            kind_modifiers: String::new(),
                            range,
                            selection_range,
                            container_name: None,
                            children,
                        });
                    }
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    let Some(call) = provider.arena.get_call_expr(node) else {
                        return;
                    };
                    walk(provider, call.expression, out);
                    if let Some(args) = call.arguments.as_ref() {
                        for &arg in &args.nodes {
                            walk(provider, arg, out);
                        }
                    }
                }
                _ => {}
            }
        }
        let mut new_entries = Vec::new();
        for &stmt_idx in statements {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(exp_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            walk(self, exp_stmt.expression, &mut new_entries);
        }
        symbols.extend(new_entries);
    }

    fn apply_identifier_object_assignments(
        &self,
        statements: &[NodeIndex],
        symbols: &mut Vec<DocumentSymbolEntry>,
    ) {
        // Collect top-level assignments `x = { foo: function() {…}, … }`
        // where x is a previously-declared (empty) var. tsc surfaces
        // each function-valued property of the RHS object as a TOP-LEVEL
        // nav entry (the binding expression's `parent` is the
        // ExpressionStatement, which is a direct child of the source
        // file), not as children of `x`. Non-function-valued properties
        // are dropped.
        let mut new_entries: Vec<DocumentSymbolEntry> = Vec::new();
        for &stmt_idx in statements {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(exp_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.arena.get(exp_stmt.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(bin) = self.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            let Some(lhs) = self.arena.get(bin.left) else {
                continue;
            };
            if lhs.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(owner) = self.get_name(bin.left) else {
                continue;
            };
            let Some(rhs_node) = self.arena.get(bin.right) else {
                continue;
            };
            if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            // Only process when the owner is a previously-declared var /
            // const with no initializer-driven children yet. (If the var
            // already has children from its initializer, we'd be
            // duplicating.)
            let owner_exists = symbols.iter().any(|s| {
                s.name == owner
                    && matches!(s.kind, SymbolKind::Variable | SymbolKind::Constant)
                    && s.children.is_empty()
            });
            if !owner_exists {
                continue;
            }
            let Some(obj) = self.arena.get_literal_expr(rhs_node) else {
                continue;
            };
            for &prop_idx in &obj.elements.nodes {
                let Some(prop_node) = self.arena.get(prop_idx) else {
                    continue;
                };
                if prop_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                    continue;
                }
                let Some(prop) = self.arena.get_property_assignment(prop_node) else {
                    continue;
                };
                let Some(name) = self.get_name(prop.name) else {
                    continue;
                };
                let Some(init) = self.arena.get(prop.initializer) else {
                    continue;
                };
                if init.kind != syntax_kind_ext::FUNCTION_EXPRESSION
                    && init.kind != syntax_kind_ext::ARROW_FUNCTION
                {
                    continue;
                }
                let body = self
                    .arena
                    .get_function(init)
                    .map_or(NodeIndex::NONE, |f| f.body);
                let children = self.collect_children_from_block(body, Some(&name));
                let range = node_range(self.arena, self.line_map, self.source_text, prop_idx);
                let selection_range =
                    node_range(self.arena, self.line_map, self.source_text, prop.name);
                new_entries.push(DocumentSymbolEntry {
                    name,
                    detail: None,
                    kind: SymbolKind::Method,
                    kind_modifiers: String::new(),
                    range,
                    selection_range,
                    container_name: None,
                    children,
                });
            }
        }
        symbols.extend(new_entries);
    }

    fn collect_returned_object_members(
        &self,
        block_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbolEntry> {
        if block_idx.is_none() {
            return Vec::new();
        }
        let Some(block_node) = self.arena.get(block_idx) else {
            return Vec::new();
        };
        if block_node.kind != syntax_kind_ext::BLOCK {
            return Vec::new();
        }
        let Some(block) = self.arena.get_block(block_node) else {
            return Vec::new();
        };
        for &stmt in &block.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                continue;
            };
            let expr_idx = ret.expression;
            if expr_idx.is_none() {
                continue;
            }
            let Some(expr_node) = self.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return self.collect_object_literal_members(expr_idx, container_name);
            }
        }
        Vec::new()
    }

    /// Helper to collect children from a block (e.g. inside function).
    /// Only collects nested functions/classes for the outline.
    fn collect_children_from_block(
        &self,
        block_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbolEntry> {
        let mut symbols = Vec::new();
        if block_idx.is_none() {
            return symbols;
        }

        if let Some(node) = self.arena.get(block_idx)
            && node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(node)
        {
            for &stmt in &block.statements.nodes {
                // tsc's `addChildrenRecursively` walks every statement
                // inside a block and treats function/class/interface/
                // enum/type-alias/module declarations AND variable
                // statements as nav nodes. Surfacing vars matches tests
                // like `navigationBarItemsFunctions` which expect
                // `function baz() { var v = 10 }` → baz has child v.
                if let Some(stmt_node) = self.arena.get(stmt)
                    && matches!(
                        stmt_node.kind,
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::INTERFACE_DECLARATION
                            || k == syntax_kind_ext::ENUM_DECLARATION
                            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                            || k == syntax_kind_ext::MODULE_DECLARATION
                            || k == syntax_kind_ext::VARIABLE_STATEMENT
                    )
                {
                    symbols.extend(self.collect_symbols(stmt, container_name));
                }
            }
        }
        symbols
    }
}

#[cfg(test)]
#[path = "../../tests/document_symbols_tests.rs"]
mod document_symbols_tests;
