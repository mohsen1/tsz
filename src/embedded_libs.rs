//! Embedded TypeScript Library Files
//!
//! This module embeds the official TypeScript library definition files directly into
//! the binary using `include_str!`. This allows tsz to work without requiring
//! separate lib file installation.
//!
//! The lib files are sourced from the TypeScript npm package, versioned via
//! `conformance/typescript-versions.json`.
//!
//! # Build Requirements
//!
//! Before building, ensure lib assets are generated:
//! ```bash
//! node scripts/generate-lib-assets.mjs
//! ```
//!
//! The script will:
//! - Install the correct TypeScript version from npm (in conformance/node_modules)
//! - Copy lib.*.d.ts files to src/lib-assets/
//! - Generate lib_manifest.json with dependency information
//!
//! Note: src/lib-assets/ is NOT committed to the repo. It's generated at build time.
//!
//! # Performance Note
//!
//! For faster startup, use the pre-parsed lib system in [`crate::preparsed_libs`]:
//! 1. Run `tsz --generate-lib-cache` to create pre-parsed binary data
//! 2. Rebuild with `--features preparsed_libs` to embed the pre-parsed data
//!
//! The pre-parsed approach is ~10x faster than parsing raw text at runtime.
//!
//! # Usage
//!
//! ```rust
//! use wasm::embedded_libs::{get_lib, get_default_libs_for_target, EmbeddedLib};
//!
//! // Get a specific lib file
//! if let Some(lib) = get_lib("es5") {
//!     println!("ES5 lib has {} bytes", lib.content.len());
//! }
//!
//! // Get all libs needed for a target
//! let libs = get_default_libs_for_target(ScriptTarget::ES2020);
//! for lib in libs {
//!     println!("Loading: {}", lib.name);
//! }
//! ```

use crate::common::ScriptTarget;
use std::collections::HashSet;

/// An embedded TypeScript library file.
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedLib {
    /// The lib name (e.g., "es5", "es2015.promise", "dom")
    pub name: &'static str,
    /// The file name (e.g., "lib.es5.d.ts")
    pub file_name: &'static str,
    /// The file content
    pub content: &'static str,
    /// Referenced libs (from /// <reference lib="..." />)
    pub references: &'static [&'static str],
}

// =============================================================================
// ES5 Base Library
// =============================================================================

pub const LIB_ES5: EmbeddedLib = EmbeddedLib {
    name: "es5",
    file_name: "lib.es5.d.ts",
    content: include_str!("lib-assets/es5.d.ts"),
    references: &["decorators", "decorators.legacy"],
};

pub const LIB_ES5_FULL: EmbeddedLib = EmbeddedLib {
    name: "es5.full",
    file_name: "lib.es5.full.d.ts",
    content: include_str!("lib-assets/es5.full.d.ts"),
    references: &["es5", "dom", "webworker.importscripts", "scripthost"],
};

pub const LIB_DECORATORS: EmbeddedLib = EmbeddedLib {
    name: "decorators",
    file_name: "lib.decorators.d.ts",
    content: include_str!("lib-assets/decorators.d.ts"),
    references: &[],
};

pub const LIB_DECORATORS_LEGACY: EmbeddedLib = EmbeddedLib {
    name: "decorators.legacy",
    file_name: "lib.decorators.legacy.d.ts",
    content: include_str!("lib-assets/decorators.legacy.d.ts"),
    references: &[],
};

// =============================================================================
// ES2015 (ES6) Libraries
// =============================================================================

pub const LIB_ES2015: EmbeddedLib = EmbeddedLib {
    name: "es2015",
    file_name: "lib.es2015.d.ts",
    content: include_str!("lib-assets/es2015.d.ts"),
    references: &[
        "es5",
        "es2015.core",
        "es2015.collection",
        "es2015.iterable",
        "es2015.generator",
        "es2015.promise",
        "es2015.proxy",
        "es2015.reflect",
        "es2015.symbol",
        "es2015.symbol.wellknown",
    ],
};

pub const LIB_ES2015_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2015.full",
    file_name: "lib.es2015.full.d.ts",
    content: include_str!("lib-assets/es2015.full.d.ts"),
    references: &[
        "es2015",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
    ],
};

pub const LIB_ES2015_CORE: EmbeddedLib = EmbeddedLib {
    name: "es2015.core",
    file_name: "lib.es2015.core.d.ts",
    content: include_str!("lib-assets/es2015.core.d.ts"),
    references: &[],
};

pub const LIB_ES2015_COLLECTION: EmbeddedLib = EmbeddedLib {
    name: "es2015.collection",
    file_name: "lib.es2015.collection.d.ts",
    content: include_str!("lib-assets/es2015.collection.d.ts"),
    references: &[],
};

pub const LIB_ES2015_GENERATOR: EmbeddedLib = EmbeddedLib {
    name: "es2015.generator",
    file_name: "lib.es2015.generator.d.ts",
    content: include_str!("lib-assets/es2015.generator.d.ts"),
    references: &["es2015.iterable"],
};

pub const LIB_ES2015_ITERABLE: EmbeddedLib = EmbeddedLib {
    name: "es2015.iterable",
    file_name: "lib.es2015.iterable.d.ts",
    content: include_str!("lib-assets/es2015.iterable.d.ts"),
    references: &["es2015.symbol"],
};

pub const LIB_ES2015_PROMISE: EmbeddedLib = EmbeddedLib {
    name: "es2015.promise",
    file_name: "lib.es2015.promise.d.ts",
    content: include_str!("lib-assets/es2015.promise.d.ts"),
    references: &[],
};

