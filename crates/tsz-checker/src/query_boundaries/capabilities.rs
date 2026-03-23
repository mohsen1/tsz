//! Environment capabilities boundary for lib/config/feature-gate diagnostics.
//!
//! This module centralizes all environment and capability queries that determine
//! whether a given TypeScript feature is available under the current compiler
//! configuration. Instead of scattering ad-hoc `match compiler_options.module`
//! checks across the checker, diagnostic-producing code queries this boundary.
//!
//! Diagnostic families routed through this boundary:
//! - TS2318: Cannot find global type (missing lib types)
//! - TS2591: Cannot find name (Node.js globals)
//! - TS2583: Cannot find name (ES2015+ lib suggestion)
//! - TS2584: Cannot find name (DOM lib suggestion)
//! - TS2823: Import attributes require specific module option
//! - TS2854: Top-level await using requires specific module/target
//! - TS5071: resolveJsonModule incompatible with module option
//! - TS5107: Deprecated compiler option

use tsz_common::common::{ModuleKind, ScriptTarget};

/// Feature gates that can be queried against the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureGate {
    /// `import ... with { ... }` / `import ... assert { ... }` syntax
    ImportAttributes,
    /// `using` declaration (requires Disposable global)
    UsingDeclaration,
    /// `await using` declaration (requires `AsyncDisposable` global)
    AwaitUsingDeclaration,
    /// Top-level `await using` (requires specific module + target)
    TopLevelAwaitUsing,
    /// Top-level `await` expression (requires same module + target as TopLevelAwaitUsing)
    TopLevelAwait,
    /// `resolveJsonModule` option validity
    ResolveJsonModule,
    /// Generator functions (requires `IterableIterator`)
    Generators,
    /// Async generator functions (requires `AsyncIterableIterator`)
    AsyncGenerators,
    /// Experimental decorators (requires `TypedPropertyDescriptor`)
    ExperimentalDecorators,
    /// Async functions (requires Promise global type)
    AsyncFunction,
    /// Async functions in ES5 (requires Promise constructor)
    AsyncFunctionEs5,
}

/// The kind of global name that was not found, determining which diagnostic to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingGlobalKind {
    /// Core global type (Array, String, etc.) → TS2318
    CoreGlobalType,
    /// Feature-specific global type (Awaited, Disposable, etc.) → TS2318
    FeatureGlobalType,
    /// ES2015+ type used as value → TS2583
    Es2015PlusType,
    /// DOM/ScriptHost global → TS2584
    DomGlobal,
    /// Node.js global → TS2591
    NodeGlobal,
    /// jQuery global → TS2592
    JQueryGlobal,
    /// Test runner global → TS2593
    TestRunnerGlobal,
    /// Bun global → TS2868
    BunGlobal,
}

/// Precomputed environment capabilities matrix.
///
/// Built once per checker context from compiler options + loaded libs.
/// Answers "is feature X available?" without re-examining options each time.
#[derive(Debug, Clone)]
pub struct EnvironmentCapabilities {
    // --- Lib availability ---
    /// Whether any lib files are loaded (not --noLib and actual files present)
    pub has_lib: bool,
    /// Whether --noLib is explicitly set
    pub no_lib: bool,

    // --- Module kind ---
    pub module: ModuleKind,
    /// Whether the module option was explicitly set by the user
    pub module_explicitly_set: bool,

    // --- Target ---
    pub target: ScriptTarget,

    // --- Feature gates (precomputed from module + target) ---
    /// Import attributes supported: module in {`ESNext`, Node18, Node20, `NodeNext`, Preserve}
    pub import_attributes_supported: bool,
    /// Top-level await using supported: requires specific module AND target >= ES2017
    pub top_level_await_using_supported: bool,
    /// resolveJsonModule compatible with current module kind
    pub resolve_json_module_compatible: bool,

    // --- Config flags ---
    pub resolve_json_module: bool,
    pub experimental_decorators: bool,
    pub ignore_deprecations: bool,
    pub verbatim_module_syntax: bool,

