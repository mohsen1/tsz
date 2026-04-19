#![allow(
    clippy::match_same_arms,
    clippy::collapsible_if,
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::type_complexity
)]

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

use crate::utils::node_range;
use tsz_common::position::{Position, Range};
use tsz_parser::parser::node::Node;
use tsz_parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// A symbol kind (matches LSP `SymbolKind` values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
    // Non-LSP kinds used internally for tsserver parity (the LSP `SymbolKind`
    // enum has no getter/setter/alias distinction — clients that surface
    // these via LSP should treat Alias as a variable/module and
    // Getter/Setter as a property).
    Alias = 27,
    Getter = 28,
    Setter = 29,
    // Interface/object-type signatures — nameless declarations that tsc
    // represents with synthetic text (`()`, `new()`, `[]`) and dedicated
    // ScriptElementKind strings. Non-LSP; treat as Property downstream.
    CallSignature = 30,
    ConstructSignature = 31,
    IndexSignature = 32,
    // A function declaration that was promoted to a class through
    // expando / prototype assignments. Its nav entry is labeled
    // `constructor` but the underlying node is still a
    // FunctionDeclaration — tsc sorts it by that kind rather than
    // treating it as nameless the way a real Constructor member is.
    SynthesizedConstructor = 33,
    // Unknown kind — rendered as an empty ScriptElementKind string.
    // tsc returns `ScriptElementKind.unknown ("")` for some nav
    // entries (expando property assignments where the RHS isn't a
    // function, certain JS patterns). Keep the name field populated
    // and let the navbar/navtree serializer omit the kind field when
    // it's falsy, matching the fourslash harness JSON compare.
    Unknown = 34,
}

impl SymbolKind {
    /// Convert to tsserver's `ScriptElementKind` string.
    pub const fn to_script_element_kind(self) -> &'static str {
        match self {
            Self::File => "script",
            Self::Module | Self::Namespace | Self::Package => "module",
            Self::Class => "class",
            Self::Method => "method",
            Self::Property | Self::Field | Self::Key => "property",
            Self::Constructor => "constructor",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Function | Self::Event | Self::Operator => "function",
            Self::Variable | Self::Boolean | Self::Array | Self::Object | Self::Null => "var",
            Self::Constant | Self::String | Self::Number => "const",
            Self::EnumMember => "enum member",
            Self::TypeParameter => "type parameter",
            Self::Struct => "type",
            Self::Alias => "alias",
            Self::Getter => "getter",
            Self::Setter => "setter",
            Self::CallSignature => "call",
            Self::ConstructSignature => "construct",
            Self::IndexSignature => "index",
            Self::SynthesizedConstructor => "constructor",
            Self::Unknown => "",
        }
    }
}

/// Represents programming constructs like variables, classes, interfaces, etc.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentSymbol {
    /// The name of this symbol.
    pub name: String,
    /// More detail for this symbol, e.g. the signature of a function.
    pub detail: Option<String>,
    /// The kind of this symbol.
    pub kind: SymbolKind,
    /// Comma-separated modifier flags (e.g. "export,declare,abstract").
    /// Corresponds to tsserver's `kindModifiers`.
    pub kind_modifiers: String,
    /// The range enclosing this symbol (entire definition).
    pub range: Range,
    /// The range that should be selected and revealed when this symbol is being picked (just the identifier).
    pub selection_range: Range,
    /// The name of the containing symbol (for flat symbol lists).
    pub container_name: Option<String>,
    /// Children of this symbol, e.g. properties of a class.
    pub children: Vec<Self>,
}

impl DocumentSymbol {
    /// Create a new document symbol.
    pub const fn new(name: String, kind: SymbolKind, range: Range, selection_range: Range) -> Self {
        Self {
            name,
            detail: None,
            kind,
            kind_modifiers: String::new(),
            range,
            selection_range,
            container_name: None,
            children: Vec::new(),
        }
    }

    /// Add a child symbol.
    pub fn add_child(&mut self, child: Self) {
        self.children.push(child);
    }

    /// Set the detail field.
    pub fn with_detail(mut self, detail: String) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Set the `kind_modifiers` field.
    pub fn with_kind_modifiers(mut self, modifiers: String) -> Self {
        self.kind_modifiers = modifiers;
        self
    }

    /// Set the `container_name` field.
    pub fn with_container_name(mut self, container: String) -> Self {
        self.container_name = Some(container);
        self
    }
}

define_lsp_provider!(minimal DocumentSymbolProvider, "Document symbol provider.");