pub const LIB_ES2015_PROXY: EmbeddedLib = EmbeddedLib {
    name: "es2015.proxy",
    file_name: "lib.es2015.proxy.d.ts",
    content: include_str!("lib-assets/es2015.proxy.d.ts"),
    references: &[],
};

pub const LIB_ES2015_REFLECT: EmbeddedLib = EmbeddedLib {
    name: "es2015.reflect",
    file_name: "lib.es2015.reflect.d.ts",
    content: include_str!("lib-assets/es2015.reflect.d.ts"),
    references: &[],
};

pub const LIB_ES2015_SYMBOL: EmbeddedLib = EmbeddedLib {
    name: "es2015.symbol",
    file_name: "lib.es2015.symbol.d.ts",
    content: include_str!("lib-assets/es2015.symbol.d.ts"),
    references: &[],
};

pub const LIB_ES2015_SYMBOL_WELLKNOWN: EmbeddedLib = EmbeddedLib {
    name: "es2015.symbol.wellknown",
    file_name: "lib.es2015.symbol.wellknown.d.ts",
    content: include_str!("lib-assets/es2015.symbol.wellknown.d.ts"),
    references: &["es2015.symbol"],
};

// =============================================================================
// ES2016 Libraries
// =============================================================================

pub const LIB_ES2016: EmbeddedLib = EmbeddedLib {
    name: "es2016",
    file_name: "lib.es2016.d.ts",
    content: include_str!("lib-assets/es2016.d.ts"),
    references: &["es2015", "es2016.array.include", "es2016.intl"],
};

pub const LIB_ES2016_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2016.full",
    file_name: "lib.es2016.full.d.ts",
    content: include_str!("lib-assets/es2016.full.d.ts"),
    references: &[
        "es2016",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
    ],
};

pub const LIB_ES2016_ARRAY_INCLUDE: EmbeddedLib = EmbeddedLib {
    name: "es2016.array.include",
    file_name: "lib.es2016.array.include.d.ts",
    content: include_str!("lib-assets/es2016.array.include.d.ts"),
    references: &[],
};

pub const LIB_ES2016_INTL: EmbeddedLib = EmbeddedLib {
    name: "es2016.intl",
    file_name: "lib.es2016.intl.d.ts",
    content: include_str!("lib-assets/es2016.intl.d.ts"),
    references: &[],
};

// =============================================================================
// ES2017 Libraries
// =============================================================================

pub const LIB_ES2017: EmbeddedLib = EmbeddedLib {
    name: "es2017",
    file_name: "lib.es2017.d.ts",
    content: include_str!("lib-assets/es2017.d.ts"),
    references: &[
        "es2016",
        "es2017.arraybuffer",
        "es2017.date",
        "es2017.object",
        "es2017.sharedmemory",
        "es2017.string",
        "es2017.intl",
        "es2017.typedarrays",
    ],
};

pub const LIB_ES2017_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2017.full",
    file_name: "lib.es2017.full.d.ts",
    content: include_str!("lib-assets/es2017.full.d.ts"),
    references: &[
        "es2017",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
    ],
};

pub const LIB_ES2017_ARRAYBUFFER: EmbeddedLib = EmbeddedLib {
    name: "es2017.arraybuffer",
    file_name: "lib.es2017.arraybuffer.d.ts",
    content: include_str!("lib-assets/es2017.arraybuffer.d.ts"),
    references: &[],
};

pub const LIB_ES2017_DATE: EmbeddedLib = EmbeddedLib {
    name: "es2017.date",
    file_name: "lib.es2017.date.d.ts",
    content: include_str!("lib-assets/es2017.date.d.ts"),
    references: &[],
};

pub const LIB_ES2017_INTL: EmbeddedLib = EmbeddedLib {
    name: "es2017.intl",
    file_name: "lib.es2017.intl.d.ts",
    content: include_str!("lib-assets/es2017.intl.d.ts"),
    references: &[],
};

pub const LIB_ES2017_OBJECT: EmbeddedLib = EmbeddedLib {
    name: "es2017.object",
    file_name: "lib.es2017.object.d.ts",
    content: include_str!("lib-assets/es2017.object.d.ts"),
    references: &[],
};

pub const LIB_ES2017_SHAREDMEMORY: EmbeddedLib = EmbeddedLib {
    name: "es2017.sharedmemory",
    file_name: "lib.es2017.sharedmemory.d.ts",
    content: include_str!("lib-assets/es2017.sharedmemory.d.ts"),
    references: &["es2015.symbol", "es2015.symbol.wellknown"],
};

pub const LIB_ES2017_STRING: EmbeddedLib = EmbeddedLib {
    name: "es2017.string",
    file_name: "lib.es2017.string.d.ts",
    content: include_str!("lib-assets/es2017.string.d.ts"),
    references: &[],
};

pub const LIB_ES2017_TYPEDARRAYS: EmbeddedLib = EmbeddedLib {
    name: "es2017.typedarrays",
    file_name: "lib.es2017.typedarrays.d.ts",
    content: include_str!("lib-assets/es2017.typedarrays.d.ts"),
    references: &[],
};

// =============================================================================
// ES2018 Libraries
// =============================================================================

pub const LIB_ES2018: EmbeddedLib = EmbeddedLib {
    name: "es2018",
    file_name: "lib.es2018.d.ts",
    content: include_str!("lib-assets/es2018.d.ts"),
    references: &[
        "es2017",
        "es2018.asynciterable",
        "es2018.asyncgenerator",
        "es2018.promise",
        "es2018.regexp",
        "es2018.intl",
    ],
};

