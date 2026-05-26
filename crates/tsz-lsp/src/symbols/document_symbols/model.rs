//! Protocol-neutral document outline model plus LSP response conversion.

use tsz_common::position::Range;

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
    // it's an empty string to match tsserver's wire format.
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
            Self::Constructor | Self::SynthesizedConstructor => "constructor",
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
            Self::Unknown => "",
        }
    }
}

/// Protocol-neutral outline entry collected from syntax before LSP conversion.
#[derive(Debug, Clone)]
pub(super) struct DocumentSymbolEntry {
    pub name: String,
    pub detail: Option<String>,
    pub kind: SymbolKind,
    pub kind_modifiers: String,
    pub range: Range,
    pub selection_range: Range,
    pub container_name: Option<String>,
    pub children: Vec<Self>,
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

impl From<DocumentSymbolEntry> for DocumentSymbol {
    fn from(entry: DocumentSymbolEntry) -> Self {
        Self {
            name: entry.name,
            detail: entry.detail,
            kind: entry.kind,
            kind_modifiers: entry.kind_modifiers,
            range: entry.range,
            selection_range: entry.selection_range,
            container_name: entry.container_name,
            children: document_symbols_from_entries(entry.children),
        }
    }
}

pub(super) fn document_symbols_from_entries(
    entries: Vec<DocumentSymbolEntry>,
) -> Vec<DocumentSymbol> {
    entries.into_iter().map(DocumentSymbol::from).collect()
}
