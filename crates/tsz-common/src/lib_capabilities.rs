//! Shared TypeScript library symbol capability metadata.
//!
//! This table is the common policy source for code that needs to decide whether
//! a missing global should be treated as a baseline lib symbol, an ES lib
//! upgrade suggestion, or a DOM-only global.

/// Compiler lib that first provides a known global symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredLib {
    Es5,
    Es2015,
    Es2017,
    Es2018,
    Es2020,
    Es2021,
    EsNext,
    Dom,
}

impl RequiredLib {
    /// Return the canonical compiler-option lib name used in diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Es5 => "es5",
            Self::Es2015 => "es2015",
            Self::Es2017 => "es2017",
            Self::Es2018 => "es2018",
            Self::Es2020 => "es2020",
            Self::Es2021 => "es2021",
            Self::EsNext => "esnext",
            Self::Dom => "dom",
        }
    }
}

/// How the compiler policy uses a lib symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibCapabilityKind {
    /// Expected global merged from the active lib set.
    Global,
    /// Type-position name that can produce a TS2583 lib suggestion.
    Type,
    /// Value-position name that can produce a TS2583 lib suggestion.
    Value,
}

/// One known global capability supplied by a TypeScript lib.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibCapability {
    pub symbol: &'static str,
    pub kind: LibCapabilityKind,
    pub introduced_in: RequiredLib,
    pub required_lib: RequiredLib,
}

impl LibCapability {
    const fn global(symbol: &'static str, required_lib: RequiredLib) -> Self {
        Self {
            symbol,
            kind: LibCapabilityKind::Global,
            introduced_in: required_lib,
            required_lib,
        }
    }

    const fn type_name(symbol: &'static str, required_lib: RequiredLib) -> Self {
        Self {
            symbol,
            kind: LibCapabilityKind::Type,
            introduced_in: required_lib,
            required_lib,
        }
    }

    const fn value(symbol: &'static str, required_lib: RequiredLib) -> Self {
        Self {
            symbol,
            kind: LibCapabilityKind::Value,
            introduced_in: required_lib,
            required_lib,
        }
    }
}