    // --- Deprecation state (set by driver after config parsing) ---
    /// Whether TS5107/TS5101 deprecation diagnostics were produced during config parsing.
    /// When true, tsc stops compilation early and skips lib type resolution.
    pub has_deprecation_diagnostics: bool,
}

impl EnvironmentCapabilities {
    /// Build capabilities from compiler options and lib state.
    pub const fn from_options(
        options: &tsz_common::checker_options::CheckerOptions,
        has_lib: bool,
    ) -> Self {
        let import_attributes_supported = matches!(
            options.module,
            ModuleKind::ESNext
                | ModuleKind::Node18
                | ModuleKind::Node20
                | ModuleKind::NodeNext
                | ModuleKind::Preserve
        );

        // Top-level await using requires:
        // module in {ES2022, ESNext, System, Node16, Node18, Node20, NodeNext, Preserve}
        // AND target >= ES2017
        let top_level_await_using_module = matches!(
            options.module,
            ModuleKind::ES2022
                | ModuleKind::ESNext
                | ModuleKind::System
                | ModuleKind::Node16
                | ModuleKind::Node18
                | ModuleKind::Node20
                | ModuleKind::NodeNext
                | ModuleKind::Preserve
        );
        let top_level_await_using_target = options.target as u32 >= ScriptTarget::ES2017 as u32;
        let top_level_await_using_supported =
            top_level_await_using_module && top_level_await_using_target;

        // resolveJsonModule cannot be used with module=none/system/umd
        let resolve_json_module_compatible = !matches!(
            options.module,
            ModuleKind::None | ModuleKind::System | ModuleKind::UMD
        );

        Self {
            has_lib,
            no_lib: options.no_lib,
            module: options.module,
            module_explicitly_set: options.module_explicitly_set,
            target: options.target,
            import_attributes_supported,
            top_level_await_using_supported,
            resolve_json_module_compatible,
            resolve_json_module: options.resolve_json_module,
            experimental_decorators: options.experimental_decorators,
            ignore_deprecations: options.ignore_deprecations,
            verbatim_module_syntax: options.verbatim_module_syntax,
            has_deprecation_diagnostics: false, // set by driver after config parsing
        }
    }

    /// Check whether a feature gate is satisfied by the current environment.
    pub const fn feature_available(&self, gate: FeatureGate) -> bool {
        match gate {
            FeatureGate::ImportAttributes => self.import_attributes_supported,
            FeatureGate::TopLevelAwaitUsing | FeatureGate::TopLevelAwait => {
                self.top_level_await_using_supported
            }
            FeatureGate::ResolveJsonModule => self.resolve_json_module_compatible,
            // For these gates, we check lib availability (global type presence is separate)
            FeatureGate::UsingDeclaration
            | FeatureGate::AwaitUsingDeclaration
            | FeatureGate::Generators
            | FeatureGate::AsyncGenerators
            | FeatureGate::ExperimentalDecorators
            | FeatureGate::AsyncFunction
            | FeatureGate::AsyncFunctionEs5 => {
                // These require specific global types; lib presence is necessary but
                // the caller must also check global type availability via has_name_in_lib.
                self.has_lib
            }
        }
    }

    /// Classify a missing global name into a diagnostic kind.
    ///
    /// This is the single decision point for "which diagnostic code should be used
    /// when name X is not found?" — replaces the scattered classifier chain in
    /// `report_cannot_find_name_internal`.
    pub fn classify_missing_global(&self, name: &str) -> Option<MissingGlobalKind> {
        // Node.js globals
        if is_known_node_global(name) {
            return Some(MissingGlobalKind::NodeGlobal);
        }
        // DOM/ScriptHost globals
        if is_known_dom_global(name) {
            return Some(MissingGlobalKind::DomGlobal);
        }
        // jQuery globals
        if is_known_jquery_global(name) {
            return Some(MissingGlobalKind::JQueryGlobal);
        }
        // Test runner globals
        if is_known_test_runner_global(name) {
            return Some(MissingGlobalKind::TestRunnerGlobal);
        }
        // Bun global
        if name == "Bun" {
            return Some(MissingGlobalKind::BunGlobal);
        }
        // ES2015+ types that need lib upgrade
        if tsz_binder::lib_loader::is_es2015_plus_type(name) {
            return Some(MissingGlobalKind::Es2015PlusType);
        }
        None
    }

