//! Embedded lib.d.ts file contents for zero-I/O startup.
//!
//! Generated automatically from comment-stripped lib files.
//! Comments are removed at build time to reduce parse work by ~58%.
//!
//! Uses a match statement instead of a `HashMap` for zero-cost initialization
//! (no Lazy, no heap allocation, no `once_cell` synchronization).

pub const LIB_FILE_COUNT: usize = 107;

/// Look up embedded lib content by filename (e.g., "dom.d.ts", "es5.d.ts").
/// Returns None for unknown filenames.
#[inline]
#[allow(clippy::match_same_arms)]
pub fn get_lib_content(filename: &str) -> Option<&'static str> {
    match filename {
        "decorators.d.ts" => Some(include_str!("../lib-assets-stripped/decorators.d.ts")),
        "decorators.legacy.d.ts" => Some(include_str!(
            "../lib-assets-stripped/decorators.legacy.d.ts"
        )),
        "dom.asynciterable.d.ts" => Some(include_str!(
            "../lib-assets-stripped/dom.asynciterable.d.ts"
        )),
        "dom.d.ts" => Some(include_str!("../lib-assets-stripped/dom.d.ts")),
        "dom.iterable.d.ts" => Some(include_str!("../lib-assets-stripped/dom.iterable.d.ts")),
        "es2015.collection.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2015.collection.d.ts"
        )),
        "es2015.core.d.ts" => Some(include_str!("../lib-assets-stripped/es2015.core.d.ts")),
        "es2015.d.ts" => Some(include_str!("../lib-assets-stripped/es2015.d.ts")),
        "es2015.generator.d.ts" => {
            Some(include_str!("../lib-assets-stripped/es2015.generator.d.ts"))
        }
        "es2015.iterable.d.ts" => Some(include_str!("../lib-assets-stripped/es2015.iterable.d.ts")),
        "es2015.promise.d.ts" => Some(include_str!("../lib-assets-stripped/es2015.promise.d.ts")),
        "es2015.proxy.d.ts" => Some(include_str!("../lib-assets-stripped/es2015.proxy.d.ts")),
        "es2015.reflect.d.ts" => Some(include_str!("../lib-assets-stripped/es2015.reflect.d.ts")),
        "es2015.symbol.d.ts" => Some(include_str!("../lib-assets-stripped/es2015.symbol.d.ts")),
        "es2015.symbol.wellknown.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2015.symbol.wellknown.d.ts"
        )),
        "es2016.array.include.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2016.array.include.d.ts"
        )),
        "es2016.d.ts" => Some(include_str!("../lib-assets-stripped/es2016.d.ts")),
        "es2016.full.d.ts" => Some(include_str!("../lib-assets-stripped/es2016.full.d.ts")),
        "es2016.intl.d.ts" => Some(include_str!("../lib-assets-stripped/es2016.intl.d.ts")),
        "es2017.arraybuffer.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2017.arraybuffer.d.ts"
        )),
        "es2017.d.ts" => Some(include_str!("../lib-assets-stripped/es2017.d.ts")),
        "es2017.date.d.ts" => Some(include_str!("../lib-assets-stripped/es2017.date.d.ts")),
        "es2017.full.d.ts" => Some(include_str!("../lib-assets-stripped/es2017.full.d.ts")),
        "es2017.intl.d.ts" => Some(include_str!("../lib-assets-stripped/es2017.intl.d.ts")),
        "es2017.object.d.ts" => Some(include_str!("../lib-assets-stripped/es2017.object.d.ts")),
        "es2017.sharedmemory.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2017.sharedmemory.d.ts"
        )),
        "es2017.string.d.ts" => Some(include_str!("../lib-assets-stripped/es2017.string.d.ts")),
        "es2017.typedarrays.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2017.typedarrays.d.ts"
        )),
        "es2018.asyncgenerator.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2018.asyncgenerator.d.ts"
        )),
        "es2018.asynciterable.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2018.asynciterable.d.ts"
        )),
        "es2018.d.ts" => Some(include_str!("../lib-assets-stripped/es2018.d.ts")),
        "es2018.full.d.ts" => Some(include_str!("../lib-assets-stripped/es2018.full.d.ts")),
        "es2018.intl.d.ts" => Some(include_str!("../lib-assets-stripped/es2018.intl.d.ts")),
        "es2018.promise.d.ts" => Some(include_str!("../lib-assets-stripped/es2018.promise.d.ts")),
        "es2018.regexp.d.ts" => Some(include_str!("../lib-assets-stripped/es2018.regexp.d.ts")),
        "es2019.array.d.ts" => Some(include_str!("../lib-assets-stripped/es2019.array.d.ts")),
        "es2019.d.ts" => Some(include_str!("../lib-assets-stripped/es2019.d.ts")),
        "es2019.full.d.ts" => Some(include_str!("../lib-assets-stripped/es2019.full.d.ts")),
        "es2019.intl.d.ts" => Some(include_str!("../lib-assets-stripped/es2019.intl.d.ts")),
        "es2019.object.d.ts" => Some(include_str!("../lib-assets-stripped/es2019.object.d.ts")),
        "es2019.string.d.ts" => Some(include_str!("../lib-assets-stripped/es2019.string.d.ts")),
        "es2019.symbol.d.ts" => Some(include_str!("../lib-assets-stripped/es2019.symbol.d.ts")),
        "es2020.bigint.d.ts" => Some(include_str!("../lib-assets-stripped/es2020.bigint.d.ts")),
        "es2020.d.ts" => Some(include_str!("../lib-assets-stripped/es2020.d.ts")),
        "es2020.date.d.ts" => Some(include_str!("../lib-assets-stripped/es2020.date.d.ts")),
        "es2020.full.d.ts" => Some(include_str!("../lib-assets-stripped/es2020.full.d.ts")),
        "es2020.intl.d.ts" => Some(include_str!("../lib-assets-stripped/es2020.intl.d.ts")),
        "es2020.number.d.ts" => Some(include_str!("../lib-assets-stripped/es2020.number.d.ts")),
        "es2020.promise.d.ts" => Some(include_str!("../lib-assets-stripped/es2020.promise.d.ts")),
        "es2020.sharedmemory.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2020.sharedmemory.d.ts"
        )),
        "es2020.string.d.ts" => Some(include_str!("../lib-assets-stripped/es2020.string.d.ts")),
        "es2020.symbol.wellknown.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2020.symbol.wellknown.d.ts"
        )),
        "es2021.d.ts" => Some(include_str!("../lib-assets-stripped/es2021.d.ts")),
        "es2021.full.d.ts" => Some(include_str!("../lib-assets-stripped/es2021.full.d.ts")),
        "es2021.intl.d.ts" => Some(include_str!("../lib-assets-stripped/es2021.intl.d.ts")),
        "es2021.promise.d.ts" => Some(include_str!("../lib-assets-stripped/es2021.promise.d.ts")),
        "es2021.string.d.ts" => Some(include_str!("../lib-assets-stripped/es2021.string.d.ts")),
        "es2021.weakref.d.ts" => Some(include_str!("../lib-assets-stripped/es2021.weakref.d.ts")),
        "es2022.array.d.ts" => Some(include_str!("../lib-assets-stripped/es2022.array.d.ts")),
        "es2022.d.ts" => Some(include_str!("../lib-assets-stripped/es2022.d.ts")),
        "es2022.error.d.ts" => Some(include_str!("../lib-assets-stripped/es2022.error.d.ts")),
        "es2022.full.d.ts" => Some(include_str!("../lib-assets-stripped/es2022.full.d.ts")),
        "es2022.intl.d.ts" => Some(include_str!("../lib-assets-stripped/es2022.intl.d.ts")),
        "es2022.object.d.ts" => Some(include_str!("../lib-assets-stripped/es2022.object.d.ts")),
        "es2022.regexp.d.ts" => Some(include_str!("../lib-assets-stripped/es2022.regexp.d.ts")),
        "es2022.string.d.ts" => Some(include_str!("../lib-assets-stripped/es2022.string.d.ts")),
        "es2023.array.d.ts" => Some(include_str!("../lib-assets-stripped/es2023.array.d.ts")),
        "es2023.collection.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2023.collection.d.ts"
        )),
        "es2023.d.ts" => Some(include_str!("../lib-assets-stripped/es2023.d.ts")),
        "es2023.full.d.ts" => Some(include_str!("../lib-assets-stripped/es2023.full.d.ts")),
        "es2023.intl.d.ts" => Some(include_str!("../lib-assets-stripped/es2023.intl.d.ts")),
        "es2024.arraybuffer.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2024.arraybuffer.d.ts"
        )),
        "es2024.collection.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2024.collection.d.ts"
        )),
        "es2024.d.ts" => Some(include_str!("../lib-assets-stripped/es2024.d.ts")),
        "es2024.full.d.ts" => Some(include_str!("../lib-assets-stripped/es2024.full.d.ts")),
        "es2024.object.d.ts" => Some(include_str!("../lib-assets-stripped/es2024.object.d.ts")),
        "es2024.promise.d.ts" => Some(include_str!("../lib-assets-stripped/es2024.promise.d.ts")),
        "es2024.regexp.d.ts" => Some(include_str!("../lib-assets-stripped/es2024.regexp.d.ts")),
        "es2024.sharedmemory.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2024.sharedmemory.d.ts"
        )),
        "es2024.string.d.ts" => Some(include_str!("../lib-assets-stripped/es2024.string.d.ts")),
        "es2025.collection.d.ts" => Some(include_str!(
            "../lib-assets-stripped/es2025.collection.d.ts"
        )),
        "es2025.intl.d.ts" => Some(include_str!("../lib-assets-stripped/es2025.intl.d.ts")),
        "es5.d.ts" => Some(include_str!("../lib-assets-stripped/es5.d.ts")),
        "es5.full.d.ts" => Some(include_str!("../lib-assets-stripped/es5.full.d.ts")),
        "es6.d.ts" => Some(include_str!("../lib-assets-stripped/es6.d.ts")),
        "esnext.array.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.array.d.ts")),
        "esnext.date.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.date.d.ts")),
        "esnext.collection.d.ts" => Some(include_str!(
            "../lib-assets-stripped/esnext.collection.d.ts"
        )),
        "esnext.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.d.ts")),
        "esnext.decorators.d.ts" => Some(include_str!(
            "../lib-assets-stripped/esnext.decorators.d.ts"
        )),
        "esnext.disposable.d.ts" => Some(include_str!(
            "../lib-assets-stripped/esnext.disposable.d.ts"
        )),
        "esnext.error.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.error.d.ts")),
        "esnext.float16.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.float16.d.ts")),
        "esnext.full.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.full.d.ts")),
        "esnext.intl.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.intl.d.ts")),
        "esnext.iterator.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.iterator.d.ts")),
        "esnext.promise.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.promise.d.ts")),
        "esnext.sharedmemory.d.ts" => Some(include_str!(
            "../lib-assets-stripped/esnext.sharedmemory.d.ts"
        )),
        "esnext.temporal.d.ts" => Some(include_str!("../lib-assets-stripped/esnext.temporal.d.ts")),
        "esnext.typedarrays.d.ts" => Some(include_str!(
            "../lib-assets-stripped/esnext.typedarrays.d.ts"
        )),
        "scripthost.d.ts" => Some(include_str!("../lib-assets-stripped/scripthost.d.ts")),
        "tsserverlibrary.d.ts" => Some(include_str!("../lib-assets-stripped/tsserverlibrary.d.ts")),
        "typescript.d.ts" => Some(include_str!("../lib-assets-stripped/typescript.d.ts")),
        "webworker.asynciterable.d.ts" => Some(include_str!(
            "../lib-assets-stripped/webworker.asynciterable.d.ts"
        )),
        "webworker.d.ts" => Some(include_str!("../lib-assets-stripped/webworker.d.ts")),
        "webworker.importscripts.d.ts" => Some(include_str!(
            "../lib-assets-stripped/webworker.importscripts.d.ts"
        )),
        "webworker.iterable.d.ts" => Some(include_str!(
            "../lib-assets-stripped/webworker.iterable.d.ts"
        )),
        _ => None,
    }
}