pub const LIB_ES2018_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2018.full",
    file_name: "lib.es2018.full.d.ts",
    content: include_str!("lib-assets/es2018.full.d.ts"),
    references: &[
        "es2018",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
    ],
};

pub const LIB_ES2018_ASYNCGENERATOR: EmbeddedLib = EmbeddedLib {
    name: "es2018.asyncgenerator",
    file_name: "lib.es2018.asyncgenerator.d.ts",
    content: include_str!("lib-assets/es2018.asyncgenerator.d.ts"),
    references: &["es2018.asynciterable"],
};

pub const LIB_ES2018_ASYNCITERABLE: EmbeddedLib = EmbeddedLib {
    name: "es2018.asynciterable",
    file_name: "lib.es2018.asynciterable.d.ts",
    content: include_str!("lib-assets/es2018.asynciterable.d.ts"),
    references: &["es2015.symbol", "es2015.iterable"],
};

pub const LIB_ES2018_INTL: EmbeddedLib = EmbeddedLib {
    name: "es2018.intl",
    file_name: "lib.es2018.intl.d.ts",
    content: include_str!("lib-assets/es2018.intl.d.ts"),
    references: &[],
};

pub const LIB_ES2018_PROMISE: EmbeddedLib = EmbeddedLib {
    name: "es2018.promise",
    file_name: "lib.es2018.promise.d.ts",
    content: include_str!("lib-assets/es2018.promise.d.ts"),
    references: &[],
};

pub const LIB_ES2018_REGEXP: EmbeddedLib = EmbeddedLib {
    name: "es2018.regexp",
    file_name: "lib.es2018.regexp.d.ts",
    content: include_str!("lib-assets/es2018.regexp.d.ts"),
    references: &["es2015.symbol", "es2015.iterable"],
};

// =============================================================================
// ES2019 Libraries
// =============================================================================

pub const LIB_ES2019: EmbeddedLib = EmbeddedLib {
    name: "es2019",
    file_name: "lib.es2019.d.ts",
    content: include_str!("lib-assets/es2019.d.ts"),
    references: &[
        "es2018",
        "es2019.array",
        "es2019.object",
        "es2019.string",
        "es2019.symbol",
        "es2019.intl",
    ],
};

pub const LIB_ES2019_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2019.full",
    file_name: "lib.es2019.full.d.ts",
    content: include_str!("lib-assets/es2019.full.d.ts"),
    references: &[
        "es2019",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
    ],
};

pub const LIB_ES2019_ARRAY: EmbeddedLib = EmbeddedLib {
    name: "es2019.array",
    file_name: "lib.es2019.array.d.ts",
    content: include_str!("lib-assets/es2019.array.d.ts"),
    references: &[],
};

pub const LIB_ES2019_INTL: EmbeddedLib = EmbeddedLib {
    name: "es2019.intl",
    file_name: "lib.es2019.intl.d.ts",
    content: include_str!("lib-assets/es2019.intl.d.ts"),
    references: &[],
};

pub const LIB_ES2019_OBJECT: EmbeddedLib = EmbeddedLib {
    name: "es2019.object",
    file_name: "lib.es2019.object.d.ts",
    content: include_str!("lib-assets/es2019.object.d.ts"),
    references: &[],
};

pub const LIB_ES2019_STRING: EmbeddedLib = EmbeddedLib {
    name: "es2019.string",
    file_name: "lib.es2019.string.d.ts",
    content: include_str!("lib-assets/es2019.string.d.ts"),
    references: &[],
};

pub const LIB_ES2019_SYMBOL: EmbeddedLib = EmbeddedLib {
    name: "es2019.symbol",
    file_name: "lib.es2019.symbol.d.ts",
    content: include_str!("lib-assets/es2019.symbol.d.ts"),
    references: &[],
};

// =============================================================================
// ES2020 Libraries
// =============================================================================

pub const LIB_ES2020: EmbeddedLib = EmbeddedLib {
    name: "es2020",
    file_name: "lib.es2020.d.ts",
    content: include_str!("lib-assets/es2020.d.ts"),
    references: &[
        "es2019",
        "es2020.bigint",
        "es2020.date",
        "es2020.number",
        "es2020.promise",
        "es2020.sharedmemory",
        "es2020.string",
        "es2020.symbol.wellknown",
        "es2020.intl",
    ],
};

pub const LIB_ES2020_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2020.full",
    file_name: "lib.es2020.full.d.ts",
    content: include_str!("lib-assets/es2020.full.d.ts"),
    references: &[
        "es2020",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
    ],
};

pub const LIB_ES2020_BIGINT: EmbeddedLib = EmbeddedLib {
    name: "es2020.bigint",
    file_name: "lib.es2020.bigint.d.ts",
    content: include_str!("lib-assets/es2020.bigint.d.ts"),
    references: &["es2015.symbol", "es2015.symbol.wellknown", "es2015.iterable"],
};

pub const LIB_ES2020_DATE: EmbeddedLib = EmbeddedLib {
    name: "es2020.date",
    file_name: "lib.es2020.date.d.ts",
    content: include_str!("lib-assets/es2020.date.d.ts"),
    references: &[],
};

pub const LIB_ES2020_INTL: EmbeddedLib = EmbeddedLib {
    name: "es2020.intl",
    file_name: "lib.es2020.intl.d.ts",
    content: include_str!("lib-assets/es2020.intl.d.ts"),
    references: &["es2018.intl"],
};

pub const LIB_ES2020_NUMBER: EmbeddedLib = EmbeddedLib {
    name: "es2020.number",
    file_name: "lib.es2020.number.d.ts",
    content: include_str!("lib-assets/es2020.number.d.ts"),
    references: &[],
};