    /// Get the required global type name for a feature gate.
    pub const fn required_global_type(gate: FeatureGate) -> Option<&'static str> {
        match gate {
            FeatureGate::UsingDeclaration => Some("Disposable"),
            FeatureGate::AwaitUsingDeclaration => Some("AsyncDisposable"),
            FeatureGate::Generators => Some("IterableIterator"),
            FeatureGate::AsyncGenerators => Some("AsyncIterableIterator"),
            FeatureGate::ExperimentalDecorators => Some("TypedPropertyDescriptor"),
            FeatureGate::AsyncFunction => Some("Promise"),
            _ => None,
        }
    }

    /// Map a feature-specific global type name back to its feature gate.
    ///
    /// This is the reverse lookup for `required_global_type()` — given a type
    /// name like `"Disposable"`, returns the gate that requires it. Used to
    /// determine whether a missing global type should produce a TS2318
    /// diagnostic based on file-level feature usage.
    pub const fn gate_for_required_type(type_name: &str) -> Option<FeatureGate> {
        // const fn doesn't support &str matching, so use byte comparison
        match type_name.as_bytes() {
            b"Disposable" => Some(FeatureGate::UsingDeclaration),
            b"AsyncDisposable" => Some(FeatureGate::AwaitUsingDeclaration),
            b"IterableIterator" => Some(FeatureGate::Generators),
            b"AsyncIterableIterator" => Some(FeatureGate::AsyncGenerators),
            b"TypedPropertyDescriptor" => Some(FeatureGate::ExperimentalDecorators),
            b"Promise" => Some(FeatureGate::AsyncFunction),
            b"Awaited" => Some(FeatureGate::AsyncFunction),
            _ => None,
        }
    }
}

// =============================================================================
// Global name classifiers (moved from name_resolution.rs to centralize)
// =============================================================================

/// Check if a name is a known Node.js global or built-in module name
/// that requires @types/node (TS2591).
///
/// tsc emits TS2591 for both:
/// 1. Node.js runtime globals: `require`, `process`, `Buffer`, etc.
/// 2. Node.js built-in module names used as identifiers (e.g. from
///    `import fs = require("fs")`): `fs`, `url`, `events`, etc.
/// 3. `node:`-prefixed module specifiers used as names.
pub(crate) fn is_known_node_global(name: &str) -> bool {
    // Node.js runtime globals
    if matches!(
        name,
        "require" | "exports" | "module" | "process" | "Buffer" | "__filename" | "__dirname"
    ) {
        return true;
    }
    // Node.js built-in module names (commonly used as identifiers via
    // `import X = require("X")` patterns).
    // NOTE: "console" is intentionally excluded — tsc classifies standalone
    // "console" as a DOM global (TS2584), not a Node global (TS2591).
    // NOTE: "assert" is intentionally excluded — tsc classifies standalone
    // "assert" as TS2304 (Cannot find name), not TS2591.  While "assert" is
    // a Node.js built-in module, it is NOT a global variable and tsc never
    // suggests @types/node installation for it.
    if matches!(
        name,
        "buffer"
            | "child_process"
            | "cluster"
            | "constants"
            | "crypto"
            | "dgram"
            | "dns"
            | "domain"
            | "events"
            | "fs"
            | "http"
            | "http2"
            | "https"
            | "inspector"
            | "module"
            | "net"
            | "os"
            | "path"
            | "perf_hooks"
            | "punycode"
            | "querystring"
            | "readline"
            | "repl"
            | "stream"
            | "string_decoder"
            | "sys"
            | "timers"
            | "tls"
            | "tty"
            | "url"
            | "util"
            | "v8"
            | "vm"
            | "worker_threads"
            | "zlib"
    ) {
        return true;
    }
    // node:-prefixed module specifiers (e.g. "node:path")
    if name.starts_with("node:") {
        return true;
    }
    false
}

