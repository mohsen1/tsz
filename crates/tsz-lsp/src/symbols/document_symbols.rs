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
                }
                symbols
            }

            // Function Declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    let name_node = func.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<anonymous>".to_string());

                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range = if name_node.is_some() {
                        node_range(self.arena, self.line_map, self.source_text, name_node)
                    } else {
                        self.get_range_keyword(node_idx, 8) // "function".len()
                    };

                    let modifiers = self.get_kind_modifiers_from_list(&func.modifiers);

                    // Collect nested symbols (functions/classes inside this function)
                    let children = self.collect_children_from_block(func.body, Some(&name));

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

            // Class Declaration
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
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
                                        && let Some(name) = self.get_name(decl.name)
                                    {
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

                                        let var_modifiers = if is_let {
                                            if stmt_modifiers.is_empty() {
                                                "let".to_string()
                                            } else {
                                                format!("{stmt_modifiers},let")
                                            }
                                        } else {
                                            stmt_modifiers.clone()
                                        };
                                        // Walk the initializer expression:
                                        // object literals / class expressions
                                        // / arrow bodies surface their members
                                        // as children of the variable entry.
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
                    let name = self
                        .get_name(prop.name)
                        .unwrap_or_else(|| "<property>".to_string());
                    let range = node_range(self.arena, self.line_map, self.source_text, node_idx);
                    let selection_range =
                        node_range(self.arena, self.line_map, self.source_text, prop.name);
                    let modifiers = self.get_kind_modifiers_from_list(&prop.modifiers);

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

                    let children = if innermost_body.is_some() {
                        self.collect_symbols(innermost_body, Some(&name))
                    } else {
                        vec![]
                    };
                    let _ = innermost;

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
                    if is_default {
                        let range =
                            node_range(self.arena, self.line_map, self.source_text, node_idx);
                        let selection_range = self.get_range_keyword(node_idx, 6); // "export".len()
                        return vec![DocumentSymbol {
                            name: "default".to_string(),
                            detail: None,
                            kind: SymbolKind::Variable,
                            kind_modifiers: "export".to_string(),
                            range,
                            selection_range,
                            container_name: container_name.map(std::string::ToString::to_string),
                            children: vec![],
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
                    let modifiers = self.get_kind_modifiers_from_list(&export_assign.modifiers);

                    vec![DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::Variable,
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

        // `class Foo {}` as an initializer — delegate to the class arm so
        // modifiers / members render the same.
        if init_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            return self.collect_symbols(init_idx, container_name);
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
                // Collect declaration-ish statements that tsc includes in
                // the outline for a block body: functions, classes,
                // interfaces, enums, and type aliases. Variable
                // declarations inside function/constructor/method bodies
                // are deliberately omitted — tsc doesn't surface them.
                if let Some(stmt_node) = self.arena.get(stmt)
                    && matches!(
                        stmt_node.kind,
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::INTERFACE_DECLARATION
                            || k == syntax_kind_ext::ENUM_DECLARATION
                            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                            || k == syntax_kind_ext::MODULE_DECLARATION
                    )
                {
                    symbols.extend(self.collect_symbols(stmt, container_name));
                }
            }
        }
        symbols
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

    /// Check if a node kind is a declaration.
    const fn is_declaration(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
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
                return self
                    .arena
                    .get_identifier(node)
                    .map(|id| id.escaped_text.clone());
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
            } else if node.kind == SyntaxKind::StringLiteral as u16
                || node.kind == SyntaxKind::NumericLiteral as u16
            {
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
