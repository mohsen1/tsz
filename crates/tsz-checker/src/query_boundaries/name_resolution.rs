//! Name resolution boundary types and consolidated lookup API.
//!
//! This module provides a single checker-facing boundary for name resolution
//! outcomes, distinguishing value vs type vs namespace lookups and classifying
//! failures into actionable categories. Suggestion policy (spelling, globals,
//! lib changes) is boundary-owned rather than scattered across checker call sites.

use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

// ---------------------------------------------------------------------------
// NameLookupKind: what meaning the caller is looking for
// ---------------------------------------------------------------------------

/// The semantic meaning being sought in a name lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameLookupKind {
    /// Expression context: variables, functions, enum values, class constructors.
    Value,
    /// Type annotation context: interfaces, type aliases, classes-as-types.
    Type,
    /// Module/namespace context: `import X = Ns.Member`, qualified names.
    Namespace,
}

// ---------------------------------------------------------------------------
// NameResolutionRequest: what the checker wants to resolve
// ---------------------------------------------------------------------------

/// A request to resolve a name at a specific AST location.
#[derive(Debug, Clone)]
pub struct NameResolutionRequest<'a> {
    /// The identifier text being resolved.
    pub name: &'a str,
    /// The AST node where the identifier appears.
    pub node: NodeIndex,
    /// What kind of binding the caller is looking for.
    pub kind: NameLookupKind,
}

impl<'a> NameResolutionRequest<'a> {
    pub fn value(name: &'a str, node: NodeIndex) -> Self {
        Self {
            name,
            node,
            kind: NameLookupKind::Value,
        }
    }

    pub fn type_ref(name: &'a str, node: NodeIndex) -> Self {
        Self {
            name,
            node,
            kind: NameLookupKind::Type,
        }
    }

    pub fn namespace(name: &'a str, node: NodeIndex) -> Self {
        Self {
            name,
            node,
            kind: NameLookupKind::Namespace,
        }
    }
}

// ---------------------------------------------------------------------------
// ResolvedName: successful resolution outcomes
// ---------------------------------------------------------------------------

/// A successfully resolved name and how it was found.
#[derive(Debug, Clone)]
pub enum ResolvedName {
    /// Symbol found in scope with the expected meaning.
    Symbol(SymbolId),
    /// Found as a type parameter in the current generic context.
    TypeParameter(TypeId),
    /// Found as an intrinsic value (`undefined`, `NaN`, `Infinity`).
    Intrinsic(TypeId),
    /// Found as a known global value via lib or file_locals.
    GlobalValue(SymbolId),
}

// ---------------------------------------------------------------------------
// ResolutionFailure: why resolution failed
// ---------------------------------------------------------------------------

/// Classifies why a name could not be resolved to the expected meaning.
#[derive(Debug, Clone)]
pub enum ResolutionFailure {
    /// Name not found anywhere in the current scope chain or globals.
    NotFound,
    /// Found, but the binding has the wrong meaning (e.g., type used as value).
    WrongMeaning {
        symbol_id: SymbolId,
        /// What we found.
        found_as: NameLookupKind,
    },
    /// Found as a type-only import or export (TS1361/TS1362).
    TypeOnlyBinding {
        symbol_id: SymbolId,
        origin: TypeOnlyOrigin,
    },
    /// Found as an uninstantiated namespace used in value position (TS2708).
    NamespaceAsValue { namespace_name: String },
    /// Namespace/module exists but has no exported member with this name (TS2694).
    NotExported {
        namespace_name: String,
        member_name: String,
        available_exports: Vec<String>,
    },
    /// Resolution suppressed due to parse errors, import errors, or other
    /// cascading-error guards. No diagnostic should be emitted.
    Suppressed,
}

/// Where a type-only binding originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeOnlyOrigin {
    /// From `import type { X } from ...`
    ImportType,
    /// From `export type { X }`
    ExportType,
    /// Primitive type keyword used as value (number, string, etc.)
    PrimitiveKeyword,
    /// ES2015+ type with missing lib (Promise, Map, etc.)
    MissingLib,
}