pub const LIB_ES2020_PROMISE: EmbeddedLib = EmbeddedLib {
    name: "es2020.promise",
    file_name: "lib.es2020.promise.d.ts",
    content: include_str!("lib-assets/es2020.promise.d.ts"),
    references: &[],
};

pub const LIB_ES2020_SHAREDMEMORY: EmbeddedLib = EmbeddedLib {
    name: "es2020.sharedmemory",
    file_name: "lib.es2020.sharedmemory.d.ts",
    content: include_str!("lib-assets/es2020.sharedmemory.d.ts"),
    references: &["es2015.symbol", "es2015.symbol.wellknown"],
};

pub const LIB_ES2020_STRING: EmbeddedLib = EmbeddedLib {
    name: "es2020.string",
    file_name: "lib.es2020.string.d.ts",
    content: include_str!("lib-assets/es2020.string.d.ts"),
    references: &["es2015.iterable"],
};

pub const LIB_ES2020_SYMBOL_WELLKNOWN: EmbeddedLib = EmbeddedLib {
    name: "es2020.symbol.wellknown",
    file_name: "lib.es2020.symbol.wellknown.d.ts",
    content: include_str!("lib-assets/es2020.symbol.wellknown.d.ts"),
    references: &[
        "es2015.iterable",
        "es2015.symbol",
        "es2018.asynciterable",
    ],
};

// =============================================================================
// ES2021 Libraries
// =============================================================================

pub const LIB_ES2021: EmbeddedLib = EmbeddedLib {
    name: "es2021",
    file_name: "lib.es2021.d.ts",
    content: include_str!("lib-assets/es2021.d.ts"),
    references: &[
        "es2020",
        "es2021.promise",
        "es2021.string",
        "es2021.weakref",
        "es2021.intl",
    ],
};

pub const LIB_ES2021_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2021.full",
    file_name: "lib.es2021.full.d.ts",
    content: include_str!("lib-assets/es2021.full.d.ts"),
    references: &[
        "es2021",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
    ],
};

pub const LIB_ES2021_INTL: EmbeddedLib = EmbeddedLib {
    name: "es2021.intl",
    file_name: "lib.es2021.intl.d.ts",
    content: include_str!("lib-assets/es2021.intl.d.ts"),
    references: &[],
};

pub const LIB_ES2021_PROMISE: EmbeddedLib = EmbeddedLib {
    name: "es2021.promise",
    file_name: "lib.es2021.promise.d.ts",
    content: include_str!("lib-assets/es2021.promise.d.ts"),
    references: &[],
};

pub const LIB_ES2021_STRING: EmbeddedLib = EmbeddedLib {
    name: "es2021.string",
    file_name: "lib.es2021.string.d.ts",
    content: include_str!("lib-assets/es2021.string.d.ts"),
    references: &[],
};

pub const LIB_ES2021_WEAKREF: EmbeddedLib = EmbeddedLib {
    name: "es2021.weakref",
    file_name: "lib.es2021.weakref.d.ts",
    content: include_str!("lib-assets/es2021.weakref.d.ts"),
    references: &[],
};

// =============================================================================
// ES2022 Libraries
// =============================================================================

pub const LIB_ES2022: EmbeddedLib = EmbeddedLib {
    name: "es2022",
    file_name: "lib.es2022.d.ts",
    content: include_str!("lib-assets/es2022.d.ts"),
    references: &[
        "es2021",
        "es2022.array",
        "es2022.error",
        "es2022.intl",
        "es2022.object",
        "es2022.string",
        "es2022.regexp",
    ],
};

pub const LIB_ES2022_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2022.full",
    file_name: "lib.es2022.full.d.ts",
    content: include_str!("lib-assets/es2022.full.d.ts"),
    references: &[
        "es2022",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
    ],
};

pub const LIB_ES2022_ARRAY: EmbeddedLib = EmbeddedLib {
    name: "es2022.array",
    file_name: "lib.es2022.array.d.ts",
    content: include_str!("lib-assets/es2022.array.d.ts"),
    references: &[],
};

pub const LIB_ES2022_ERROR: EmbeddedLib = EmbeddedLib {
    name: "es2022.error",
    file_name: "lib.es2022.error.d.ts",
    content: include_str!("lib-assets/es2022.error.d.ts"),
    references: &[],
};

pub const LIB_ES2022_INTL: EmbeddedLib = EmbeddedLib {
    name: "es2022.intl",
    file_name: "lib.es2022.intl.d.ts",
    content: include_str!("lib-assets/es2022.intl.d.ts"),
    references: &[],
};

pub const LIB_ES2022_OBJECT: EmbeddedLib = EmbeddedLib {
    name: "es2022.object",
    file_name: "lib.es2022.object.d.ts",
    content: include_str!("lib-assets/es2022.object.d.ts"),
    references: &[],
};

pub const LIB_ES2022_REGEXP: EmbeddedLib = EmbeddedLib {
    name: "es2022.regexp",
    file_name: "lib.es2022.regexp.d.ts",
    content: include_str!("lib-assets/es2022.regexp.d.ts"),
    references: &[],
};

pub const LIB_ES2022_STRING: EmbeddedLib = EmbeddedLib {
    name: "es2022.string",
    file_name: "lib.es2022.string.d.ts",
    content: include_str!("lib-assets/es2022.string.d.ts"),
    references: &[],
};

// =============================================================================
// ES2023 Libraries
// =============================================================================

pub const LIB_ES2023: EmbeddedLib = EmbeddedLib {
    name: "es2023",
    file_name: "lib.es2023.d.ts",
    content: include_str!("lib-assets/es2023.d.ts"),
    references: &[
        "es2022",
        "es2023.array",
        "es2023.collection",
        "es2023.intl",
    ],
};