const EMBEDDED_CONTENT_HASH_OFFSET: u64 = 0xcbf29ce484222325;
const EMBEDDED_CONTENT_HASH_PRIME: u64 = 0x100000001b3;

const fn embedded_content_hash(bytes: &[u8]) -> u64 {
    let mut hash = EMBEDDED_CONTENT_HASH_OFFSET;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(EMBEDDED_CONTENT_HASH_PRIME);
        i += 1;
    }
    hash
}

macro_rules! embedded_hash_arm {
    ($filename:literal) => {{
        #[allow(long_running_const_eval)]
        const HASH: u64 = embedded_content_hash(include_bytes!(concat!(
            "../lib-assets-stripped/",
            $filename
        )));
        Some(HASH)
    }};
}

/// Look up the compile-time content fingerprint for an embedded lib file.
#[inline]
#[allow(clippy::match_same_arms)]
pub fn get_lib_content_hash(filename: &str) -> Option<u64> {
    match filename {
        "decorators.d.ts" => embedded_hash_arm!("decorators.d.ts"),
        "decorators.legacy.d.ts" => embedded_hash_arm!("decorators.legacy.d.ts"),
        "dom.asynciterable.d.ts" => embedded_hash_arm!("dom.asynciterable.d.ts"),
        "dom.d.ts" => embedded_hash_arm!("dom.d.ts"),
        "dom.iterable.d.ts" => embedded_hash_arm!("dom.iterable.d.ts"),
        "es2015.collection.d.ts" => embedded_hash_arm!("es2015.collection.d.ts"),
        "es2015.core.d.ts" => embedded_hash_arm!("es2015.core.d.ts"),
        "es2015.d.ts" => embedded_hash_arm!("es2015.d.ts"),
        "es2015.generator.d.ts" => embedded_hash_arm!("es2015.generator.d.ts"),
        "es2015.iterable.d.ts" => embedded_hash_arm!("es2015.iterable.d.ts"),
        "es2015.promise.d.ts" => embedded_hash_arm!("es2015.promise.d.ts"),
        "es2015.proxy.d.ts" => embedded_hash_arm!("es2015.proxy.d.ts"),
        "es2015.reflect.d.ts" => embedded_hash_arm!("es2015.reflect.d.ts"),
        "es2015.symbol.d.ts" => embedded_hash_arm!("es2015.symbol.d.ts"),
        "es2015.symbol.wellknown.d.ts" => embedded_hash_arm!("es2015.symbol.wellknown.d.ts"),
        "es2016.array.include.d.ts" => embedded_hash_arm!("es2016.array.include.d.ts"),
        "es2016.d.ts" => embedded_hash_arm!("es2016.d.ts"),
        "es2016.full.d.ts" => embedded_hash_arm!("es2016.full.d.ts"),
        "es2016.intl.d.ts" => embedded_hash_arm!("es2016.intl.d.ts"),
        "es2017.arraybuffer.d.ts" => embedded_hash_arm!("es2017.arraybuffer.d.ts"),
        "es2017.d.ts" => embedded_hash_arm!("es2017.d.ts"),
        "es2017.date.d.ts" => embedded_hash_arm!("es2017.date.d.ts"),
        "es2017.full.d.ts" => embedded_hash_arm!("es2017.full.d.ts"),
        "es2017.intl.d.ts" => embedded_hash_arm!("es2017.intl.d.ts"),
        "es2017.object.d.ts" => embedded_hash_arm!("es2017.object.d.ts"),
        "es2017.sharedmemory.d.ts" => embedded_hash_arm!("es2017.sharedmemory.d.ts"),
        "es2017.string.d.ts" => embedded_hash_arm!("es2017.string.d.ts"),
        "es2017.typedarrays.d.ts" => embedded_hash_arm!("es2017.typedarrays.d.ts"),
        "es2018.asyncgenerator.d.ts" => embedded_hash_arm!("es2018.asyncgenerator.d.ts"),
        "es2018.asynciterable.d.ts" => embedded_hash_arm!("es2018.asynciterable.d.ts"),
        "es2018.d.ts" => embedded_hash_arm!("es2018.d.ts"),
        "es2018.full.d.ts" => embedded_hash_arm!("es2018.full.d.ts"),
        "es2018.intl.d.ts" => embedded_hash_arm!("es2018.intl.d.ts"),
        "es2018.promise.d.ts" => embedded_hash_arm!("es2018.promise.d.ts"),
        "es2018.regexp.d.ts" => embedded_hash_arm!("es2018.regexp.d.ts"),
        "es2019.array.d.ts" => embedded_hash_arm!("es2019.array.d.ts"),
        "es2019.d.ts" => embedded_hash_arm!("es2019.d.ts"),
        "es2019.full.d.ts" => embedded_hash_arm!("es2019.full.d.ts"),
        "es2019.intl.d.ts" => embedded_hash_arm!("es2019.intl.d.ts"),
        "es2019.object.d.ts" => embedded_hash_arm!("es2019.object.d.ts"),
        "es2019.string.d.ts" => embedded_hash_arm!("es2019.string.d.ts"),
        "es2019.symbol.d.ts" => embedded_hash_arm!("es2019.symbol.d.ts"),
        "es2020.bigint.d.ts" => embedded_hash_arm!("es2020.bigint.d.ts"),
        "es2020.d.ts" => embedded_hash_arm!("es2020.d.ts"),
        "es2020.date.d.ts" => embedded_hash_arm!("es2020.date.d.ts"),
        "es2020.full.d.ts" => embedded_hash_arm!("es2020.full.d.ts"),
        "es2020.intl.d.ts" => embedded_hash_arm!("es2020.intl.d.ts"),
        "es2020.number.d.ts" => embedded_hash_arm!("es2020.number.d.ts"),
        "es2020.promise.d.ts" => embedded_hash_arm!("es2020.promise.d.ts"),
        "es2020.sharedmemory.d.ts" => embedded_hash_arm!("es2020.sharedmemory.d.ts"),
        "es2020.string.d.ts" => embedded_hash_arm!("es2020.string.d.ts"),
        "es2020.symbol.wellknown.d.ts" => embedded_hash_arm!("es2020.symbol.wellknown.d.ts"),
        "es2021.d.ts" => embedded_hash_arm!("es2021.d.ts"),
        "es2021.full.d.ts" => embedded_hash_arm!("es2021.full.d.ts"),
        "es2021.intl.d.ts" => embedded_hash_arm!("es2021.intl.d.ts"),
        "es2021.promise.d.ts" => embedded_hash_arm!("es2021.promise.d.ts"),
        "es2021.string.d.ts" => embedded_hash_arm!("es2021.string.d.ts"),
        "es2021.weakref.d.ts" => embedded_hash_arm!("es2021.weakref.d.ts"),
        "es2022.array.d.ts" => embedded_hash_arm!("es2022.array.d.ts"),
        "es2022.d.ts" => embedded_hash_arm!("es2022.d.ts"),
        "es2022.error.d.ts" => embedded_hash_arm!("es2022.error.d.ts"),
        "es2022.full.d.ts" => embedded_hash_arm!("es2022.full.d.ts"),
        "es2022.intl.d.ts" => embedded_hash_arm!("es2022.intl.d.ts"),
        "es2022.object.d.ts" => embedded_hash_arm!("es2022.object.d.ts"),
        "es2022.regexp.d.ts" => embedded_hash_arm!("es2022.regexp.d.ts"),
        "es2022.string.d.ts" => embedded_hash_arm!("es2022.string.d.ts"),
        "es2023.array.d.ts" => embedded_hash_arm!("es2023.array.d.ts"),
        "es2023.collection.d.ts" => embedded_hash_arm!("es2023.collection.d.ts"),
        "es2023.d.ts" => embedded_hash_arm!("es2023.d.ts"),
        "es2023.full.d.ts" => embedded_hash_arm!("es2023.full.d.ts"),
        "es2023.intl.d.ts" => embedded_hash_arm!("es2023.intl.d.ts"),
        "es2024.arraybuffer.d.ts" => embedded_hash_arm!("es2024.arraybuffer.d.ts"),
        "es2024.collection.d.ts" => embedded_hash_arm!("es2024.collection.d.ts"),
        "es2024.d.ts" => embedded_hash_arm!("es2024.d.ts"),
        "es2024.full.d.ts" => embedded_hash_arm!("es2024.full.d.ts"),
        "es2024.object.d.ts" => embedded_hash_arm!("es2024.object.d.ts"),
        "es2024.promise.d.ts" => embedded_hash_arm!("es2024.promise.d.ts"),
        "es2024.regexp.d.ts" => embedded_hash_arm!("es2024.regexp.d.ts"),
        "es2024.sharedmemory.d.ts" => embedded_hash_arm!("es2024.sharedmemory.d.ts"),
        "es2024.string.d.ts" => embedded_hash_arm!("es2024.string.d.ts"),
        "es2025.collection.d.ts" => embedded_hash_arm!("es2025.collection.d.ts"),
        "es2025.intl.d.ts" => embedded_hash_arm!("es2025.intl.d.ts"),
        "es5.d.ts" => embedded_hash_arm!("es5.d.ts"),
        "es5.full.d.ts" => embedded_hash_arm!("es5.full.d.ts"),
        "es6.d.ts" => embedded_hash_arm!("es6.d.ts"),
        "esnext.array.d.ts" => embedded_hash_arm!("esnext.array.d.ts"),
        "esnext.collection.d.ts" => embedded_hash_arm!("esnext.collection.d.ts"),
        "esnext.d.ts" => embedded_hash_arm!("esnext.d.ts"),
        "esnext.date.d.ts" => embedded_hash_arm!("esnext.date.d.ts"),
        "esnext.decorators.d.ts" => embedded_hash_arm!("esnext.decorators.d.ts"),
        "esnext.disposable.d.ts" => embedded_hash_arm!("esnext.disposable.d.ts"),
        "esnext.error.d.ts" => embedded_hash_arm!("esnext.error.d.ts"),
        "esnext.float16.d.ts" => embedded_hash_arm!("esnext.float16.d.ts"),
        "esnext.full.d.ts" => embedded_hash_arm!("esnext.full.d.ts"),
        "esnext.intl.d.ts" => embedded_hash_arm!("esnext.intl.d.ts"),
        "esnext.iterator.d.ts" => embedded_hash_arm!("esnext.iterator.d.ts"),
        "esnext.promise.d.ts" => embedded_hash_arm!("esnext.promise.d.ts"),
        "esnext.sharedmemory.d.ts" => embedded_hash_arm!("esnext.sharedmemory.d.ts"),
        "esnext.temporal.d.ts" => embedded_hash_arm!("esnext.temporal.d.ts"),
        "esnext.typedarrays.d.ts" => embedded_hash_arm!("esnext.typedarrays.d.ts"),
        "scripthost.d.ts" => embedded_hash_arm!("scripthost.d.ts"),
        "tsserverlibrary.d.ts" => embedded_hash_arm!("tsserverlibrary.d.ts"),
        "typescript.d.ts" => embedded_hash_arm!("typescript.d.ts"),
        "webworker.asynciterable.d.ts" => embedded_hash_arm!("webworker.asynciterable.d.ts"),
        "webworker.d.ts" => embedded_hash_arm!("webworker.d.ts"),
        "webworker.importscripts.d.ts" => embedded_hash_arm!("webworker.importscripts.d.ts"),
        "webworker.iterable.d.ts" => embedded_hash_arm!("webworker.iterable.d.ts"),
        _ => None,
    }
}