// ---------------------------------------------------------------------------
// NameSuggestion: what to suggest when resolution fails
// ---------------------------------------------------------------------------

/// A suggestion to offer alongside a resolution failure diagnostic.
#[derive(Debug, Clone)]
pub enum NameSuggestion {
    /// No suggestion available.
    None,
    /// "Did you mean 'X'?" spelling suggestion (TS2552).
    Spelling(Vec<String>),
    /// "Did you mean the static member 'C.X'?" (TS2662).
    StaticMember { class_name: String },
    /// Suggest changing lib compiler option (TS2583).
    ChangeLib { suggested_lib: String },
    /// Suggest including 'dom' lib (TS2584).
    IncludeDomLib,
    /// Suggest installing @types/node (TS2591).
    InstallNodeTypes,
    /// Suggest installing @types/jquery (TS2592).
    InstallJQueryTypes,
    /// Suggest installing test runner types (TS2593).
    InstallTestTypes,
    /// Suggest installing @types/bun (TS2868).
    InstallBunTypes,
    /// Identifier is a primitive type keyword used as value (TS2693).
    TypeKeywordAsValue,
    /// "Did you mean 'Y'?" for namespace exports (TS2724).
    ExportSpelling { suggestion: String },
    /// Identifier is an unused renaming in typeof context.
    UnusedRenaming { original_name: String },
}

// ---------------------------------------------------------------------------
// ResolutionOutcome: combined result for the checker
// ---------------------------------------------------------------------------

/// The complete outcome of a name resolution attempt, combining the resolution
/// result with any applicable suggestion for diagnostics.
#[derive(Debug, Clone)]
pub struct ResolutionOutcome {
    /// Whether resolution succeeded or how it failed.
    pub result: Result<ResolvedName, ResolutionFailure>,
    /// What suggestion to attach if the caller emits a diagnostic.
    pub suggestion: NameSuggestion,
}

// ---------------------------------------------------------------------------
// classify_unresolved_name_suggestion: boundary-owned suggestion policy
// ---------------------------------------------------------------------------

/// Classify what kind of suggestion to offer for an unresolved name.
///
/// This is a pure classifier that examines the name and determines the
/// suggestion category. It does NOT perform scope lookups or spelling
/// search — those are done by the checker after consulting this policy.
///
/// Consolidates the scattered `is_known_dom_global`, `is_es2015_plus_type`,
/// `is_known_node_global`, etc. checks into a single boundary function.
pub fn classify_unresolved_name_suggestion(name: &str) -> UnresolvedNameClass {
    use tsz_binder::lib_loader;

    // Primitive type keywords (TS2693)
    if is_primitive_type_keyword(name) {
        return UnresolvedNameClass::PrimitiveTypeKeyword;
    }

    // ES2015+ types requiring lib change (TS2583)
    if lib_loader::is_es2015_plus_type(name) {
        let lib = lib_loader::get_suggested_lib_for_type(name);
        return UnresolvedNameClass::Es2015PlusType {
            suggested_lib: lib.to_string(),
        };
    }

    // Known DOM globals requiring 'dom' lib (TS2584)
    if crate::error_reporter::is_known_dom_global(name) {
        return UnresolvedNameClass::DomGlobal;
    }

    // Known Node.js globals requiring @types/node (TS2591)
    if is_known_node_global(name) {
        return UnresolvedNameClass::NodeGlobal;
    }

    // Known jQuery globals (TS2592)
    if is_known_jquery_global(name) {
        return UnresolvedNameClass::JQueryGlobal;
    }

    // Known test runner globals (TS2593)
    if is_known_test_runner_global(name) {
        return UnresolvedNameClass::TestRunnerGlobal;
    }

    // Bun global (TS2868)
    if name == "Bun" {
        return UnresolvedNameClass::BunGlobal;
    }

    UnresolvedNameClass::Unknown
}