/// Central policy table for known lib-provided globals.
pub const LIB_CAPABILITIES: &[LibCapability] = &[
    // Baseline ECMAScript globals validated after lib merge.
    LibCapability::global("Object", RequiredLib::Es5),
    LibCapability::global("Function", RequiredLib::Es5),
    LibCapability::global("Array", RequiredLib::Es5),
    LibCapability::global("String", RequiredLib::Es5),
    LibCapability::global("Number", RequiredLib::Es5),
    LibCapability::global("Boolean", RequiredLib::Es5),
    LibCapability::global("Symbol", RequiredLib::Es2015),
    LibCapability::global("BigInt", RequiredLib::Es2020),
    LibCapability::global("Error", RequiredLib::Es5),
    LibCapability::global("EvalError", RequiredLib::Es5),
    LibCapability::global("RangeError", RequiredLib::Es5),
    LibCapability::global("ReferenceError", RequiredLib::Es5),
    LibCapability::global("SyntaxError", RequiredLib::Es5),
    LibCapability::global("TypeError", RequiredLib::Es5),
    LibCapability::global("URIError", RequiredLib::Es5),
    LibCapability::global("Map", RequiredLib::Es2015),
    LibCapability::global("Set", RequiredLib::Es2015),
    LibCapability::global("WeakMap", RequiredLib::Es2015),
    LibCapability::global("WeakSet", RequiredLib::Es2015),
    LibCapability::global("Promise", RequiredLib::Es2015),
    LibCapability::global("Reflect", RequiredLib::Es2015),
    LibCapability::global("Proxy", RequiredLib::Es2015),
    LibCapability::global("eval", RequiredLib::Es5),
    LibCapability::global("isNaN", RequiredLib::Es5),
    LibCapability::global("isFinite", RequiredLib::Es5),
    LibCapability::global("parseFloat", RequiredLib::Es5),
    LibCapability::global("parseInt", RequiredLib::Es5),
    LibCapability::global("Infinity", RequiredLib::Es5),
    LibCapability::global("NaN", RequiredLib::Es5),
    LibCapability::global("undefined", RequiredLib::Es5),
    // DOM-only globals are tracked here but are not part of baseline validation.
    LibCapability::global("console", RequiredLib::Dom),
    // Type-position names that can produce TS2583.
    LibCapability::type_name("Promise", RequiredLib::Es2015),
    LibCapability::type_name("PromiseConstructor", RequiredLib::Es2015),
    LibCapability::type_name("PromiseConstructorLike", RequiredLib::Es2015),
    LibCapability::type_name("PromiseSettledResult", RequiredLib::Es2015),
    LibCapability::type_name("PromiseFulfilledResult", RequiredLib::Es2015),
    LibCapability::type_name("PromiseRejectedResult", RequiredLib::Es2015),
    LibCapability::type_name("Map", RequiredLib::Es2015),
    LibCapability::type_name("MapConstructor", RequiredLib::Es2015),
    LibCapability::type_name("Set", RequiredLib::Es2015),
    LibCapability::type_name("SetConstructor", RequiredLib::Es2015),
    LibCapability::type_name("WeakMap", RequiredLib::Es2015),
    LibCapability::type_name("WeakMapConstructor", RequiredLib::Es2015),
    LibCapability::type_name("WeakSet", RequiredLib::Es2015),
    LibCapability::type_name("WeakSetConstructor", RequiredLib::Es2015),
    LibCapability::type_name("Proxy", RequiredLib::Es2015),
    LibCapability::type_name("ProxyHandler", RequiredLib::Es2015),
    LibCapability::type_name("ProxyConstructor", RequiredLib::Es2015),
    LibCapability::type_name("Reflect", RequiredLib::Es2015),
    LibCapability::type_name("Symbol", RequiredLib::Es2015),
    LibCapability::type_name("SymbolConstructor", RequiredLib::Es2015),
    LibCapability::type_name("Iterator", RequiredLib::Es2015),
    LibCapability::type_name("IterableIterator", RequiredLib::Es2015),
    LibCapability::type_name("IteratorResult", RequiredLib::Es2015),
    LibCapability::type_name("IteratorYieldResult", RequiredLib::Es2015),
    LibCapability::type_name("IteratorReturnResult", RequiredLib::Es2015),
    LibCapability::type_name("AsyncIterator", RequiredLib::Es2015),
    LibCapability::type_name("AsyncIterable", RequiredLib::Es2015),
    LibCapability::type_name("AsyncIterableIterator", RequiredLib::Es2015),
    LibCapability::type_name("Generator", RequiredLib::Es2015),
    LibCapability::type_name("GeneratorFunction", RequiredLib::Es2015),
    LibCapability::type_name("GeneratorFunctionConstructor", RequiredLib::Es2015),
    LibCapability::type_name("ArrayLike", RequiredLib::Es2015),
    LibCapability::type_name("ReadonlyMap", RequiredLib::Es2015),
    LibCapability::type_name("ReadonlySet", RequiredLib::Es2015),
    LibCapability::type_name("TemplateStringsArray", RequiredLib::Es2015),
    LibCapability::type_name("TypedPropertyDescriptor", RequiredLib::Es2015),
    LibCapability::type_name("CallableFunction", RequiredLib::Es2015),
    LibCapability::type_name("NewableFunction", RequiredLib::Es2015),
    LibCapability::type_name("PropertyKey", RequiredLib::Es2015),
    LibCapability::type_name("AsyncFunction", RequiredLib::Es2015),
    LibCapability::type_name("AsyncFunctionConstructor", RequiredLib::Es2015),
    LibCapability::type_name("SharedArrayBuffer", RequiredLib::Es2017),
    LibCapability::type_name("SharedArrayBufferConstructor", RequiredLib::Es2017),
    LibCapability::type_name("Atomics", RequiredLib::Es2017),
    LibCapability::type_name("AsyncGenerator", RequiredLib::Es2018),
    LibCapability::type_name("AsyncGeneratorFunction", RequiredLib::Es2018),
    LibCapability::type_name("AsyncGeneratorFunctionConstructor", RequiredLib::Es2018),
    LibCapability::type_name("ObjectEntries", RequiredLib::Es2015),
    LibCapability::type_name("ObjectValues", RequiredLib::Es2015),
    LibCapability::type_name("BigInt", RequiredLib::Es2020),
    LibCapability::type_name("BigIntConstructor", RequiredLib::Es2020),
    LibCapability::type_name("BigInt64Array", RequiredLib::Es2020),
    LibCapability::type_name("BigInt64ArrayConstructor", RequiredLib::Es2020),
    LibCapability::type_name("BigUint64Array", RequiredLib::Es2020),
    LibCapability::type_name("BigUint64ArrayConstructor", RequiredLib::Es2020),
    LibCapability::type_name("FinalizationRegistry", RequiredLib::Es2021),
    LibCapability::type_name("FinalizationRegistryConstructor", RequiredLib::Es2021),
    LibCapability::type_name("WeakRef", RequiredLib::Es2021),
    LibCapability::type_name("WeakRefConstructor", RequiredLib::Es2021),
    LibCapability::type_name("AggregateError", RequiredLib::Es2021),
    LibCapability::type_name("AggregateErrorConstructor", RequiredLib::Es2021),
    LibCapability::type_name("Awaited", RequiredLib::Es2015),
    LibCapability::type_name("ErrorOptions", RequiredLib::Es2021),
    LibCapability::type_name("Disposable", RequiredLib::EsNext),
    LibCapability::type_name("AsyncDisposable", RequiredLib::EsNext),
    // Value-position names that tsc upgrades from TS2304 to TS2583.
    LibCapability::value("Map", RequiredLib::Es2015),
    LibCapability::value("Set", RequiredLib::Es2015),
    LibCapability::value("Promise", RequiredLib::Es2015),
    LibCapability::value("Symbol", RequiredLib::Es2015),
    LibCapability::value("WeakMap", RequiredLib::Es2015),
    LibCapability::value("WeakSet", RequiredLib::Es2015),
    LibCapability::value("Iterator", RequiredLib::Es2015),
    LibCapability::value("AsyncIterator", RequiredLib::Es2015),
    LibCapability::value("SharedArrayBuffer", RequiredLib::Es2017),
    LibCapability::value("Atomics", RequiredLib::Es2017),
    LibCapability::value("AsyncIterable", RequiredLib::Es2015),
    LibCapability::value("AsyncIterableIterator", RequiredLib::Es2015),
    LibCapability::value("AsyncGenerator", RequiredLib::Es2018),
    LibCapability::value("AsyncGeneratorFunction", RequiredLib::Es2018),
    LibCapability::value("BigInt", RequiredLib::Es2020),
    LibCapability::value("Reflect", RequiredLib::Es2015),
    LibCapability::value("BigInt64Array", RequiredLib::Es2020),
    LibCapability::value("BigUint64Array", RequiredLib::Es2020),
];