/// Look up embedded `/// <reference lib=...>` entries by filename.
#[inline]
pub fn get_lib_references(filename: &str) -> Option<&'static [&'static str]> {
    is_embedded_lib(filename).then(|| get_embedded_lib_references(filename))
}

/// Look up embedded `/// <reference lib=...>` entries for a known embedded lib.
#[inline]
#[allow(clippy::match_same_arms)]
pub fn get_embedded_lib_references(filename: &str) -> &'static [&'static str] {
    match filename {
        "dom.d.ts" => &["es2015", "es2018.asynciterable"],
        "es2015.d.ts" => &[
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
        "es2015.generator.d.ts" => &["es2015.iterable"],
        "es2015.iterable.d.ts" => &["es2015.symbol"],
        "es2015.symbol.wellknown.d.ts" => &["es2015.symbol"],
        "es2016.d.ts" => &["es2015", "es2016.array.include", "es2016.intl"],
        "es2016.full.d.ts" => &[
            "es2016",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
        ],
        "es2017.d.ts" => &[
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.intl",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
        ],
        "es2017.full.d.ts" => &[
            "es2017",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
        ],
        "es2017.sharedmemory.d.ts" => &["es2015.symbol", "es2015.symbol.wellknown"],
        "es2018.asyncgenerator.d.ts" => &["es2018.asynciterable"],
        "es2018.asynciterable.d.ts" => &["es2015.symbol", "es2015.iterable"],
        "es2018.d.ts" => &[
            "es2017",
            "es2018.asynciterable",
            "es2018.asyncgenerator",
            "es2018.promise",
            "es2018.regexp",
            "es2018.intl",
        ],
        "es2018.full.d.ts" => &[
            "es2018",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
            "dom.asynciterable",
        ],
        "es2019.d.ts" => &[
            "es2018",
            "es2019.array",
            "es2019.object",
            "es2019.string",
            "es2019.symbol",
            "es2019.intl",
        ],
        "es2019.full.d.ts" => &[
            "es2019",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
            "dom.asynciterable",
        ],
        "es2019.object.d.ts" => &["es2015.iterable"],
        "es2020.bigint.d.ts" => &["es2020.intl"],
        "es2020.d.ts" => &[
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
        "es2020.date.d.ts" => &["es2020.intl"],
        "es2020.full.d.ts" => &[
            "es2020",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
            "dom.asynciterable",
        ],
        "es2020.intl.d.ts" => &["es2018.intl"],
        "es2020.number.d.ts" => &["es2020.intl"],
        "es2020.sharedmemory.d.ts" => &["es2020.bigint"],
        "es2020.string.d.ts" => &["es2015.iterable", "es2020.intl", "es2020.symbol.wellknown"],
        "es2020.symbol.wellknown.d.ts" => &["es2015.iterable", "es2015.symbol"],
        "es2021.d.ts" => &[
            "es2020",
            "es2021.promise",
            "es2021.string",
            "es2021.weakref",
            "es2021.intl",
        ],
        "es2021.full.d.ts" => &[
            "es2021",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
            "dom.asynciterable",
        ],
        "es2021.weakref.d.ts" => &["es2015.symbol.wellknown"],
        "es2022.d.ts" => &[
            "es2021",
            "es2022.array",
            "es2022.error",
            "es2022.intl",
            "es2022.object",
            "es2022.regexp",
            "es2022.string",
        ],
        "es2022.error.d.ts" => &["es2021.promise"],
        "es2022.full.d.ts" => &[
            "es2022",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
            "dom.asynciterable",
        ],
        "es2023.d.ts" => &["es2022", "es2023.array", "es2023.collection", "es2023.intl"],
        "es2023.full.d.ts" => &[
            "es2023",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
            "dom.asynciterable",
        ],
        "es2024.collection.d.ts" => &["es2023.collection"],
        "es2024.d.ts" => &[
            "es2023",
            "es2024.arraybuffer",
            "es2024.collection",
            "es2024.object",
            "es2024.promise",
            "es2024.regexp",
            "es2024.sharedmemory",
            "es2024.string",
        ],
        "es2024.full.d.ts" => &[
            "es2024",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
            "dom.asynciterable",
        ],
        "es2024.sharedmemory.d.ts" => &["es2020.bigint"],
        "es2025.collection.d.ts" => &["es2024.collection"],
        "es2025.intl.d.ts" => &["es2018.intl"],
        "es5.d.ts" => &["decorators", "decorators.legacy"],
        "es5.full.d.ts" => &["es5", "dom", "webworker.importscripts", "scripthost"],
        "es6.d.ts" => &[
            "es2015",
            "dom",
            "dom.iterable",
            "webworker.importscripts",
            "scripthost",
        ],
        "esnext.collection.d.ts" => &["es2025.collection"],
        "esnext.d.ts" => &[
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
        "esnext.date.d.ts" => &["esnext.temporal"],
        "esnext.decorators.d.ts" => &["es2015.symbol", "decorators"],
        "esnext.disposable.d.ts" => &["es2015.symbol", "es2015.iterable", "es2018.asynciterable"],
        "esnext.float16.d.ts" => &["es2015.symbol", "es2015.iterable"],
        "esnext.full.d.ts" => &[
            "esnext",
            "dom",
            "webworker.importscripts",
            "scripthost",
            "dom.iterable",
            "dom.asynciterable",
        ],
        "esnext.iterator.d.ts" => &["es2015.iterable"],
        "esnext.temporal.d.ts" => &["es2015.symbol.wellknown", "es2020.intl", "es2025.intl"],
        "webworker.d.ts" => &["es2015", "es2018.asynciterable"],
        _ => &[],
    }
}

/// Convert a `/// <reference lib=...>` value to its embedded lib filename.
pub(crate) fn embedded_reference_filename(lib_name: &str) -> String {
    match lib_name {
        "lib" | "lib.d.ts" => "es5.d.ts".to_string(),
        name if name.starts_with("lib.") && name.ends_with(".d.ts") => {
            format!("{}.d.ts", &name[4..name.len() - 5])
        }
        name if name.ends_with(".d.ts") => name.to_string(),
        name => format!("{name}.d.ts"),
    }
}

/// Check if a filename corresponds to an embedded lib file.
#[inline]
pub fn is_embedded_lib(filename: &str) -> bool {
    get_lib_content(filename).is_some()
}

/// All embedded lib filenames, sorted alphabetically.
pub fn all_lib_filenames() -> impl Iterator<Item = &'static str> {
    ALL_LIB_FILENAMES.iter().copied()
}

static ALL_LIB_FILENAMES: &[&str] = &[
    "decorators.d.ts",
    "decorators.legacy.d.ts",
    "dom.asynciterable.d.ts",
    "dom.d.ts",
    "dom.iterable.d.ts",
    "es2015.collection.d.ts",
    "es2015.core.d.ts",
    "es2015.d.ts",
    "es2015.generator.d.ts",
    "es2015.iterable.d.ts",
    "es2015.promise.d.ts",
    "es2015.proxy.d.ts",
    "es2015.reflect.d.ts",
    "es2015.symbol.d.ts",
    "es2015.symbol.wellknown.d.ts",
    "es2016.array.include.d.ts",
    "es2016.d.ts",
    "es2016.full.d.ts",
    "es2016.intl.d.ts",
    "es2017.arraybuffer.d.ts",
    "es2017.d.ts",
    "es2017.date.d.ts",
    "es2017.full.d.ts",
    "es2017.intl.d.ts",
    "es2017.object.d.ts",
    "es2017.sharedmemory.d.ts",
    "es2017.string.d.ts",
    "es2017.typedarrays.d.ts",
    "es2018.asyncgenerator.d.ts",
    "es2018.asynciterable.d.ts",
    "es2018.d.ts",
    "es2018.full.d.ts",
    "es2018.intl.d.ts",
    "es2018.promise.d.ts",
    "es2018.regexp.d.ts",
    "es2019.array.d.ts",
    "es2019.d.ts",
    "es2019.full.d.ts",
    "es2019.intl.d.ts",
    "es2019.object.d.ts",
    "es2019.string.d.ts",
    "es2019.symbol.d.ts",
    "es2020.bigint.d.ts",
    "es2020.d.ts",
    "es2020.date.d.ts",
    "es2020.full.d.ts",
    "es2020.intl.d.ts",
    "es2020.number.d.ts",
    "es2020.promise.d.ts",
    "es2020.sharedmemory.d.ts",
    "es2020.string.d.ts",
    "es2020.symbol.wellknown.d.ts",
    "es2021.d.ts",
    "es2021.full.d.ts",
    "es2021.intl.d.ts",
    "es2021.promise.d.ts",
    "es2021.string.d.ts",
    "es2021.weakref.d.ts",
    "es2022.array.d.ts",
    "es2022.d.ts",
    "es2022.error.d.ts",
    "es2022.full.d.ts",
    "es2022.intl.d.ts",
    "es2022.object.d.ts",
    "es2022.regexp.d.ts",
    "es2022.string.d.ts",
    "es2023.array.d.ts",
    "es2023.collection.d.ts",
    "es2023.d.ts",
    "es2023.full.d.ts",
    "es2023.intl.d.ts",
    "es2024.arraybuffer.d.ts",
    "es2024.collection.d.ts",
    "es2024.d.ts",
    "es2024.full.d.ts",
    "es2024.object.d.ts",
    "es2024.promise.d.ts",
    "es2024.regexp.d.ts",
    "es2024.sharedmemory.d.ts",
    "es2024.string.d.ts",
    "es2025.collection.d.ts",
    "es2025.intl.d.ts",
    "es5.d.ts",
    "es5.full.d.ts",
    "es6.d.ts",
    "esnext.array.d.ts",
    "esnext.collection.d.ts",
    "esnext.d.ts",
    "esnext.date.d.ts",
    "esnext.decorators.d.ts",
    "esnext.disposable.d.ts",
    "esnext.error.d.ts",
    "esnext.float16.d.ts",
    "esnext.full.d.ts",
    "esnext.intl.d.ts",
    "esnext.iterator.d.ts",
    "esnext.promise.d.ts",
    "esnext.sharedmemory.d.ts",
    "esnext.temporal.d.ts",
    "esnext.typedarrays.d.ts",
    "scripthost.d.ts",
    "tsserverlibrary.d.ts",
    "typescript.d.ts",
    "webworker.asynciterable.d.ts",
    "webworker.d.ts",
    "webworker.importscripts.d.ts",
    "webworker.iterable.d.ts",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_embedded_lib_lookup() {
        let content = get_lib_content("es5.d.ts").expect("es5.d.ts should be embedded");
        assert!(is_embedded_lib("es5.d.ts"));
        assert!(!content.is_empty());
    }

    #[test]
    fn test_unknown_embedded_lib_lookup() {
        assert!(get_lib_content("not-a-lib.d.ts").is_none());
        assert!(!is_embedded_lib("not-a-lib.d.ts"));
    }

    #[test]
    fn test_all_lib_filenames_count_matches_constant_and_are_unique() {
        let mut filenames: Vec<_> = all_lib_filenames().collect();
        filenames.sort_unstable();

        assert_eq!(filenames.len(), LIB_FILE_COUNT);

        let mut deduped = filenames.clone();
        deduped.dedup();
        assert_eq!(deduped, filenames);
    }

    #[test]
    fn test_is_embedded_lib_and_get_lib_content_align_for_all_entries() {
        let mut seen = 0;

        for filename in all_lib_filenames() {
            seen += 1;
            assert!(is_embedded_lib(filename), "{filename} should be recognized");

            let content = get_lib_content(filename).expect("embedded lib content missing");
            assert!(!content.is_empty(), "{filename} should have content");
            assert!(
                get_lib_content_hash(filename).is_some(),
                "{filename} should have a content hash"
            );
            assert!(
                get_lib_references(filename).is_some(),
                "{filename} should have a reference entry"
            );
        }

        assert_eq!(seen, LIB_FILE_COUNT);
    }

    #[test]
    fn test_embedded_lib_references_resolve_to_embedded_assets() {
        for filename in all_lib_filenames() {
            for ref_lib in get_embedded_lib_references(filename) {
                let embedded_name = embedded_reference_filename(ref_lib);
                assert!(
                    is_embedded_lib(&embedded_name),
                    "{filename} reference {ref_lib} should resolve to embedded {embedded_name}"
                );
            }
        }
    }

    #[test]
    fn test_esnext_date_lib_embedded() {
        let content =
            get_lib_content("esnext.date.d.ts").expect("esnext.date.d.ts should be embedded");
        assert!(is_embedded_lib("esnext.date.d.ts"));
        assert!(
            content.contains("toTemporalInstant"),
            "esnext.date.d.ts should define toTemporalInstant"
        );
        assert!(
            content.contains("Temporal.Instant"),
            "esnext.date.d.ts should reference Temporal.Instant"
        );
    }

    #[test]
    fn test_esnext_temporal_lib_embedded() {
        let content = get_lib_content("esnext.temporal.d.ts")
            .expect("esnext.temporal.d.ts should be embedded");
        assert!(is_embedded_lib("esnext.temporal.d.ts"));
        assert!(
            content.contains("namespace Temporal"),
            "esnext.temporal.d.ts should declare Temporal namespace"
        );
        assert!(
            content.contains("Instant"),
            "esnext.temporal.d.ts should define Temporal.Instant"
        );
        assert!(
            content.contains("PlainDate"),
            "esnext.temporal.d.ts should define Temporal.PlainDate"
        );
        assert!(
            content.contains("ZonedDateTime"),
            "esnext.temporal.d.ts should define Temporal.ZonedDateTime"
        );
    }

    #[test]
    fn test_es2021_intl_embedded_lib_is_self_contained_for_datetime_range_parts() {
        let content =
            get_lib_content("es2021.intl.d.ts").expect("es2021.intl.d.ts should be embedded");
        assert!(content.contains("interface DateTimeFormatPart"));
        assert!(content.contains("interface DateTimeRangeFormatPart extends DateTimeFormatPart"));
    }
}
