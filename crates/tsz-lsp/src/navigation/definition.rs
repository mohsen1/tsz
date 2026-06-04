use crate::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};

use crate::utils::{
    find_node_at_or_before_offset, find_symbol_query_node_at_or_before, is_comment_context,
    is_symbol_query_node, should_backtrack_to_previous_symbol,
};

use tsz_binder::{SymbolId, symbol_flags};

use tsz_common::position::{Location, Position, Range};

use tsz_parser::NodeIndex;

use tsz_parser::syntax_kind_ext;

/// Well-known built-in global identifiers that are provided by the runtime
/// environment and not defined in user source files.
/// When these are encountered and no declaration is found, we return None
/// instead of crashing or returning garbage positions.
const BUILTIN_GLOBALS: &[&str] = &[
    // Console API
    "console",
    // Fundamental objects
    "Object",
    "Function",
    "Boolean",
    "Symbol",
    // Error types
    "Error",
    "AggregateError",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    // Numbers and dates
    "Number",
    "BigInt",
    "Math",
    "Date",
    "Infinity",
    "NaN",
    "undefined",
    // Text processing
    "String",
    "RegExp",
    // Indexed collections
    "Array",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    // Keyed collections
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "WeakRef",
    // Structured data
    "ArrayBuffer",
    "SharedArrayBuffer",
    "Atomics",
    "DataView",
    "JSON",
    // Control abstraction
    "Promise",
    "Generator",
    "GeneratorFunction",
    "AsyncFunction",
    "AsyncGenerator",
    "AsyncGeneratorFunction",
    // Reflection
    "Reflect",
    "Proxy",
    // Internationalization
    "Intl",
    // Web APIs
    "globalThis",
    "window",
    "document",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "fetch",
    "Headers",
    "Request",
    "Response",
    "URL",
    "URLSearchParams",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "requestAnimationFrame",
    "cancelAnimationFrame",
    "queueMicrotask",
    "structuredClone",
    "atob",
    "btoa",
    "TextEncoder",
    "TextDecoder",
    "AbortController",
    "AbortSignal",
    "Blob",
    "File",
    "FileReader",
    "FormData",
    "ReadableStream",
    "WritableStream",
    "TransformStream",
    "Event",
    "EventTarget",
    "CustomEvent",
    "MutationObserver",
    "IntersectionObserver",
    "ResizeObserver",
    "PerformanceObserver",
    "WebSocket",
    "Worker",
    "MessageChannel",
    "MessagePort",
    "BroadcastChannel",
    // Node.js globals
    "process",
    "Buffer",
    "require",
    "module",
    "exports",
    "__dirname",
    "__filename",
    "global",
    // TypeScript utility types (may appear as identifiers)
    "Partial",
    "Required",
    "Readonly",
    "Record",
    "Pick",
    "Omit",
    "Exclude",
    "Extract",
    "NonNullable",
    "Parameters",
    "ConstructorParameters",
    "ReturnType",
    "InstanceType",
    "ThisParameterType",
    "OmitThisParameter",
    "ThisType",
    "Awaited",
    // Iterator/Iterable
    "Iterator",
    "IterableIterator",
    "AsyncIterableIterator",
];

/// Check if a name is a well-known built-in global.
fn is_builtin_global(name: &str) -> bool {
    BUILTIN_GLOBALS.contains(&name)
}

/// Rich definition information matching TypeScript's tsserver response format.
/// Includes metadata about the symbol kind, name, and declaration context.
#[derive(Debug, Clone)]
pub struct DefinitionInfo {
    /// The location of the identifier name within the declaration.
    pub location: Location,
    /// The span of the entire declaration (contextSpan in tsserver).
    pub context_span: Option<Range>,
    /// The symbol name (e.g., "ambientVar").
    pub name: String,
    /// The symbol kind string (e.g., "var", "function", "class").
    pub kind: String,
    /// The container name (e.g., class name for a method).
    pub container_name: String,
    /// The container kind string.
    pub container_kind: String,
    /// Whether the symbol is local (not exported).
    pub is_local: bool,
    /// Whether the symbol is ambient (declared with `declare`).
    pub is_ambient: bool,
}

define_lsp_provider!(binder GoToDefinition, "Go-to-Definition provider.");

include!("definition_parts/part1.rs");
include!("definition_parts/part2.rs");

#[cfg(test)]
#[path = "../../tests/definition_tests.rs"]
mod definition_tests;