/// Check if a module specifier is a known Node.js built-in module (TS2591).
///
/// When module resolution fails for one of these specifiers, tsc emits TS2591
/// ("Cannot find name 'X'. Do you need to install type definitions for node?")
/// instead of TS2307 ("Cannot find module 'X'").
pub(crate) fn is_known_node_module(specifier: &str) -> bool {
    // Handle `node:` prefix (e.g., `node:fs`, `node:path`)
    let name = specifier.strip_prefix("node:").unwrap_or(specifier);
    matches!(
        name,
        "assert"
            | "async_hooks"
            | "buffer"
            | "child_process"
            | "cluster"
            | "console"
            | "constants"
            | "crypto"
            | "dgram"
            | "diagnostics_channel"
            | "dns"
            | "domain"
            | "events"
            | "fs"
            | "http"
            | "http2"
            | "https"
            | "inspector"
            | "module"
            | "net"
            | "os"
            | "path"
            | "perf_hooks"
            | "process"
            | "punycode"
            | "querystring"
            | "readline"
            | "repl"
            | "stream"
            | "string_decoder"
            | "sys"
            | "timers"
            | "tls"
            | "trace_events"
            | "tty"
            | "url"
            | "util"
            | "v8"
            | "vm"
            | "wasi"
            | "worker_threads"
            | "zlib"
    )
}

/// Check if a name is a known DOM/ScriptHost global that requires 'dom' lib (TS2584).
pub(crate) fn is_known_dom_global(name: &str) -> bool {
    matches!(
        name,
        "window"
            | "document"
            | "console"
            | "setTimeout"
            | "clearTimeout"
            | "setInterval"
            | "clearInterval"
            | "requestAnimationFrame"
            | "cancelAnimationFrame"
            | "alert"
            | "confirm"
            | "prompt"
            | "fetch"
            | "navigator"
            | "location"
            | "localStorage"
            | "sessionStorage"
            | "XMLHttpRequest"
            | "HTMLElement"
            | "HTMLDivElement"
            | "HTMLInputElement"
            | "HTMLButtonElement"
            | "HTMLFormElement"
            | "HTMLImageElement"
            | "HTMLAnchorElement"
            | "HTMLTableElement"
            | "HTMLCanvasElement"
            | "HTMLVideoElement"
            | "HTMLAudioElement"
            | "HTMLSelectElement"
            | "HTMLTextAreaElement"
            | "Event"
            | "MouseEvent"
            | "KeyboardEvent"
            | "TouchEvent"
            | "FocusEvent"
            | "CustomEvent"
            | "EventTarget"
            | "Node"
            | "NodeList"
            | "Element"
            | "Document"
            | "DocumentFragment"
            | "MutationObserver"
            | "IntersectionObserver"
            | "ResizeObserver"
            | "URL"
            | "URLSearchParams"
            | "AbortController"
            | "AbortSignal"
            | "FormData"
            | "Headers"
            | "Request"
            | "Response"
            | "Blob"
            | "File"
            | "FileReader"
            | "FileList"
            | "Worker"
            | "ServiceWorker"
            | "WebSocket"
            | "Performance"
            | "PerformanceObserver"
            | "Crypto"
            | "SubtleCrypto"
            | "TextEncoder"
            | "TextDecoder"
            | "DOMParser"
            | "Selection"
            | "Range"
            | "SVGElement"
            | "CSSStyleDeclaration"
            | "MediaQueryList"
            | "CanvasRenderingContext2D"
            | "WebGLRenderingContext"
            | "AudioContext"
            | "OfflineAudioContext"
            | "BroadcastChannel"
            | "MessageChannel"
            | "MessagePort"
            | "ReadableStream"
            | "WritableStream"
            | "TransformStream"
            | "CompressionStream"
            | "DecompressionStream"
            | "StructuredSerializeOptions"
    )
}