impl<'a> DocumentSymbolProvider<'a> {
    /// Get all symbols in the document.
    pub fn get_document_symbols(&self, root: NodeIndex) -> Vec<DocumentSymbol> {
        self.collect_symbols(root, None)
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
    ) -> Vec<DocumentSymbol> {
        let Some(node) = self.arena.get(node_idx) else {
            return Vec::new();
        };

        match node.kind {
            // Source File: Recurse into statements
            k if k == syntax_kind_ext::SOURCE_FILE => {
                let mut symbols = Vec::new();
                if let Some(sf) = self.arena.get_source_file(node) {
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
                    // Multiple `namespace A {}` / `namespace A.B {}`
                    // declarations merge into a single nested nav
                    // entry (matches tsc's `mergeChildren`).
                    merge_same_name_modules(&mut symbols);
                    // JS files can declare types via JSDoc
                    // `@typedef` tags on any top-level statement.
                    // Scan them so they surface as `type` nav entries.
                    self.apply_jsdoc_typedefs(&sf.statements.nodes, &mut symbols);
                }
                symbols
            }

            // Function Declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    let name_node = func.name;
                    // tsc uses the literal `<function>` placeholder for
                    // name-less function declarations (parser error
                    // recovery cases like `function;`). Keep the same
                    // placeholder so snapshot diffs stay aligned.
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

                    let mut sym = DocumentSymbol {
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

                    let mut children = Vec::new();
                    for &member in &class.members.nodes {
                        children.extend(self.collect_symbols(member, Some(&name)));
                    }

                    vec![DocumentSymbol {
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

                    let mut children = Vec::new();
                    for &member in &iface.members.nodes {
                        children.extend(self.collect_symbols(member, Some(&name)));
                    }

                    vec![DocumentSymbol {
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

                    vec![DocumentSymbol {
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
                                            symbols.push(DocumentSymbol {
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

                    let mut children = Vec::new();
                    for &member in &enum_decl.members.nodes {
                        children.extend(self.collect_symbols(member, Some(&name)));
                    }

                    vec![DocumentSymbol {
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

                    vec![DocumentSymbol {
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

                    vec![DocumentSymbol {
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

                    vec![DocumentSymbol {
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

                    vec![DocumentSymbol {
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
                vec![DocumentSymbol {
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
                vec![DocumentSymbol {
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
                vec![DocumentSymbol {
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

                    vec![DocumentSymbol {
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
                    let children = self.collect_children_from_block(ctor.body, container_name);
                    let modifiers = self.get_kind_modifiers_from_list(&ctor.modifiers);
                    out.push(DocumentSymbol {
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
                        out.push(DocumentSymbol {
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

                    vec![DocumentSymbol {
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

                    vec![DocumentSymbol {
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
                        while !body.is_none() {
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

                    vec![DocumentSymbol {
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
                        return vec![DocumentSymbol {
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

                    vec![DocumentSymbol {
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
        }
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
    ) -> Vec<DocumentSymbol> {
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
        if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
            || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
        {
            if let Some(func) = self.arena.get_function(init_node) {
                return self.collect_children_from_block(func.body, container_name);
            }
        }

        Vec::new()
    }

    /// Emit a child entry for each property in an OBJECT_LITERAL_EXPRESSION.
    /// `PROPERTY_ASSIGNMENT` → property / nested object / method depending
    /// on the initializer; `SHORTHAND_PROPERTY_ASSIGNMENT` → property;
    /// `METHOD_DECLARATION` (`m() {}` shorthand) → method. Computed
    /// property names retain their bracket form via `get_name`.
    fn collect_object_literal_members(
        &self,
        obj_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbol> {
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
                symbols.push(DocumentSymbol {
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
                    symbols.push(DocumentSymbol {
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
                    symbols.push(DocumentSymbol {
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
    fn classify_export_expression(&self, expr_idx: NodeIndex) -> (SymbolKind, Vec<DocumentSymbol>) {
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

    /// Classify a PROPERTY_ASSIGNMENT's initializer for navbar display.
    /// Function / arrow initializers are methods (optionally with a
    /// body-walked child list); object literals become nested objects;
    /// class expressions become class entries; everything else is a
    /// plain property leaf.
    fn classify_property_initializer(
        &self,
        init_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> (SymbolKind, Vec<DocumentSymbol>) {
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

    /// Recursively walk an OBJECT_BINDING_PATTERN or
    /// ARRAY_BINDING_PATTERN and append a nav entry per bound name.
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
        out: &mut Vec<DocumentSymbol>,
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
            out.push(DocumentSymbol {
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

    /// Scan a function/method body for a RETURN_STATEMENT whose
    /// expression is an OBJECT_LITERAL_EXPRESSION. When found, emit
    /// that object's members as if they were direct children of the
    /// enclosing function — this mirrors tsc's treatment of factory
    /// functions (`function F() { return { a, b } }`).
    /// Scan top-level expression statements for JS "expando" patterns
    /// (`X.prototype.Y = fn`, `X.Y = fn`) and `Object.defineProperty(X,
    /// 'Y', …)` calls. Each assignment attaches a member to the nav
    /// entry named `X`, promoting it to a class and synthesizing a
    /// `constructor` child so the navtree matches tsc's JS-mode
    /// behavior.
    /// Walk top-level statements for `@typedef` / `@callback` JSDoc
    /// tags and surface their names as `type` nav entries. For now
    /// this is a stub — JSDoc AST plumbing would need to flow through
    /// the parser to the LSP to implement properly.
    #[allow(clippy::unused_self)]
    fn apply_jsdoc_typedefs(&self, _statements: &[NodeIndex], _symbols: &mut [DocumentSymbol]) {
        // TODO: when the parser exposes JSDoc nodes, walk them for
        // `@typedef T` and append `DocumentSymbol { name: T, kind:
        // SymbolKind::Struct }` entries. Until then this is a no-op.
    }

    /// Post-process: scan top-level expression statements for
    /// `identifier = { … }` and attach the RHS object literal's
    /// members as children of the matching var / const entry. Skips
    /// owners that already have children (from an initializer or an
    /// expando promotion).
    fn apply_identifier_object_assignments(
        &self,
        statements: &[NodeIndex],
        symbols: &mut Vec<DocumentSymbol>,
    ) {
        // Collect top-level assignments `x = { foo: function() {…}, … }`
        // where x is a previously-declared (empty) var. tsc surfaces
        // each function-valued property of the RHS object as a TOP-LEVEL
        // nav entry (the binding expression's `parent` is the
        // ExpressionStatement, which is a direct child of the source
        // file), not as children of `x`. Non-function-valued properties
        // are dropped.
        let mut new_entries: Vec<DocumentSymbol> = Vec::new();
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
                new_entries.push(DocumentSymbol {
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

    fn apply_expando_assignments(&self, statements: &[NodeIndex], symbols: &mut [DocumentSymbol]) {
        // Group expando members by owner name. `(owner → Vec<(member_name,
        // prototype?, method?, fn_body?)>)`. We also track whether any
        // assignment for that owner came through `.prototype` — that
        // drives whether a synthetic constructor is injected.
        // Kind override for a prototype-object method shorthand. tsc
        // uses ScriptElementKind.method for `X.prototype = { m() {} }`
        // and ScriptElementKind.function for `X.prototype.m = function(){}`.
        #[derive(Clone, Copy, Debug)]
        enum MemberKindHint {
            None,
            Method,
        }
        #[derive(Default)]
        struct Expando {
            // (name, is_function_like, body_block_idx for children,
            // statement index for source-position sort,
            // descriptor_idx for `Object.defineProperty` cases so the
            // descriptor's `get`/`set` properties surface as children,
            // member_via_prototype — whether *this specific* member
            // was assigned through `.prototype.y` so we distinguish
            // `X.prototype.y = 0` (kind: property) from `X.y = 0`
            // (kind: "" unknown),
            // kind_hint — `Method` when the member came from an
            // object-literal shorthand method (`{ m() {} }` in
            // `X.prototype = {…}`), so it renders as `method` instead
            // of `function`.)
            members: Vec<(
                String,
                bool,
                NodeIndex,
                NodeIndex,
                NodeIndex,
                bool,
                MemberKindHint,
            )>,
            via_prototype: bool,
        }
        let mut groups: std::collections::BTreeMap<String, Expando> =
            std::collections::BTreeMap::new();

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
            let expr_idx = exp_stmt.expression;
            let Some(expr_node) = self.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let Some(bin) = self.arena.get_binary_expr(expr_node) else {
                    continue;
                };
                if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                    continue;
                }
                // Special case: `X.prototype = { a, b() {}, … }` — treat
                // each property of the RHS object literal as a prototype
                // member (same as `X.prototype.a = …` for each).
                if let Some(owner) = self.parse_prototype_assignment(bin.left) {
                    let rhs = self.arena.get(bin.right);
                    if let Some(rhs_node) = rhs
                        && rhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        && let Some(obj) = self.arena.get_literal_expr(rhs_node)
                    {
                        let entry = groups.entry(owner.clone()).or_default();
                        entry.via_prototype = true;
                        for &prop_idx in &obj.elements.nodes {
                            let Some(prop_node) = self.arena.get(prop_idx) else {
                                continue;
                            };
                            let (name_idx, init_idx, is_shorthand_method) = match prop_node.kind {
                                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                                    let Some(prop) = self.arena.get_property_assignment(prop_node)
                                    else {
                                        continue;
                                    };
                                    (prop.name, prop.initializer, false)
                                }
                                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                    let Some(m) = self.arena.get_method_decl(prop_node) else {
                                        continue;
                                    };
                                    (m.name, NodeIndex::NONE, true)
                                }
                                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                                    let Some(s) = self.arena.get_shorthand_property(prop_node)
                                    else {
                                        continue;
                                    };
                                    (s.name, NodeIndex::NONE, false)
                                }
                                _ => continue,
                            };
                            let Some(member_name) = self.get_name(name_idx) else {
                                continue;
                            };
                            let is_fn =
                                is_shorthand_method || self.is_function_like_expression(init_idx);
                            let body = if is_fn {
                                if is_shorthand_method {
                                    self.arena
                                        .get_method_decl_at(prop_idx)
                                        .map_or(NodeIndex::NONE, |m| m.body)
                                } else {
                                    self.arena
                                        .get(init_idx)
                                        .and_then(|n| self.arena.get_function(n))
                                        .map_or(NodeIndex::NONE, |f| f.body)
                                }
                            } else {
                                NodeIndex::NONE
                            };
                            let hint = if is_shorthand_method {
                                MemberKindHint::Method
                            } else {
                                MemberKindHint::None
                            };
                            entry.members.push((
                                member_name,
                                is_fn,
                                body,
                                stmt_idx,
                                NodeIndex::NONE,
                                true,
                                hint,
                            ));
                        }
                        continue;
                    }
                }
                if let Some((owner, name, via_prototype)) = self.parse_expando_lhs(bin.left) {
                    let is_fn = self.is_function_like_expression(bin.right);
                    let body = if is_fn {
                        self.arena
                            .get(bin.right)
                            .and_then(|n| self.arena.get_function(n))
                            .map_or(NodeIndex::NONE, |f| f.body)
                    } else {
                        NodeIndex::NONE
                    };
                    let entry = groups.entry(owner).or_default();
                    entry.members.push((
                        name,
                        is_fn,
                        body,
                        stmt_idx,
                        NodeIndex::NONE,
                        via_prototype,
                        MemberKindHint::None,
                    ));
                    if via_prototype {
                        entry.via_prototype = true;
                    }
                }
            } else if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION {
                // `Object.defineProperty(X, 'y', descriptor)` /
                // `Object.defineProperty(X.prototype, 'y', descriptor)` —
                // descriptor's own property members (e.g. `get`/`set`)
                // surface as the navbar entry's children. is_fn=false
                // gives it `Unknown` kind so tsc's omit-empty-kind
                // behavior kicks in.
                if let Some((owner, name, via_prototype, descriptor)) =
                    self.parse_define_property(expr_idx)
                {
                    let entry = groups.entry(owner).or_default();
                    entry.members.push((
                        name,
                        false,
                        NodeIndex::NONE,
                        stmt_idx,
                        descriptor,
                        via_prototype,
                        MemberKindHint::None,
                    ));
                    if via_prototype {
                        entry.via_prototype = true;
                    }
                }
            }
        }

        if groups.is_empty() {
            return;
        }

        for sym in symbols.iter_mut() {
            let Some(expando) = groups.get(&sym.name) else {
                continue;
            };
            // Only promote var / function entries — actual `class X {}`
            // declarations keep their own structure.
            let promote = matches!(
                sym.kind,
                SymbolKind::Function | SymbolKind::Variable | SymbolKind::Constant
            );
            if !promote {
                continue;
            }
            let was_function = matches!(sym.kind, SymbolKind::Function);
            sym.kind = SymbolKind::Class;
            // Add synthetic constructor when the underlying declaration
            // was a function (callable as `new X()`) or we've seen a
            // `.prototype.*` write against a var. Mirrors tsc's
            // promoted-class output which always shows a constructor.
            let has_ctor = sym.children.iter().any(|c| c.name == "constructor");
            if (was_function || expando.via_prototype) && !has_ctor {
                sym.children.insert(
                    0,
                    DocumentSymbol {
                        name: "constructor".to_string(),
                        detail: None,
                        // Always use SynthesizedConstructor for expando-
                        // promoted classes. The presence of this kind
                        // is the signal `sort_symbols_deep` uses to
                        // switch its sort to source-position order for
                        // this container's children (matches tsc's
                        // behavior for expando nav nodes that tryGetName
                        // can't name).
                        kind: SymbolKind::SynthesizedConstructor,
                        kind_modifiers: String::new(),
                        range: sym.range,
                        selection_range: sym.selection_range,
                        container_name: sym.container_name.clone(),
                        children: vec![],
                    },
                );
            }
            for (name, is_fn, body, stmt_idx, descriptor, member_via_proto, kind_hint) in
                &expando.members
            {
                let children = if !body.is_none() {
                    self.collect_children_from_block(*body, Some(&sym.name))
                } else if !descriptor.is_none() {
                    // defineProperty descriptor — walk its literal
                    // members so `get` / `set` show up as methods.
                    self.collect_object_literal_members(*descriptor, Some(&sym.name))
                } else {
                    Vec::new()
                };
                let kind = match kind_hint {
                    MemberKindHint::Method => SymbolKind::Method,
                    MemberKindHint::None => {
                        if *is_fn {
                            SymbolKind::Function
                        } else if !descriptor.is_none() {
                            // `Object.defineProperty(X, 'y', …)` has no
                            // inferable kind at tsc — the entry renders
                            // with no kind field.
                            SymbolKind::Unknown
                        } else if *member_via_proto {
                            // `X.prototype.y = 0` is treated as a
                            // prototype property assignment →
                            // ScriptElementKind.property.
                            SymbolKind::Property
                        } else {
                            // `X.y = 0` (static, non-function) — tsc
                            // omits the kind field entirely.
                            SymbolKind::Unknown
                        }
                    }
                };
                // Use the original statement's range so the
                // expando-child sort (by source position) orders these
                // relative to the synthesized constructor in the same
                // order they appear in source.
                let range = node_range(self.arena, self.line_map, self.source_text, *stmt_idx);
                sym.children.push(DocumentSymbol {
                    name: name.clone(),
                    detail: None,
                    kind,
                    kind_modifiers: String::new(),
                    range,
                    selection_range: range,
                    container_name: Some(sym.name.clone()),
                    children,
                });
            }
        }
    }

    /// Parse the LHS of an assignment as an expando access chain:
    ///   `X.Y` → (X, Y, false)
    ///   `X.prototype.Y` → (X, Y, true)
    ///   `X[Symbol.something]` → (X, "[Symbol.something]", false)
    /// Returns `None` if the shape isn't a simple dotted/bracketed
    /// access rooted at an identifier.
    /// Match the LHS of an assignment as `X.prototype` (or
    /// `X["prototype"]`). Returns `X`'s name on success. This is the
    /// whole-object prototype form (`X.prototype = {...}`), not the
    /// per-member form handled by `parse_expando_lhs`.
    fn parse_prototype_assignment(&self, lhs: NodeIndex) -> Option<String> {
        let node = self.arena.get(lhs)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let member = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.get_name(access.name_or_argument)?
        } else {
            let arg = self.arena.get(access.name_or_argument)?;
            if arg.kind != SyntaxKind::StringLiteral as u16 {
                return None;
            }
            self.arena.get_literal(arg)?.text.clone()
        };
        if member != "prototype" {
            return None;
        }
        let root = self.arena.get(access.expression)?;
        if root.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        self.get_name(access.expression)
    }

    fn parse_expando_lhs(&self, lhs: NodeIndex) -> Option<(String, String, bool)> {
        let node = self.arena.get(lhs)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        // The rhs (name_or_argument) can be a name (property access) or
        // an expression (element access). Stringify with get_name so
        // computed accesses like `f[Symbol.iterator]` surface a
        // `[Symbol.iterator]` text just like computed property names do.
        let member_name = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.get_name(access.name_or_argument)?
        } else {
            // Element access: for string-literal keys (`X["a"]`) keep
            // the quoted source form (tsc's navbar uses the literal
            // text with quotes). For computed accesses (e.g.
            // `f[Symbol.iterator]`) emit the `[expr]` bracket form.
            let arg = self.arena.get(access.name_or_argument)?;
            if arg.kind == SyntaxKind::StringLiteral as u16 {
                let start = arg.pos as usize;
                let end = arg.end as usize;
                if start > end || end > self.source_text.len() {
                    return None;
                }
                self.source_text[start..end].trim().to_string()
            } else {
                let start = arg.pos as usize;
                let end = arg.end as usize;
                if start > end || end > self.source_text.len() {
                    return None;
                }
                // Parser records `end` as the position after the next
                // consumed token, so for `f[Symbol.iterator]` the arg
                // slice picks up the closing `]`. Trim trailing `]`
                // before wrapping so the bracket form doesn't double up.
                let mut inner = self.source_text[start..end].trim();
                if inner.ends_with(']') {
                    inner = &inner[..inner.len() - 1];
                }
                format!("[{}]", inner.trim_end())
            }
        };

        // Inner expression: `X` (identifier), `X.prototype`, or
        // `X["prototype"]`.
        let inner = access.expression;
        let inner_node = self.arena.get(inner)?;
        if inner_node.kind == SyntaxKind::Identifier as u16 {
            let owner = self.get_name(inner)?;
            return Some((owner, member_name, false));
        }
        if inner_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || inner_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let inner_access = self.arena.get_access_expr(inner_node)?;
            // Inner member must be the string "prototype" — either the
            // identifier `prototype` or a `["prototype"]` literal.
            let proto = if inner_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.get_name(inner_access.name_or_argument)?
            } else {
                let arg = self.arena.get(inner_access.name_or_argument)?;
                if arg.kind != SyntaxKind::StringLiteral as u16 {
                    return None;
                }
                self.arena.get_literal(arg)?.text.clone()
            };
            if proto != "prototype" {
                return None;
            }
            let root = self.arena.get(inner_access.expression)?;
            if root.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let owner = self.get_name(inner_access.expression)?;
            return Some((owner, member_name, true));
        }
        None
    }

    /// Detect `Object.defineProperty(X, 'y', descriptor)` — returns
    /// `(X_name, y_name, via_prototype, descriptor_idx)`. Returns None
    /// for any non-matching call shape.
    fn parse_define_property(
        &self,
        call_idx: NodeIndex,
    ) -> Option<(String, String, bool, NodeIndex)> {
        let call_node = self.arena.get(call_idx)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(call_node)?;
        // Callee must be `Object.defineProperty`.
        let callee = self.arena.get(call.expression)?;
        if callee.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let callee_access = self.arena.get_access_expr(callee)?;
        let callee_name = self.get_name(callee_access.name_or_argument)?;
        if callee_name != "defineProperty" {
            return None;
        }
        let root = self.arena.get(callee_access.expression)?;
        if root.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let root_name = self.get_name(callee_access.expression)?;
        if root_name != "Object" {
            return None;
        }
        // Need at least two args: target, name-literal.
        let args = call.arguments.as_ref()?;
        if args.nodes.len() < 2 {
            return None;
        }
        let target_idx = args.nodes[0];
        let name_idx = args.nodes[1];
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != SyntaxKind::StringLiteral as u16 {
            return None;
        }
        let member = self.arena.get_literal(name_node)?.text.clone();
        let descriptor = args.nodes.get(2).copied().unwrap_or(NodeIndex::NONE);
        // Target: either `X` (identifier) or `X.prototype`.
        let target = self.arena.get(target_idx)?;
        if target.kind == SyntaxKind::Identifier as u16 {
            let owner = self.get_name(target_idx)?;
            return Some((owner, member, false, descriptor));
        }
        if target.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(target)?;
            let proto_name = self.get_name(access.name_or_argument)?;
            if proto_name != "prototype" {
                return None;
            }
            let root = self.arena.get(access.expression)?;
            if root.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let owner = self.get_name(access.expression)?;
            return Some((owner, member, true, descriptor));
        }
        None
    }

    /// Check whether an expression is a function-like value
    /// (`function () {}`, `function name() {}`, or `(a) => {}`).
    fn is_function_like_expression(&self, expr: NodeIndex) -> bool {
        let Some(node) = self.arena.get(expr) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
        )
    }

    fn collect_returned_object_members(
        &self,
        block_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbol> {
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
    ) -> Vec<DocumentSymbol> {
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

    /// A class-member name is "complex-computed" when it's a
    /// `[expr]` bracket form whose inner expression isn't a simple
    /// identifier or literal. tsc omits these entries from the
    /// navbar outline entirely (compare
    /// `navigationBarPropertyDeclarations`: `[1+1]` → not surfaced).
    fn is_complex_computed_name(&self, name_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(name_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(comp) = self.arena.get_computed_property(node) else {
            return false;
        };
        let expr_idx = comp.expression;
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        // Simple identifier / literal cases are not complex.
        matches!(
            expr_node.kind,
            k if !(k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::PrivateIdentifier as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION)
        )
    }

    /// The declaration arms use `<class>`, `<function>`, etc. as a stable
    /// placeholder when a declaration has no identifier. When such a
    /// placeholder bubbles up through a default export, tsc replaces it
    /// with the literal `default` as the nav item's text — these are the
    /// forms we'd substitute.
    fn is_synthetic_placeholder_name(&self, name: &str) -> bool {
        matches!(
            name,
            "<class>" | "<function>" | "<anonymous>" | "<interface>" | "<type>" | "<enum>"
        )
    }

    /// Check if a node kind is a declaration. Used in the
    /// EXPORT_DECLARATION arm to decide whether to recurse into the
    /// exported clause (declarations) vs. treat it as a re-export
    /// (NAMED_EXPORTS etc).
    const fn is_declaration(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
            // `export namespace X {}` inside an ambient module wraps
            // the MODULE_DECLARATION in an EXPORT_DECLARATION. Without
            // this, the module body drops on the floor.
            || kind == syntax_kind_ext::MODULE_DECLARATION
    }

    /// Build an `alias` entry for a single import/export binding. The `name`
    /// is the local identifier the binding introduces into scope (e.g. `B`
    /// for `import { x as B }` or `export { a as B }`). `decl_idx` is the
    /// enclosing statement used for the range span — tsc anchors specifier
    /// spans to the whole statement, not the specifier token.
    fn alias_symbol(
        &self,
        name: String,
        name_node: NodeIndex,
        decl_idx: NodeIndex,
        container_name: Option<&str>,
        modifiers: String,
    ) -> DocumentSymbol {
        let range = node_range(self.arena, self.line_map, self.source_text, decl_idx);
        let selection_range = if name_node.is_some() {
            node_range(self.arena, self.line_map, self.source_text, name_node)
        } else {
            self.get_range_keyword(decl_idx, 6)
        };
        DocumentSymbol {
            name,
            detail: None,
            kind: SymbolKind::Alias,
            kind_modifiers: modifiers,
            range,
            selection_range,
            container_name: container_name.map(std::string::ToString::to_string),
            children: vec![],
        }
    }

    /// Collect specifiers from a `NAMED_EXPORTS` / `NAMED_IMPORTS` clause.
    /// Each specifier's local name becomes an alias. When `treat_as_export`
    /// is true, the `export` modifier is applied (used for
    /// `export { a } from "x"` so we can attach modifiers at the
    /// declaration site; currently tsc doesn't emit `export` on these so we
    /// always pass false).
    fn collect_import_export_specifiers(
        &self,
        clause_idx: NodeIndex,
        container_name: Option<&str>,
        treat_as_export: bool,
    ) -> Vec<DocumentSymbol> {
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return Vec::new();
        };
        let Some(named) = self.arena.get_named_imports(clause_node) else {
            return Vec::new();
        };
        let mut symbols = Vec::new();
        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                continue;
            };
            let name = self
                .get_name(spec.name)
                .unwrap_or_else(|| "<unknown>".to_string());
            let mods = if treat_as_export {
                String::from("export")
            } else {
                String::new()
            };
            symbols.push(self.alias_symbol(name, spec.name, spec_idx, container_name, mods));
        }
        symbols
    }

    /// Collect aliases from an `import ...` declaration.
    fn collect_import_decl(
        &self,
        node: &Node,
        node_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbol> {
        let Some(import) = self.arena.get_import_decl(node) else {
            return Vec::new();
        };
        let clause_idx = import.import_clause;
        if clause_idx.is_none() {
            return Vec::new();
        }
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return Vec::new();
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return Vec::new();
        };
        let mut symbols = Vec::new();

        // `import foo from "..."` — default import.
        if clause.name.is_some()
            && let Some(name) = self.get_name(clause.name)
        {
            symbols.push(self.alias_symbol(
                name,
                clause.name,
                node_idx,
                container_name,
                String::new(),
            ));
        }

        // Named bindings: either `NAMESPACE_IMPORT` (for `* as ns`) or
        // `NAMED_IMPORTS` (for `{ a, b as B }`).
        let named_idx = clause.named_bindings;
        if !named_idx.is_none()
            && let Some(named_node) = self.arena.get(named_idx)
        {
            if named_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                if let Some(named) = self.arena.get_named_imports(named_node) {
                    let name = if named.name.is_some() {
                        self.get_name(named.name)
                            .unwrap_or_else(|| "<unknown>".to_string())
                    } else {
                        "<unknown>".to_string()
                    };
                    symbols.push(self.alias_symbol(
                        name,
                        named.name,
                        node_idx,
                        container_name,
                        String::new(),
                    ));
                }
            } else if named_node.kind == syntax_kind_ext::NAMED_IMPORTS {
                symbols.extend(self.collect_import_export_specifiers(
                    named_idx,
                    container_name,
                    false,
                ));
            }
        }

        symbols
    }

    /// Collect an alias from an `import e = require("...")` / `import e = x.y`
    /// declaration. When the statement has an `export` modifier, it is
    /// surfaced as a `kindModifier` on the alias.
    fn collect_import_equals(
        &self,
        node: &Node,
        node_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbol> {
        let Some(import) = self.arena.get_import_decl(node) else {
            return Vec::new();
        };
        // For IMPORT_EQUALS_DECLARATION, `import_clause` is the identifier
        // on the LHS of the `=`.
        let name_idx = import.import_clause;
        let Some(name) = self.get_name(name_idx) else {
            return Vec::new();
        };
        let modifiers = self.get_kind_modifiers_from_list(&import.modifiers);
        vec![self.alias_symbol(name, name_idx, node_idx, container_name, modifiers)]
    }

    /// Get range for a keyword (when no identifier exists, e.g. "constructor").
    fn get_range_keyword(&self, node_idx: NodeIndex, len: u32) -> Range {
        if let Some(node) = self.arena.get(node_idx) {
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self
                .line_map
                .offset_to_position(node.pos + len, self.source_text);
            Range::new(start, end)
        } else {
            Range::new(Position::new(0, 0), Position::new(0, 0))
        }
    }

    /// Extract text from identifier node.
    fn get_name(&self, node_idx: NodeIndex) -> Option<String> {
        if node_idx.is_none() {
            return None;
        }
        if let Some(node) = self.arena.get(node_idx) {
            if node.kind == SyntaxKind::Identifier as u16 {
                return self.arena.get_identifier(node).and_then(|id| {
                    // An empty identifier is typically produced by
                    // parser error recovery (e.g. `function;` gives a
                    // name-less FUNCTION_DECLARATION). Treat as missing
                    // so callers fall back to `<function>` / `<class>`.
                    if id.escaped_text.is_empty() {
                        None
                    } else {
                        Some(id.escaped_text.clone())
                    }
                });
            } else if node.kind == SyntaxKind::PrivateIdentifier as u16 {
                // Private identifiers keep their `#` prefix in navbar
                // output (`#foo`). The scanner's token value may or may
                // not already include the `#` — normalize by prepending
                // when missing.
                return self.arena.get_identifier(node).map(|id| {
                    if id.escaped_text.starts_with('#') {
                        id.escaped_text.clone()
                    } else {
                        format!("#{}", id.escaped_text)
                    }
                });
            } else if node.kind == SyntaxKind::StringLiteral as u16 {
                // tsc's `nodeText(name)` returns the literal's source
                // form — keep the surrounding quotes so `"prop": 1` in
                // an object literal becomes navbar text `"prop"` (and
                // `declare module 'x'` stays `'x'` with single quotes).
                let start = node.pos as usize;
                let end = node.end as usize;
                if start <= end && end <= self.source_text.len() {
                    return Some(self.source_text[start..end].trim().to_string());
                }
                return self.arena.get_literal(node).map(|l| l.text.clone());
            } else if node.kind == SyntaxKind::NumericLiteral as u16 {
                return self.arena.get_literal(node).map(|l| l.text.clone());
            } else if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                // `["bar"]` / `[key]` on a class/interface/object member.
                // tsc uses the source-text form verbatim (including the
                // surrounding brackets) as the nav item's `text`. The
                // parser records `end` as the position after the next
                // token (so `["bar"]:` or `["bar"] ` creeps in). Cut at
                // the last `]` to keep just the bracket form.
                let start = node.pos as usize;
                let end = node.end as usize;
                if start <= end && end <= self.source_text.len() {
                    let slice = &self.source_text[start..end];
                    if let Some(close) = slice.rfind(']') {
                        return Some(slice[..=close].to_string());
                    }
                    return Some(slice.to_string());
                }
            }
        }
        None
    }
}

/// Mirror tsc's `cleanText`: truncate to 150 characters (appending
/// `...`) and strip ECMAScript line terminators, including the
/// trailing backslash from multiline string literal continuations.
/// Used exclusively for module names — tsc applies this to every
/// navbar/navtree text, but for our purposes identifier text doesn't
/// ever contain line terminators so applying it narrowly is enough.
fn clean_module_text(text: &str) -> String {
    const MAX_LEN: usize = 150;
    let truncated = if text.chars().count() > MAX_LEN {
        let head: String = text.chars().take(MAX_LEN).collect();
        format!("{head}...")
    } else {
        text.to_string()
    };
    let mut out = String::with_capacity(truncated.len());
    let mut chars = truncated.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Backslash before a newline (line continuation inside a
            // multi-line string) — drop both.
            '\\' if matches!(chars.peek(), Some('\r' | '\n' | '\u{2028}' | '\u{2029}')) => {
                // consume the paired line terminator (handling \r\n too)
                if let Some('\r') = chars.next()
                    && matches!(chars.peek(), Some('\n'))
                {
                    chars.next();
                }
            }
            // Bare line terminators are removed.
            '\r' => {
                if matches!(chars.peek(), Some('\n')) {
                    chars.next();
                }
            }
            '\n' | '\u{2028}' | '\u{2029}' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Append `declare` to every descendant's `kindModifiers` (skipping
/// duplicates). tsc implicitly applies `declare` to every declaration
/// nested inside an ambient namespace/module, so the nav output
/// reads `export,declare` for `declare namespace Windows { export
/// var A }` instead of just `export`.
fn propagate_ambient_modifier(symbols: &mut [DocumentSymbol]) {
    for sym in symbols.iter_mut() {
        let mut buf = sym.kind_modifiers.clone();
        append_modifier(&mut buf, "declare");
        sym.kind_modifiers = buf;
        propagate_ambient_modifier(&mut sym.children);
    }
}

/// Merge sibling Module/Namespace entries that share a name — tsc's
/// `mergeChildren` behavior for TypeScript's declaration merging
/// (`namespace A {} / namespace A {}` shows one entry whose children
/// combine both declarations). Recurses so nested namespaces
/// (`namespace A.B {} / namespace A {}`) also merge at every level.
fn merge_same_name_modules(symbols: &mut Vec<DocumentSymbol>) {
    // First recurse into each symbol's children so we merge deepest
    // first (avoids seeing pre-merged children on subsequent passes).
    for sym in symbols.iter_mut() {
        merge_same_name_modules(&mut sym.children);
    }
    // Now merge at this level. Walk children left to right; for each
    // Module entry, look ahead for another Module with the same name
    // and fold its children in, dropping the duplicate.
    let mut i = 0;
    while i < symbols.len() {
        let mergeable = is_mergeable_kind(symbols[i].kind);
        if mergeable.is_none() {
            i += 1;
            continue;
        }
        let target_group = mergeable.unwrap();
        let name = symbols[i].name.clone();
        let mut j = i + 1;
        while j < symbols.len() {
            let same =
                is_mergeable_kind(symbols[j].kind) == Some(target_group) && symbols[j].name == name;
            if same {
                let other = symbols.remove(j);
                symbols[i].children.extend(other.children);
            } else {
                j += 1;
            }
        }
        // After merging this slot's siblings, its children may now
        // contain duplicates from the folded-in entries (e.g.
        // `namespace A { interface I {} } + namespace A { interface I {} }`
        // → merged A has two `I` children). Recurse once more to resolve.
        merge_same_name_modules(&mut symbols[i].children);
        i += 1;
    }
}

/// Group mergeable nav kinds — tsc's declaration-merge rules allow
/// same-name modules/namespaces, interfaces, and enums to fold their
/// children together, but not mixed-kind siblings. Returns Some for
/// mergeable groups and None otherwise (functions, variables, classes,
/// etc., which never merge).
const fn is_mergeable_kind(kind: SymbolKind) -> Option<u8> {
    match kind {
        SymbolKind::Module | SymbolKind::Namespace | SymbolKind::Package => Some(1),
        SymbolKind::Interface => Some(2),
        SymbolKind::Enum => Some(3),
        _ => None,
    }
}

/// Helper to append a modifier to a comma-separated string.
fn append_modifier(result: &mut String, modifier: &str) {
    // tsc never emits the same modifier twice on a single
    // kindModifiers entry. Skip duplicates so concatenation across
    // nested AST shapes (e.g. `export var x`) stays stable.
    if result.split(',').any(|existing| existing == modifier) {
        return;
    }
    if !result.is_empty() {
        result.push(',');
    }
    result.push_str(modifier);
}

#[cfg(test)]
#[path = "../../tests/document_symbols_tests.rs"]
mod document_symbols_tests;
