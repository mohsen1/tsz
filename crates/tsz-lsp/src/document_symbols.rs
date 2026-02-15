//! Document Symbols implementation for LSP.
//!
//! Provides an outline/structure view of a TypeScript file showing all
//! functions, classes, interfaces, types, variables, etc.
//!
//! The output is designed to match tsserver's `navtree` response format:
//! - `name` corresponds to tsserver's `text`
//! - `kind` corresponds to tsserver's `kind` (ScriptElementKind)
//! - `kind_modifiers` corresponds to tsserver's `kindModifiers`
//! - `range` corresponds to tsserver's `spans[0]`
//! - `selection_range` corresponds to tsserver's `nameSpan`
//! - `children` corresponds to tsserver's `childItems`
//! - `container_name` provides the parent container for flat symbol lists

use tsz_common::position::{Position, Range};
use tsz_parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// A symbol kind (matches LSP SymbolKind values).
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
}

impl SymbolKind {
    /// Convert to tsserver's ScriptElementKind string.
    pub fn to_script_element_kind(self) -> &'static str {
        match self {
            Self::File => "script",
            Self::Module | Self::Namespace => "module",
            Self::Class => "class",
            Self::Method => "method",
            Self::Property | Self::Field => "property",
            Self::Constructor => "constructor",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Function => "function",
            Self::Variable => "var",
            Self::Constant | Self::String | Self::Number => "const",
            Self::EnumMember => "enum member",
            Self::TypeParameter => "type parameter",
            Self::Boolean => "var",
            Self::Array => "var",
            Self::Object => "var",
            Self::Key => "property",
            Self::Null => "var",
            Self::Struct => "type",
            Self::Event => "function",
            Self::Operator => "function",
            Self::Package => "module",
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
    pub fn new(name: String, kind: SymbolKind, range: Range, selection_range: Range) -> Self {
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

    /// Set the kind_modifiers field.
    pub fn with_kind_modifiers(mut self, modifiers: String) -> Self {
        self.kind_modifiers = modifiers;
        self
    }

    /// Set the container_name field.
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
                let modifier_str = match mod_node.kind {
                    k if k == SyntaxKind::ExportKeyword as u16 => Some("export"),
                    k if k == SyntaxKind::DeclareKeyword as u16 => Some("declare"),
                    k if k == SyntaxKind::AbstractKeyword as u16 => Some("abstract"),
                    k if k == SyntaxKind::StaticKeyword as u16 => Some("static"),
                    k if k == SyntaxKind::AsyncKeyword as u16 => Some("async"),
                    k if k == SyntaxKind::DefaultKeyword as u16 => Some("default"),
                    k if k == SyntaxKind::ConstKeyword as u16 => Some("const"),
                    k if k == SyntaxKind::ReadonlyKeyword as u16 => Some("readonly"),
                    k if k == SyntaxKind::PublicKeyword as u16 => Some("public"),
                    k if k == SyntaxKind::PrivateKeyword as u16 => Some("private"),
                    k if k == SyntaxKind::ProtectedKeyword as u16 => Some("protected"),
                    k if k == SyntaxKind::OverrideKeyword as u16 => Some("override"),
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

                    let range = self.get_range(node_idx);
                    let selection_range = if !name_node.is_none() {
                        self.get_range(name_node)
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

                    let range = self.get_range(node_idx);
                    let selection_range = if !name_node.is_none() {
                        self.get_range(name_node)
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

                    let range = self.get_range(node_idx);
                    let selection_range = if !name_node.is_none() {
                        self.get_range(name_node)
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

                    let range = self.get_range(node_idx);
                    let selection_range = if !name_node.is_none() {
                        self.get_range(name_node)
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
                            let is_const = (list_node.flags as u32 & node_flags::CONST) != 0;
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
                                        let range = self.get_range(decl_idx);
                                        let selection_range = self.get_range(decl.name);

                                        symbols.push(DocumentSymbol {
                                            name,
                                            detail: None,
                                            kind,
                                            kind_modifiers: stmt_modifiers.clone(),
                                            range,
                                            selection_range,
                                            container_name: container_name
                                                .map(std::string::ToString::to_string),
                                            children: vec![],
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

                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(name_node);

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

            // Enum Member
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                if let Some(member) = self.arena.get_enum_member(node) {
                    let name_node = member.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<member>".to_string());

                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(name_node);

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
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(method.name);
                    let modifiers = self.get_kind_modifiers_from_list(&method.modifiers);

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

            // Property Declaration (Class Member)
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    let name = self
                        .get_name(prop.name)
                        .unwrap_or_else(|| "<property>".to_string());
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(prop.name);
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
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(sig.name);
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

            // Method Signature (Interface Member)
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(node) {
                    let name = self
                        .get_name(sig.name)
                        .unwrap_or_else(|| "<method>".to_string());
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(sig.name);
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

            // Constructor (Class Member)
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                let (children, modifiers) = if let Some(ctor) = self.arena.get_constructor(node) {
                    let c = self.collect_children_from_block(ctor.body, container_name);
                    let m = self.get_kind_modifiers_from_list(&ctor.modifiers);
                    (c, m)
                } else {
                    (vec![], String::new())
                };

                vec![DocumentSymbol {
                    name: "constructor".to_string(),
                    detail: None,
                    kind: SymbolKind::Constructor,
                    kind_modifiers: modifiers,
                    range: self.get_range(node_idx),
                    selection_range: self.get_range_keyword(node_idx, 11), // "constructor".len()
                    container_name: container_name.map(std::string::ToString::to_string),
                    children,
                }]
            }

            // Get Accessor (Class Member)
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let name_node = accessor.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<accessor>".to_string());
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(name_node);
                    let mut modifiers = self.get_kind_modifiers_from_list(&accessor.modifiers);
                    append_modifier(&mut modifiers, "getter");

                    vec![DocumentSymbol {
                        name,
                        detail: Some("getter".to_string()),
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

            // Set Accessor (Class Member)
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let name_node = accessor.name;
                    let name = self
                        .get_name(name_node)
                        .unwrap_or_else(|| "<accessor>".to_string());
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(name_node);
                    let mut modifiers = self.get_kind_modifiers_from_list(&accessor.modifiers);
                    append_modifier(&mut modifiers, "setter");

                    vec![DocumentSymbol {
                        name,
                        detail: Some("setter".to_string()),
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

            // Module / Namespace Declaration
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    let name = self
                        .get_name(module.name)
                        .unwrap_or_else(|| "<module>".to_string());
                    let range = self.get_range(node_idx);
                    let selection_range = self.get_range(module.name);

                    let modifiers = self.get_kind_modifiers_from_list(&module.modifiers);

                    let children = if !module.body.is_none() {
                        self.collect_symbols(module.body, Some(&name))
                    } else {
                        vec![]
                    };

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

                    if !export_clause.is_none()
                        && let Some(clause_node) = self.arena.get(export_clause)
                        && self.is_declaration(clause_node.kind)
                    {
                        // Collect the inner declaration and add "export" modifier
                        let mut symbols = self.collect_symbols(export_clause, container_name);
                        for sym in &mut symbols {
                            let mut mods = String::from("export");
                            if is_default {
                                append_modifier(&mut mods, "default");
                            }
                            if !sym.kind_modifiers.is_empty() {
                                mods.push(',');
                                mods.push_str(&sym.kind_modifiers);
                            }
                            sym.kind_modifiers = mods;
                        }
                        return symbols;
                    }

                    // export default <expression> (non-declaration)
                    if is_default {
                        let range = self.get_range(node_idx);
                        let selection_range = self.get_range_keyword(node_idx, 6); // "export".len()
                        return vec![DocumentSymbol {
                            name: "default".to_string(),
                            detail: None,
                            kind: SymbolKind::Variable,
                            kind_modifiers: "export,default".to_string(),
                            range,
                            selection_range,
                            container_name: container_name.map(std::string::ToString::to_string),
                            children: vec![],
                        }];
                    }
                }
                vec![]
            }

            // Export Assignment (export default ...)
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                if let Some(export_assign) = self.arena.get_export_assignment(node) {
                    let name = if export_assign.is_export_equals {
                        "export=".to_string()
                    } else {
                        "default".to_string()
                    };

                    let range = self.get_range(node_idx);
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
                // Only collect declarations (functions, classes) - not variables
                if let Some(stmt_node) = self.arena.get(stmt)
                    && (stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION)
                {
                    symbols.extend(self.collect_symbols(stmt, container_name));
                }
            }
        }
        symbols
    }

    /// Check if a node kind is a declaration.
    fn is_declaration(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
    }

    /// Convert node range to LSP Range.
    fn get_range(&self, node_idx: NodeIndex) -> Range {
        if let Some(node) = self.arena.get(node_idx) {
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self.line_map.offset_to_position(node.end, self.source_text);
            Range::new(start, end)
        } else {
            Range::new(Position::new(0, 0), Position::new(0, 0))
        }
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
            } else if node.kind == SyntaxKind::StringLiteral as u16
                || node.kind == SyntaxKind::NumericLiteral as u16
            {
                return self.arena.get_literal(node).map(|l| l.text.clone());
            }
        }
        None
    }
}

/// Helper to append a modifier to a comma-separated string.
fn append_modifier(result: &mut String, modifier: &str) {
    if !result.is_empty() {
        result.push(',');
    }
    result.push_str(modifier);
}

#[cfg(test)]
#[path = "../tests/document_symbols_tests.rs"]
mod document_symbols_tests;