/// Return the table entry for a symbol/kind pair.
#[must_use]
pub fn capability_for(symbol: &str, kind: LibCapabilityKind) -> Option<&'static LibCapability> {
    LIB_CAPABILITIES
        .iter()
        .find(|entry| entry.symbol == symbol && entry.kind == kind)
}

/// Whether a symbol is a known ES lib type-position capability.
#[must_use]
pub fn is_known_es_type(symbol: &str) -> bool {
    capability_for(symbol, LibCapabilityKind::Type).is_some()
}

/// Whether a symbol is in tsc's narrow value-position TS2583 suggestion set.
#[must_use]
pub fn is_known_value_lib_suggestion(symbol: &str) -> bool {
    capability_for(symbol, LibCapabilityKind::Value).is_some()
}

/// Return the suggested lib for a known type-position capability.
#[must_use]
pub fn suggested_lib_for_type(symbol: &str) -> Option<RequiredLib> {
    capability_for(symbol, LibCapabilityKind::Type).map(|entry| entry.required_lib)
}

/// Return baseline globals expected from non-DOM libs during binder validation.
pub fn baseline_global_symbols() -> impl Iterator<Item = &'static str> {
    LIB_CAPABILITIES
        .iter()
        .filter(|entry| {
            entry.kind == LibCapabilityKind::Global && entry.required_lib != RequiredLib::Dom
        })
        .map(|entry| entry.symbol)
}

/// Return DOM-only globals tracked separately from baseline validation.
pub fn dom_global_symbols() -> impl Iterator<Item = &'static str> {
    LIB_CAPABILITIES
        .iter()
        .filter(|entry| {
            entry.kind == LibCapabilityKind::Global && entry.required_lib == RequiredLib::Dom
        })
        .map(|entry| entry.symbol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_validation_excludes_dom_globals() {
        let baseline: Vec<_> = baseline_global_symbols().collect();
        assert!(baseline.contains(&"Object"));
        assert!(baseline.contains(&"Promise"));
        assert!(!baseline.contains(&"console"));

        let dom: Vec<_> = dom_global_symbols().collect();
        assert_eq!(dom, vec!["console"]);
    }

    #[test]
    fn type_and_value_queries_share_the_same_table() {
        assert!(is_known_es_type("Promise"));
        assert!(is_known_es_type("AsyncGenerator"));
        assert!(!is_known_es_type("PromiseLike"));

        assert!(is_known_value_lib_suggestion("Promise"));
        assert!(is_known_value_lib_suggestion("Reflect"));
        assert!(!is_known_value_lib_suggestion("Proxy"));
    }

    #[test]
    fn suggested_libs_come_from_capabilities() {
        assert_eq!(
            suggested_lib_for_type("Promise").map(RequiredLib::as_str),
            Some("es2015")
        );
        assert_eq!(
            suggested_lib_for_type("SharedArrayBuffer").map(RequiredLib::as_str),
            Some("es2017")
        );
        assert_eq!(
            suggested_lib_for_type("AsyncGenerator").map(RequiredLib::as_str),
            Some("es2018")
        );
        assert_eq!(
            suggested_lib_for_type("BigInt").map(RequiredLib::as_str),
            Some("es2020")
        );
        assert_eq!(
            suggested_lib_for_type("WeakRef").map(RequiredLib::as_str),
            Some("es2021")
        );
        assert_eq!(
            suggested_lib_for_type("Disposable").map(RequiredLib::as_str),
            Some("esnext")
        );
        assert_eq!(suggested_lib_for_type("UnknownType"), None);
    }
}
