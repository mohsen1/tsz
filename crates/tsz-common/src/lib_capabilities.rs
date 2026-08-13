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

/// Canonical processing order of the standard-lib `.d.ts` files, in the same
/// relative sequence `tsc` loads them (an `es5` base, then each `esNNNN`
/// version's atomic sublibs in `es2015.core`, `es2015.collection`,
/// `es2015.generator`, `es2015.iterable`, `…` order). Only the relative order
/// matters, and only for files that co-declare the same merged interface.
///
/// This is a display-ordering concern, not a semantic one: it lets a merged
/// standard-lib interface (`Map`/`Set`/`WeakMap`/`Promise`, split across several
/// lib files) present its members — in a missing-property diagnostic — in the
/// order `tsc`'s `getPropertiesOfType` enumerates them (declaration/file order,
/// with symbol-named members grouped last by the consumer). Files absent from
/// this list rank last, so a user `.ts` module or an unlisted lib keeps its
/// existing relative position under a stable sort. Kept in sync with the
/// `VALID_LIB_VALUES` catalog in `tsz-core`; `lib_file_order_is_monotonic`
/// pins the family invariants a drift would break.
const LIB_FILE_ORDER: &[&str] = &[
    "es5",
    "es2015.core",
    "es2015.collection",
    "es2015.generator",
    "es2015.iterable",
    "es2015.promise",
    "es2015.proxy",
    "es2015.reflect",
    "es2015.symbol",
    "es2015.symbol.wellknown",
    "es2016.array.include",
    "es2016.intl",
    "es2017.arraybuffer",
    "es2017.date",
    "es2017.object",
    "es2017.sharedmemory",
    "es2017.string",
    "es2017.intl",
    "es2017.typedarrays",
    "es2018.asyncgenerator",
    "es2018.asynciterable",
    "es2018.intl",
    "es2018.promise",
    "es2018.regexp",
    "es2019.array",
    "es2019.object",
    "es2019.string",
    "es2019.symbol",
    "es2019.intl",
    "es2020.bigint",
    "es2020.date",
    "es2020.promise",
    "es2020.sharedmemory",
    "es2020.string",
    "es2020.symbol.wellknown",
    "es2020.intl",
    "es2020.number",
    "es2021.promise",
    "es2021.string",
    "es2021.weakref",
    "es2021.intl",
    "es2022.array",
    "es2022.error",
    "es2022.intl",
    "es2022.object",
    "es2022.string",
    "es2022.regexp",
    "es2023.array",
    "es2023.collection",
    "es2023.intl",
    "es2024.arraybuffer",
    "es2024.collection",
    "es2024.object",
    "es2024.promise",
    "es2024.regexp",
    "es2024.sharedmemory",
    "es2024.string",
    "es2025.collection",
    "es2025.float16",
    "es2025.intl",
    "es2025.iterator",
    "es2025.promise",
    "es2025.regexp",
    "dom",
    "dom.iterable",
    "dom.asynciterable",
    "webworker",
    "webworker.iterable",
    "webworker.asynciterable",
    "scripthost",
];

/// Canonical ordering rank of a `lib.*.d.ts` file (see [`LIB_FILE_ORDER`]).
/// Accepts a path or a bare file name; reduces it to the catalog key
/// (`lib.es2015.collection.d.ts` → `es2015.collection`). Unlisted names —
/// including any user `.ts` file — rank `u32::MAX`.
#[must_use]
pub fn lib_file_order_rank(file_name: &str) -> u32 {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let key = base
        .strip_prefix("lib.")
        .unwrap_or(base)
        .strip_suffix(".d.ts")
        .unwrap_or(base);
    LIB_FILE_ORDER
        .iter()
        .position(|&candidate| candidate == key)
        .map_or(u32::MAX, |idx| idx as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lib_file_order_is_monotonic() {
        // The families that co-declare a merged standard-lib interface must be
        // ordered exactly as `tsc` processes their files.
        let rank = |name: &str| lib_file_order_rank(name);
        // Map/Set/WeakMap/ReadonlyMap.
        assert!(rank("lib.es2015.collection.d.ts") < rank("lib.es2015.iterable.d.ts"));
        assert!(rank("lib.es2015.iterable.d.ts") < rank("lib.es2015.symbol.wellknown.d.ts"));
        // Promise: the es2018 augment's string member (`finally`) still ranks by
        // file after the es2015 base, and both before the well-known-symbol file.
        assert!(rank("lib.es2015.promise.d.ts") < rank("lib.es2018.promise.d.ts"));
        // Array-family base precedes its es2015 iteration augment.
        assert!(rank("es5") < rank("es2015.iterable"));
        // Unlisted (user) files rank last.
        assert_eq!(rank("/proj/src/main.ts"), u32::MAX);
    }

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
