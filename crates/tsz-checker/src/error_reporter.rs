//! Error reporting (`error_*` for emission, `report_*` for higher-level wrappers).
//! This module is split into focused submodules for maintainability.

/// Whether a type-only symbol came from `import type` or `export type`.
pub(crate) enum TypeOnlyKind {
    Import,
    Export,
}

// Submodules
mod assignability;
mod call_errors;
mod core;
mod generics;
mod name_resolution;
mod operator_errors;
mod properties;
mod suggestions;
mod type_value;

// =============================================================================
// Free Functions (Module-Level)
// =============================================================================

/// Check if a name is a known DOM or `ScriptHost` global that requires the 'dom' lib.
/// These names are well-known browser/runtime APIs that tsc suggests including
/// the 'dom' lib for when they can't be resolved (TS2584).
pub fn is_known_dom_global(name: &str) -> bool {
    match name {
        // Console
        "console"
        // Window/Document
        | "window" | "document" | "self"
        // DOM elements
        | "HTMLElement" | "HTMLDivElement" | "HTMLSpanElement" | "HTMLInputElement"
        | "HTMLButtonElement" | "HTMLAnchorElement" | "HTMLImageElement"
        | "HTMLCanvasElement" | "HTMLFormElement" | "HTMLSelectElement"
        | "HTMLTextAreaElement" | "HTMLTableElement" | "HTMLMediaElement"
        | "HTMLVideoElement" | "HTMLAudioElement"
        // Core DOM interfaces
        | "Element" | "Node" | "Document" | "Event" | "EventTarget"
        | "NodeList" | "HTMLCollection" | "DOMTokenList"
        // Common Web APIs
        | "XMLHttpRequest" | "fetch" | "Request" | "Response" | "Headers"
        | "URL" | "URLSearchParams"
        | "setTimeout" | "clearTimeout" | "setInterval" | "clearInterval"
        | "requestAnimationFrame" | "cancelAnimationFrame"
        | "alert" | "confirm" | "prompt"
        // Storage
        | "localStorage" | "sessionStorage" | "Storage"
        // Navigator/Location/History
        | "navigator" | "Navigator" | "location" | "Location" | "history" | "History"
        // Events
        | "MouseEvent" | "KeyboardEvent" | "TouchEvent" | "FocusEvent"
        | "CustomEvent" | "MessageEvent" | "ErrorEvent"
        | "addEventListener" | "removeEventListener"
        // Canvas/Media
        | "CanvasRenderingContext2D" | "WebGLRenderingContext"
        | "MediaStream" | "MediaRecorder"
        // Workers/ServiceWorker
        | "Worker" | "ServiceWorker" | "SharedWorker"
        // Misc browser globals
        | "MutationObserver" | "IntersectionObserver" | "ResizeObserver"
        | "Performance" | "performance"
        | "Blob" | "File" | "FileReader" | "FormData"
        | "WebSocket" | "ClipboardEvent" | "DragEvent"
        | "getComputedStyle" | "matchMedia"
        | "DOMException" | "AbortController" | "AbortSignal"
        | "TextEncoder" | "TextDecoder"
        | "crypto" | "Crypto" | "SubtleCrypto"
        | "queueMicrotask" | "structuredClone"
        | "atob" | "btoa" => true,
        _ => false,
    }
}

/// Check if a name is a known Node.js global that requires @types/node (TS2580).
pub fn is_known_node_global(name: &str) -> bool {
    matches!(
        name,
        "require" | "exports" | "module" | "process" | "Buffer" | "__filename" | "__dirname"
    )
}

/// Check if a name is a known test runner global that requires @types/jest or @types/mocha (TS2582).
pub fn is_known_test_runner_global(name: &str) -> bool {
    matches!(name, "describe" | "suite" | "it" | "test")
}