pub const LIB_ES2023_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2023.full",
    file_name: "lib.es2023.full.d.ts",
    content: include_str!("lib-assets/es2023.full.d.ts"),
    references: &[
        "es2023",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
        "dom.asynciterable",
    ],
};

pub const LIB_ES2023_ARRAY: EmbeddedLib = EmbeddedLib {
    name: "es2023.array",
    file_name: "lib.es2023.array.d.ts",
    content: include_str!("lib-assets/es2023.array.d.ts"),
    references: &[],
};

pub const LIB_ES2023_COLLECTION: EmbeddedLib = EmbeddedLib {
    name: "es2023.collection",
    file_name: "lib.es2023.collection.d.ts",
    content: include_str!("lib-assets/es2023.collection.d.ts"),
    references: &[],
};

pub const LIB_ES2023_INTL: EmbeddedLib = EmbeddedLib {
    name: "es2023.intl",
    file_name: "lib.es2023.intl.d.ts",
    content: include_str!("lib-assets/es2023.intl.d.ts"),
    references: &[],
};

// =============================================================================
// ES2024 Libraries
// =============================================================================

pub const LIB_ES2024: EmbeddedLib = EmbeddedLib {
    name: "es2024",
    file_name: "lib.es2024.d.ts",
    content: include_str!("lib-assets/es2024.d.ts"),
    references: &[
        "es2023",
        "es2024.arraybuffer",
        "es2024.collection",
        "es2024.object",
        "es2024.promise",
        "es2024.regexp",
        "es2024.sharedmemory",
        "es2024.string",
    ],
};

pub const LIB_ES2024_FULL: EmbeddedLib = EmbeddedLib {
    name: "es2024.full",
    file_name: "lib.es2024.full.d.ts",
    content: include_str!("lib-assets/es2024.full.d.ts"),
    references: &[
        "es2024",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
        "dom.asynciterable",
    ],
};

pub const LIB_ES2024_ARRAYBUFFER: EmbeddedLib = EmbeddedLib {
    name: "es2024.arraybuffer",
    file_name: "lib.es2024.arraybuffer.d.ts",
    content: include_str!("lib-assets/es2024.arraybuffer.d.ts"),
    references: &[],
};

pub const LIB_ES2024_COLLECTION: EmbeddedLib = EmbeddedLib {
    name: "es2024.collection",
    file_name: "lib.es2024.collection.d.ts",
    content: include_str!("lib-assets/es2024.collection.d.ts"),
    references: &[],
};

pub const LIB_ES2024_OBJECT: EmbeddedLib = EmbeddedLib {
    name: "es2024.object",
    file_name: "lib.es2024.object.d.ts",
    content: include_str!("lib-assets/es2024.object.d.ts"),
    references: &[],
};

pub const LIB_ES2024_PROMISE: EmbeddedLib = EmbeddedLib {
    name: "es2024.promise",
    file_name: "lib.es2024.promise.d.ts",
    content: include_str!("lib-assets/es2024.promise.d.ts"),
    references: &[],
};

pub const LIB_ES2024_REGEXP: EmbeddedLib = EmbeddedLib {
    name: "es2024.regexp",
    file_name: "lib.es2024.regexp.d.ts",
    content: include_str!("lib-assets/es2024.regexp.d.ts"),
    references: &[],
};

pub const LIB_ES2024_SHAREDMEMORY: EmbeddedLib = EmbeddedLib {
    name: "es2024.sharedmemory",
    file_name: "lib.es2024.sharedmemory.d.ts",
    content: include_str!("lib-assets/es2024.sharedmemory.d.ts"),
    references: &["es2020.bigint"],
};

pub const LIB_ES2024_STRING: EmbeddedLib = EmbeddedLib {
    name: "es2024.string",
    file_name: "lib.es2024.string.d.ts",
    content: include_str!("lib-assets/es2024.string.d.ts"),
    references: &[],
};

// =============================================================================
// ESNext Libraries
// =============================================================================

pub const LIB_ESNEXT: EmbeddedLib = EmbeddedLib {
    name: "esnext",
    file_name: "lib.esnext.d.ts",
    content: include_str!("lib-assets/esnext.d.ts"),
    references: &[
        "es2024",
        "esnext.intl",
        "esnext.decorators",
        "esnext.disposable",
        "esnext.collection",
        "esnext.array",
        "esnext.iterator",
        "esnext.promise",
        "esnext.float16",
        "esnext.error",
        "esnext.sharedmemory",
        "esnext.typedarrays",
    ],
};

pub const LIB_ESNEXT_FULL: EmbeddedLib = EmbeddedLib {
    name: "esnext.full",
    file_name: "lib.esnext.full.d.ts",
    content: include_str!("lib-assets/esnext.full.d.ts"),
    references: &[
        "esnext",
        "dom",
        "dom.iterable",
        "webworker.importscripts",
        "scripthost",
        "dom.asynciterable",
    ],
};

pub const LIB_ESNEXT_ARRAY: EmbeddedLib = EmbeddedLib {
    name: "esnext.array",
    file_name: "lib.esnext.array.d.ts",
    content: include_str!("lib-assets/esnext.array.d.ts"),
    references: &[],
};

pub const LIB_ESNEXT_COLLECTION: EmbeddedLib = EmbeddedLib {
    name: "esnext.collection",
    file_name: "lib.esnext.collection.d.ts",
    content: include_str!("lib-assets/esnext.collection.d.ts"),
    references: &["es2024.collection"],
};