/// Check if a name is a known jQuery global that requires @types/jquery (TS2592).
pub(crate) fn is_known_jquery_global(name: &str) -> bool {
    matches!(name, "$" | "jQuery")
}

/// Check if a name is a known test runner global that requires @types/jest or @types/mocha (TS2593).
pub(crate) fn is_known_test_runner_global(name: &str) -> bool {
    matches!(name, "describe" | "suite" | "it" | "test")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_attributes_feature_gate() {
        // ESNext supports import attributes
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::ESNext,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.feature_available(FeatureGate::ImportAttributes));

        // CommonJS does not
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.feature_available(FeatureGate::ImportAttributes));

        // Preserve supports it
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::Preserve,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.feature_available(FeatureGate::ImportAttributes));
    }

    #[test]
    fn test_top_level_await_using_gate() {
        // ES2022 module + ESNext target → supported
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::ES2022,
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.feature_available(FeatureGate::TopLevelAwaitUsing));

        // CommonJS + ESNext target → not supported (wrong module)
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.feature_available(FeatureGate::TopLevelAwaitUsing));

        // ESNext module + ES5 target → not supported (wrong target)
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES5,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.feature_available(FeatureGate::TopLevelAwaitUsing));
    }

    #[test]
    fn test_resolve_json_module_compatibility() {
        // CommonJS is compatible
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.resolve_json_module_compatible);

        // None is incompatible
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::None,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.resolve_json_module_compatible);

        // System is incompatible
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::System,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.resolve_json_module_compatible);

        // UMD is incompatible
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::UMD,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.resolve_json_module_compatible);
    }

    #[test]
    fn test_classify_missing_global() {
        let opts = tsz_common::checker_options::CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, true);

        assert_eq!(
            caps.classify_missing_global("require"),
            Some(MissingGlobalKind::NodeGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("process"),
            Some(MissingGlobalKind::NodeGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("console"),
            Some(MissingGlobalKind::DomGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("document"),
            Some(MissingGlobalKind::DomGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("$"),
            Some(MissingGlobalKind::JQueryGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("describe"),
            Some(MissingGlobalKind::TestRunnerGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("Bun"),
            Some(MissingGlobalKind::BunGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("Promise"),
            Some(MissingGlobalKind::Es2015PlusType)
        );
        assert_eq!(caps.classify_missing_global("myCustomVar"), None);
    }

    #[test]
    fn test_required_global_type_for_feature() {
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::UsingDeclaration),
            Some("Disposable")
        );
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::AwaitUsingDeclaration),
            Some("AsyncDisposable")
        );
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::Generators),
            Some("IterableIterator")
        );
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::ImportAttributes),
            None
        );
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::AsyncFunction),
            Some("Promise")
        );
    }

    #[test]
    fn test_gate_for_required_type_reverse_lookup() {
        // Forward and reverse mappings must be consistent
        let gates = [
            FeatureGate::UsingDeclaration,
            FeatureGate::AwaitUsingDeclaration,
            FeatureGate::Generators,
            FeatureGate::AsyncGenerators,
            FeatureGate::ExperimentalDecorators,
            FeatureGate::AsyncFunction,
        ];
        for gate in gates {
            if let Some(type_name) = EnvironmentCapabilities::required_global_type(gate) {
                assert_eq!(
                    EnvironmentCapabilities::gate_for_required_type(type_name),
                    Some(gate),
                    "Reverse lookup for type '{type_name}' (gate {gate:?}) should match"
                );
            }
        }
    }

    #[test]
    fn test_async_function_gate_no_lib() {
        let opts = tsz_common::checker_options::CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, false);
        assert!(!caps.feature_available(FeatureGate::AsyncFunction));
    }

    #[test]
    fn test_async_function_gate_with_lib() {
        let opts = tsz_common::checker_options::CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.feature_available(FeatureGate::AsyncFunction));
    }

    #[test]
    fn test_deprecation_diagnostics_default_false() {
        let opts = tsz_common::checker_options::CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.has_deprecation_diagnostics);
    }
}