/// Classification of an unresolved name for suggestion policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedNameClass {
    /// Not a known category — use spelling suggestions.
    Unknown,
    /// Primitive type keyword used as value (number, string, etc.).
    PrimitiveTypeKeyword,
    /// ES2015+ type that needs a specific lib (Promise, Map, etc.).
    Es2015PlusType { suggested_lib: String },
    /// DOM global that needs 'dom' lib.
    DomGlobal,
    /// Node.js global that needs @types/node.
    NodeGlobal,
    /// jQuery global that needs @types/jquery.
    JQueryGlobal,
    /// Test runner global that needs @types/jest or similar.
    TestRunnerGlobal,
    /// Bun global that needs @types/bun.
    BunGlobal,
}

/// Check whether a name is a TypeScript primitive type keyword.
pub fn is_primitive_type_keyword(name: &str) -> bool {
    matches!(
        name,
        "number"
            | "string"
            | "boolean"
            | "void"
            | "undefined"
            | "null"
            | "any"
            | "unknown"
            | "never"
            | "object"
            | "bigint"
    )
}

/// Check if a name is a known Node.js global.
fn is_known_node_global(name: &str) -> bool {
    matches!(
        name,
        "require" | "exports" | "module" | "process" | "Buffer" | "__filename" | "__dirname"
    )
}

/// Check if a name is a known jQuery global.
fn is_known_jquery_global(name: &str) -> bool {
    matches!(name, "$" | "jQuery")
}

/// Check if a name is a known test runner global.
fn is_known_test_runner_global(name: &str) -> bool {
    matches!(name, "describe" | "suite" | "it" | "test")
}

// ---------------------------------------------------------------------------
// Suggestion conversion helpers
// ---------------------------------------------------------------------------

impl UnresolvedNameClass {
    /// Convert this classification into a `NameSuggestion`.
    pub fn to_suggestion(&self) -> NameSuggestion {
        match self {
            UnresolvedNameClass::Unknown => NameSuggestion::None,
            UnresolvedNameClass::PrimitiveTypeKeyword => NameSuggestion::TypeKeywordAsValue,
            UnresolvedNameClass::Es2015PlusType { suggested_lib } => NameSuggestion::ChangeLib {
                suggested_lib: suggested_lib.clone(),
            },
            UnresolvedNameClass::DomGlobal => NameSuggestion::IncludeDomLib,
            UnresolvedNameClass::NodeGlobal => NameSuggestion::InstallNodeTypes,
            UnresolvedNameClass::JQueryGlobal => NameSuggestion::InstallJQueryTypes,
            UnresolvedNameClass::TestRunnerGlobal => NameSuggestion::InstallTestTypes,
            UnresolvedNameClass::BunGlobal => NameSuggestion::InstallBunTypes,
        }
    }
}

impl ResolutionOutcome {
    /// Create a successful outcome with no suggestion.
    pub fn found(resolved: ResolvedName) -> Self {
        Self {
            result: Ok(resolved),
            suggestion: NameSuggestion::None,
        }
    }

    /// Create a failure outcome.
    pub fn failed(failure: ResolutionFailure, suggestion: NameSuggestion) -> Self {
        Self {
            result: Err(failure),
            suggestion,
        }
    }

    /// Create a suppressed outcome (no diagnostic should be emitted).
    pub fn suppressed() -> Self {
        Self {
            result: Err(ResolutionFailure::Suppressed),
            suggestion: NameSuggestion::None,
        }
    }

    /// Whether this outcome represents a successful resolution.
    pub fn is_found(&self) -> bool {
        self.result.is_ok()
    }

    /// Whether this outcome should suppress diagnostic emission.
    pub fn is_suppressed(&self) -> bool {
        matches!(self.result, Err(ResolutionFailure::Suppressed))
    }
}