pub const LIB_ESNEXT_DECORATORS: EmbeddedLib = EmbeddedLib {
    name: "esnext.decorators",
    file_name: "lib.esnext.decorators.d.ts",
    content: include_str!("lib-assets/esnext.decorators.d.ts"),
    references: &["es2015.symbol", "decorators"],
};

pub const LIB_ESNEXT_DISPOSABLE: EmbeddedLib = EmbeddedLib {
    name: "esnext.disposable",
    file_name: "lib.esnext.disposable.d.ts",
    content: include_str!("lib-assets/esnext.disposable.d.ts"),
    references: &["es2015.symbol", "es2015.iterable", "es2018.asynciterable"],
};

pub const LIB_ESNEXT_ERROR: EmbeddedLib = EmbeddedLib {
    name: "esnext.error",
    file_name: "lib.esnext.error.d.ts",
    content: include_str!("lib-assets/esnext.error.d.ts"),
    references: &[],
};

pub const LIB_ESNEXT_FLOAT16: EmbeddedLib = EmbeddedLib {
    name: "esnext.float16",
    file_name: "lib.esnext.float16.d.ts",
    content: include_str!("lib-assets/esnext.float16.d.ts"),
    references: &["es2015.symbol", "es2015.iterable"],
};

pub const LIB_ESNEXT_INTL: EmbeddedLib = EmbeddedLib {
    name: "esnext.intl",
    file_name: "lib.esnext.intl.d.ts",
    content: include_str!("lib-assets/esnext.intl.d.ts"),
    references: &[],
};

pub const LIB_ESNEXT_ITERATOR: EmbeddedLib = EmbeddedLib {
    name: "esnext.iterator",
    file_name: "lib.esnext.iterator.d.ts",
    content: include_str!("lib-assets/esnext.iterator.d.ts"),
    references: &["es2015.iterable"],
};

pub const LIB_ESNEXT_PROMISE: EmbeddedLib = EmbeddedLib {
    name: "esnext.promise",
    file_name: "lib.esnext.promise.d.ts",
    content: include_str!("lib-assets/esnext.promise.d.ts"),
    references: &[],
};

pub const LIB_ESNEXT_SHAREDMEMORY: EmbeddedLib = EmbeddedLib {
    name: "esnext.sharedmemory",
    file_name: "lib.esnext.sharedmemory.d.ts",
    content: include_str!("lib-assets/esnext.sharedmemory.d.ts"),
    references: &[],
};

pub const LIB_ESNEXT_TYPEDARRAYS: EmbeddedLib = EmbeddedLib {
    name: "esnext.typedarrays",
    file_name: "lib.esnext.typedarrays.d.ts",
    content: include_str!("lib-assets/esnext.typedarrays.d.ts"),
    references: &[],
};

// =============================================================================
// DOM Libraries
// =============================================================================

pub const LIB_DOM: EmbeddedLib = EmbeddedLib {
    name: "dom",
    file_name: "lib.dom.d.ts",
    content: include_str!("lib-assets/dom.d.ts"),
    references: &["es2015", "es2018.asynciterable"],
};

pub const LIB_DOM_ITERABLE: EmbeddedLib = EmbeddedLib {
    name: "dom.iterable",
    file_name: "lib.dom.iterable.d.ts",
    content: include_str!("lib-assets/dom.iterable.d.ts"),
    references: &[],
};

pub const LIB_DOM_ASYNCITERABLE: EmbeddedLib = EmbeddedLib {
    name: "dom.asynciterable",
    file_name: "lib.dom.asynciterable.d.ts",
    content: include_str!("lib-assets/dom.asynciterable.d.ts"),
    references: &[],
};

// =============================================================================
// WebWorker Libraries
// =============================================================================

pub const LIB_WEBWORKER: EmbeddedLib = EmbeddedLib {
    name: "webworker",
    file_name: "lib.webworker.d.ts",
    content: include_str!("lib-assets/webworker.d.ts"),
    references: &["es2015", "es2018.asynciterable"],
};

pub const LIB_WEBWORKER_IMPORTSCRIPTS: EmbeddedLib = EmbeddedLib {
    name: "webworker.importscripts",
    file_name: "lib.webworker.importscripts.d.ts",
    content: include_str!("lib-assets/webworker.importscripts.d.ts"),
    references: &[],
};

pub const LIB_WEBWORKER_ITERABLE: EmbeddedLib = EmbeddedLib {
    name: "webworker.iterable",
    file_name: "lib.webworker.iterable.d.ts",
    content: include_str!("lib-assets/webworker.iterable.d.ts"),
    references: &[],
};

pub const LIB_WEBWORKER_ASYNCITERABLE: EmbeddedLib = EmbeddedLib {
    name: "webworker.asynciterable",
    file_name: "lib.webworker.asynciterable.d.ts",
    content: include_str!("lib-assets/webworker.asynciterable.d.ts"),
    references: &[],
};

// =============================================================================
// ScriptHost Library
// =============================================================================

pub const LIB_SCRIPTHOST: EmbeddedLib = EmbeddedLib {
    name: "scripthost",
    file_name: "lib.scripthost.d.ts",
    content: include_str!("lib-assets/scripthost.d.ts"),
    references: &[],
};

// =============================================================================
// Library Registry
// =============================================================================

/// All embedded libraries
pub static ALL_LIBS: &[EmbeddedLib] = &[
    // ES5
    LIB_ES5,
    LIB_ES5_FULL,
    LIB_DECORATORS,
    LIB_DECORATORS_LEGACY,
    // ES2015
    LIB_ES2015,
    LIB_ES2015_FULL,
    LIB_ES2015_CORE,
    LIB_ES2015_COLLECTION,
    LIB_ES2015_GENERATOR,
    LIB_ES2015_ITERABLE,
    LIB_ES2015_PROMISE,
    LIB_ES2015_PROXY,
    LIB_ES2015_REFLECT,
    LIB_ES2015_SYMBOL,
    LIB_ES2015_SYMBOL_WELLKNOWN,
    // ES2016
    LIB_ES2016,
    LIB_ES2016_FULL,
    LIB_ES2016_ARRAY_INCLUDE,
    LIB_ES2016_INTL,
    // ES2017
    LIB_ES2017,
    LIB_ES2017_FULL,
    LIB_ES2017_ARRAYBUFFER,
    LIB_ES2017_DATE,
    LIB_ES2017_INTL,
    LIB_ES2017_OBJECT,
    LIB_ES2017_SHAREDMEMORY,
    LIB_ES2017_STRING,
    LIB_ES2017_TYPEDARRAYS,
    // ES2018
    LIB_ES2018,
    LIB_ES2018_FULL,
    LIB_ES2018_ASYNCGENERATOR,
    LIB_ES2018_ASYNCITERABLE,
    LIB_ES2018_INTL,
    LIB_ES2018_PROMISE,
    LIB_ES2018_REGEXP,
    // ES2019
    LIB_ES2019,
    LIB_ES2019_FULL,
    LIB_ES2019_ARRAY,
    LIB_ES2019_INTL,
    LIB_ES2019_OBJECT,
    LIB_ES2019_STRING,
    LIB_ES2019_SYMBOL,
    // ES2020
    LIB_ES2020,
    LIB_ES2020_FULL,
    LIB_ES2020_BIGINT,
    LIB_ES2020_DATE,
    LIB_ES2020_INTL,
    LIB_ES2020_NUMBER,
    LIB_ES2020_PROMISE,
    LIB_ES2020_SHAREDMEMORY,
    LIB_ES2020_STRING,
    LIB_ES2020_SYMBOL_WELLKNOWN,
    // ES2021
    LIB_ES2021,
    LIB_ES2021_FULL,
    LIB_ES2021_INTL,
    LIB_ES2021_PROMISE,
    LIB_ES2021_STRING,
    LIB_ES2021_WEAKREF,
    // ES2022
    LIB_ES2022,
    LIB_ES2022_FULL,
    LIB_ES2022_ARRAY,
    LIB_ES2022_ERROR,
    LIB_ES2022_INTL,
    LIB_ES2022_OBJECT,
    LIB_ES2022_REGEXP,
    LIB_ES2022_STRING,
    // ES2023
    LIB_ES2023,
    LIB_ES2023_FULL,
    LIB_ES2023_ARRAY,
    LIB_ES2023_COLLECTION,
    LIB_ES2023_INTL,
    // ES2024
    LIB_ES2024,
    LIB_ES2024_FULL,
    LIB_ES2024_ARRAYBUFFER,
    LIB_ES2024_COLLECTION,
    LIB_ES2024_OBJECT,
    LIB_ES2024_PROMISE,
    LIB_ES2024_REGEXP,
    LIB_ES2024_SHAREDMEMORY,
    LIB_ES2024_STRING,
    // ESNext
    LIB_ESNEXT,
    LIB_ESNEXT_FULL,
    LIB_ESNEXT_ARRAY,
    LIB_ESNEXT_COLLECTION,
    LIB_ESNEXT_DECORATORS,
    LIB_ESNEXT_DISPOSABLE,
    LIB_ESNEXT_ERROR,
    LIB_ESNEXT_FLOAT16,
    LIB_ESNEXT_INTL,
    LIB_ESNEXT_ITERATOR,
    LIB_ESNEXT_PROMISE,
    LIB_ESNEXT_SHAREDMEMORY,
    LIB_ESNEXT_TYPEDARRAYS,
    // DOM
    LIB_DOM,
    LIB_DOM_ITERABLE,
    LIB_DOM_ASYNCITERABLE,
    // WebWorker
    LIB_WEBWORKER,
    LIB_WEBWORKER_IMPORTSCRIPTS,
    LIB_WEBWORKER_ITERABLE,
    LIB_WEBWORKER_ASYNCITERABLE,
    // ScriptHost
    LIB_SCRIPTHOST,
];

/// Get an embedded lib by name.
///
/// The name should match TypeScript's lib names (e.g., "es5", "es2015.promise", "dom").
pub fn get_lib(name: &str) -> Option<&'static EmbeddedLib> {
    ALL_LIBS.iter().find(|lib| lib.name == name)
}

/// Get an embedded lib by file name.
///
/// The file name should match the lib file name (e.g., "lib.es5.d.ts", "lib.dom.d.ts").
pub fn get_lib_by_file_name(file_name: &str) -> Option<&'static EmbeddedLib> {
    ALL_LIBS.iter().find(|lib| lib.file_name == file_name)
}

/// Get all embedded libs.
pub fn get_all_libs() -> &'static [EmbeddedLib] {
    ALL_LIBS
}

/// Resolve a lib and all its dependencies in dependency order.
///
/// Returns a vector of all embedded libs needed, with dependencies first.
pub fn resolve_lib_with_dependencies(name: &str) -> Vec<&'static EmbeddedLib> {
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    resolve_lib_recursive(name, &mut resolved, &mut seen);
    resolved
}

fn resolve_lib_recursive(
    name: &str,
    resolved: &mut Vec<&'static EmbeddedLib>,
    seen: &mut HashSet<String>,
) {
    if seen.contains(name) {
        return;
    }
    seen.insert(name.to_string());

    if let Some(lib) = get_lib(name) {
        // Resolve dependencies first (depth-first)
        for dep in lib.references {
            resolve_lib_recursive(dep, resolved, seen);
        }
        resolved.push(lib);
    }
}

/// Get the default libs for a given script target (without DOM).
///
/// Returns the libs needed for the specified ECMAScript version.
/// Does NOT include DOM or WebWorker libs.
pub fn get_libs_for_target(target: ScriptTarget) -> Vec<&'static EmbeddedLib> {
    let base_lib = match target {
        ScriptTarget::ES3 | ScriptTarget::ES5 => "es5",
        ScriptTarget::ES2015 => "es2015",
        ScriptTarget::ES2016 => "es2016",
        ScriptTarget::ES2017 => "es2017",
        ScriptTarget::ES2018 => "es2018",
        ScriptTarget::ES2019 => "es2019",
        ScriptTarget::ES2020 => "es2020",
        ScriptTarget::ES2021 => "es2021",
        ScriptTarget::ES2022 => "es2022",
        ScriptTarget::ESNext => "esnext",
    };
    resolve_lib_with_dependencies(base_lib)
}

/// Get the default libs for a given script target (with DOM).
///
/// Returns the libs needed for the specified ECMAScript version plus DOM and ScriptHost.
/// This matches tsc's default behavior when no explicit `lib` option is specified.
pub fn get_default_libs_for_target(target: ScriptTarget) -> Vec<&'static EmbeddedLib> {
    let mut libs = get_libs_for_target(target);

    // Add DOM libs (same as tsc default)
    if let Some(dom) = get_lib("dom") {
        libs.push(dom);
    }
    if let Some(dom_iterable) = get_lib("dom.iterable") {
        libs.push(dom_iterable);
    }
    if let Some(webworker_importscripts) = get_lib("webworker.importscripts") {
        libs.push(webworker_importscripts);
    }
    if let Some(scripthost) = get_lib("scripthost") {
        libs.push(scripthost);
    }

    libs
}

/// Parse `/// <reference lib="..." />` directives from lib content.
///
/// Returns a vector of referenced lib names.
pub fn parse_lib_references(content: &str) -> Vec<&str> {
    let mut refs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("/// <reference lib=") {
            // Parse: /// <reference lib="es5" />
            if let Some(start) = trimmed.find('"') {
                if let Some(end) = trimmed[start + 1..].find('"') {
                    refs.push(&trimmed[start + 1..start + 1 + end]);
                }
            }
        } else if !trimmed.starts_with("///") && !trimmed.is_empty() {
            // Stop at first non-reference line
            break;
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_lib() {
        let es5 = get_lib("es5").expect("es5 lib should exist");
        assert_eq!(es5.name, "es5");
        assert!(es5.content.contains("interface Object"));
    }

    #[test]
    fn test_get_lib_by_file_name() {
        let es5 = get_lib_by_file_name("lib.es5.d.ts").expect("lib.es5.d.ts should exist");
        assert_eq!(es5.name, "es5");
    }

    #[test]
    fn test_all_libs_count() {
        // We should have all the expected libs
        assert!(ALL_LIBS.len() >= 80, "Should have at least 80 libs");
    }

    #[test]
    fn test_parse_lib_references() {
        let content = r#"/// <reference lib="es5" />
/// <reference lib="es2015.promise" />
/// <reference lib="dom" />

interface Foo {}
"#;
        let refs = parse_lib_references(content);
        assert_eq!(refs, vec!["es5", "es2015.promise", "dom"]);
    }

    #[test]
    fn test_get_libs_for_target() {
        let es5_libs = get_libs_for_target(ScriptTarget::ES5);
        assert!(es5_libs.iter().any(|lib| lib.name == "es5"));

        let es2015_libs = get_libs_for_target(ScriptTarget::ES2015);
        assert!(es2015_libs.iter().any(|lib| lib.name == "es5"));
        assert!(es2015_libs.iter().any(|lib| lib.name == "es2015.promise"));
    }

    #[test]
    fn test_resolve_lib_with_dependencies() {
        let libs = resolve_lib_with_dependencies("es2015");
        // Should include es5 and all es2015 components
        let names: Vec<_> = libs.iter().map(|lib| lib.name).collect();
        assert!(names.contains(&"es5"));
        assert!(names.contains(&"es2015.promise"));
        assert!(names.contains(&"es2015.collection"));
    }

    #[test]
    fn test_dom_lib_has_window() {
        let dom = get_lib("dom").expect("dom lib should exist");
        assert!(dom.content.contains("interface Window"));
        assert!(dom.content.contains("declare var window"));
    }

    #[test]
    fn test_es5_has_core_types() {
        let es5 = get_lib("es5").expect("es5 lib should exist");
        assert!(es5.content.contains("interface Object"));
        assert!(es5.content.contains("interface Array<T>"));
        assert!(es5.content.contains("interface Function"));
        assert!(es5.content.contains("interface String"));
        assert!(es5.content.contains("interface Number"));
        assert!(es5.content.contains("interface Boolean"));
    }

    #[test]
    fn test_references_field() {
        // ES2015 should reference its component libs
        let es2015 = get_lib("es2015").expect("es2015 lib should exist");
        assert!(es2015.references.contains(&"es5"));
        assert!(es2015.references.contains(&"es2015.promise"));
        assert!(es2015.references.contains(&"es2015.collection"));

        // ES5 should reference decorators
        let es5 = get_lib("es5").expect("es5 lib should exist");
        assert!(es5.references.contains(&"decorators"));
    }
}
