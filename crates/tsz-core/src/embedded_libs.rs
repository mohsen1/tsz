//! Embedded lib.d.ts file contents for zero-I/O startup.
//!
//! Generated automatically. Do not edit.
//! Contains all TypeScript lib declaration files as compile-time constants.
//! This eliminates file I/O during lib loading, making startup independent
//! of disk speed and system load.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Total number of embedded lib files.
pub const LIB_FILE_COUNT: usize = 103;

/// Get embedded lib file content by filename.
/// Returns None if the filename is not a known lib file.
pub fn get_lib_content(filename: &str) -> Option<&'static str> {
    EMBEDDED_LIBS.get(filename).copied()
}

/// Check if a filename matches an embedded lib file.
pub fn is_embedded_lib(filename: &str) -> bool {
    EMBEDDED_LIBS.contains_key(filename)
}

/// Get all embedded lib filenames.
pub fn all_lib_filenames() -> impl Iterator<Item = &'static str> {
    EMBEDDED_LIBS.keys().copied()
}

static EMBEDDED_LIBS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::with_capacity(103);
    m.insert(
        "decorators.d.ts",
        include_str!("lib-assets/decorators.d.ts"),
    );
    m.insert(
        "decorators.legacy.d.ts",
        include_str!("lib-assets/decorators.legacy.d.ts"),
    );
    m.insert(
        "dom.asynciterable.d.ts",
        include_str!("lib-assets/dom.asynciterable.d.ts"),
    );
    m.insert("dom.d.ts", include_str!("lib-assets/dom.d.ts"));
    m.insert(
        "dom.iterable.d.ts",
        include_str!("lib-assets/dom.iterable.d.ts"),
    );
    m.insert(
        "es2015.collection.d.ts",
        include_str!("lib-assets/es2015.collection.d.ts"),
    );
    m.insert(
        "es2015.core.d.ts",
        include_str!("lib-assets/es2015.core.d.ts"),
    );
    m.insert("es2015.d.ts", include_str!("lib-assets/es2015.d.ts"));
    m.insert(
        "es2015.generator.d.ts",
        include_str!("lib-assets/es2015.generator.d.ts"),
    );
    m.insert(
        "es2015.iterable.d.ts",
        include_str!("lib-assets/es2015.iterable.d.ts"),
    );
    m.insert(
        "es2015.promise.d.ts",
        include_str!("lib-assets/es2015.promise.d.ts"),
    );
    m.insert(
        "es2015.proxy.d.ts",
        include_str!("lib-assets/es2015.proxy.d.ts"),
    );
    m.insert(
        "es2015.reflect.d.ts",
        include_str!("lib-assets/es2015.reflect.d.ts"),
    );
    m.insert(
        "es2015.symbol.d.ts",
        include_str!("lib-assets/es2015.symbol.d.ts"),
    );
    m.insert(
        "es2015.symbol.wellknown.d.ts",
        include_str!("lib-assets/es2015.symbol.wellknown.d.ts"),
    );
    m.insert(
        "es2016.array.include.d.ts",
        include_str!("lib-assets/es2016.array.include.d.ts"),
    );
    m.insert("es2016.d.ts", include_str!("lib-assets/es2016.d.ts"));
    m.insert(
        "es2016.full.d.ts",
        include_str!("lib-assets/es2016.full.d.ts"),
    );
    m.insert(
        "es2016.intl.d.ts",
        include_str!("lib-assets/es2016.intl.d.ts"),
    );
    m.insert(
        "es2017.arraybuffer.d.ts",
        include_str!("lib-assets/es2017.arraybuffer.d.ts"),
    );
    m.insert("es2017.d.ts", include_str!("lib-assets/es2017.d.ts"));
    m.insert(
        "es2017.date.d.ts",
        include_str!("lib-assets/es2017.date.d.ts"),
    );
    m.insert(
        "es2017.full.d.ts",
        include_str!("lib-assets/es2017.full.d.ts"),
    );
    m.insert(
        "es2017.intl.d.ts",
        include_str!("lib-assets/es2017.intl.d.ts"),
    );
    m.insert(
        "es2017.object.d.ts",
        include_str!("lib-assets/es2017.object.d.ts"),
    );
    m.insert(
        "es2017.sharedmemory.d.ts",
        include_str!("lib-assets/es2017.sharedmemory.d.ts"),
    );
    m.insert(
        "es2017.string.d.ts",
        include_str!("lib-assets/es2017.string.d.ts"),
    );
    m.insert(
        "es2017.typedarrays.d.ts",
        include_str!("lib-assets/es2017.typedarrays.d.ts"),
    );
    m.insert(
        "es2018.asyncgenerator.d.ts",
        include_str!("lib-assets/es2018.asyncgenerator.d.ts"),
    );
    m.insert(
        "es2018.asynciterable.d.ts",
        include_str!("lib-assets/es2018.asynciterable.d.ts"),
    );
    m.insert("es2018.d.ts", include_str!("lib-assets/es2018.d.ts"));
    m.insert(
        "es2018.full.d.ts",
        include_str!("lib-assets/es2018.full.d.ts"),
    );
    m.insert(
        "es2018.intl.d.ts",
        include_str!("lib-assets/es2018.intl.d.ts"),
    );
    m.insert(
        "es2018.promise.d.ts",
        include_str!("lib-assets/es2018.promise.d.ts"),
    );
    m.insert(
        "es2018.regexp.d.ts",
        include_str!("lib-assets/es2018.regexp.d.ts"),
    );
    m.insert(
        "es2019.array.d.ts",
        include_str!("lib-assets/es2019.array.d.ts"),
    );
    m.insert("es2019.d.ts", include_str!("lib-assets/es2019.d.ts"));
    m.insert(
        "es2019.full.d.ts",
        include_str!("lib-assets/es2019.full.d.ts"),
    );
    m.insert(
        "es2019.intl.d.ts",
        include_str!("lib-assets/es2019.intl.d.ts"),
    );
    m.insert(
        "es2019.object.d.ts",
        include_str!("lib-assets/es2019.object.d.ts"),
    );
    m.insert(
        "es2019.string.d.ts",
        include_str!("lib-assets/es2019.string.d.ts"),
    );
    m.insert(
        "es2019.symbol.d.ts",
        include_str!("lib-assets/es2019.symbol.d.ts"),
    );
    m.insert(
        "es2020.bigint.d.ts",
        include_str!("lib-assets/es2020.bigint.d.ts"),
    );
    m.insert("es2020.d.ts", include_str!("lib-assets/es2020.d.ts"));
    m.insert(
        "es2020.date.d.ts",
        include_str!("lib-assets/es2020.date.d.ts"),
    );
    m.insert(
        "es2020.full.d.ts",
        include_str!("lib-assets/es2020.full.d.ts"),
    );
    m.insert(
        "es2020.intl.d.ts",
        include_str!("lib-assets/es2020.intl.d.ts"),
    );
    m.insert(
        "es2020.number.d.ts",
        include_str!("lib-assets/es2020.number.d.ts"),
    );
    m.insert(
        "es2020.promise.d.ts",
        include_str!("lib-assets/es2020.promise.d.ts"),
    );
    m.insert(
        "es2020.sharedmemory.d.ts",
        include_str!("lib-assets/es2020.sharedmemory.d.ts"),
    );
    m.insert(
        "es2020.string.d.ts",
        include_str!("lib-assets/es2020.string.d.ts"),
    );
    m.insert(
        "es2020.symbol.wellknown.d.ts",
        include_str!("lib-assets/es2020.symbol.wellknown.d.ts"),
    );
    m.insert("es2021.d.ts", include_str!("lib-assets/es2021.d.ts"));
    m.insert(
        "es2021.full.d.ts",
        include_str!("lib-assets/es2021.full.d.ts"),
    );
    m.insert(
        "es2021.intl.d.ts",
        include_str!("lib-assets/es2021.intl.d.ts"),
    );
    m.insert(
        "es2021.promise.d.ts",
        include_str!("lib-assets/es2021.promise.d.ts"),
    );
    m.insert(
        "es2021.string.d.ts",
        include_str!("lib-assets/es2021.string.d.ts"),
    );
    m.insert(
        "es2021.weakref.d.ts",
        include_str!("lib-assets/es2021.weakref.d.ts"),
    );
    m.insert(
        "es2022.array.d.ts",
        include_str!("lib-assets/es2022.array.d.ts"),
    );
    m.insert("es2022.d.ts", include_str!("lib-assets/es2022.d.ts"));
    m.insert(
        "es2022.error.d.ts",
        include_str!("lib-assets/es2022.error.d.ts"),
    );
    m.insert(
        "es2022.full.d.ts",
        include_str!("lib-assets/es2022.full.d.ts"),
    );
    m.insert(
        "es2022.intl.d.ts",
        include_str!("lib-assets/es2022.intl.d.ts"),
    );
    m.insert(
        "es2022.object.d.ts",
        include_str!("lib-assets/es2022.object.d.ts"),
    );
    m.insert(
        "es2022.regexp.d.ts",
        include_str!("lib-assets/es2022.regexp.d.ts"),
    );
    m.insert(
        "es2022.string.d.ts",
        include_str!("lib-assets/es2022.string.d.ts"),
    );
    m.insert(
        "es2023.array.d.ts",
        include_str!("lib-assets/es2023.array.d.ts"),
    );
    m.insert(
        "es2023.collection.d.ts",
        include_str!("lib-assets/es2023.collection.d.ts"),
    );
    m.insert("es2023.d.ts", include_str!("lib-assets/es2023.d.ts"));
    m.insert(
        "es2023.full.d.ts",
        include_str!("lib-assets/es2023.full.d.ts"),
    );
    m.insert(
        "es2023.intl.d.ts",
        include_str!("lib-assets/es2023.intl.d.ts"),
    );
    m.insert(
        "es2024.arraybuffer.d.ts",
        include_str!("lib-assets/es2024.arraybuffer.d.ts"),
    );
    m.insert(
        "es2024.collection.d.ts",
        include_str!("lib-assets/es2024.collection.d.ts"),
    );
    m.insert("es2024.d.ts", include_str!("lib-assets/es2024.d.ts"));
    m.insert(
        "es2024.full.d.ts",
        include_str!("lib-assets/es2024.full.d.ts"),
    );
    m.insert(
        "es2024.object.d.ts",
        include_str!("lib-assets/es2024.object.d.ts"),
    );
    m.insert(
        "es2024.promise.d.ts",
        include_str!("lib-assets/es2024.promise.d.ts"),
    );
    m.insert(
        "es2024.regexp.d.ts",
        include_str!("lib-assets/es2024.regexp.d.ts"),
    );
    m.insert(
        "es2024.sharedmemory.d.ts",
        include_str!("lib-assets/es2024.sharedmemory.d.ts"),
    );
    m.insert(
        "es2024.string.d.ts",
        include_str!("lib-assets/es2024.string.d.ts"),
    );
    m.insert("es5.d.ts", include_str!("lib-assets/es5.d.ts"));
    m.insert("es5.full.d.ts", include_str!("lib-assets/es5.full.d.ts"));
    m.insert("es6.d.ts", include_str!("lib-assets/es6.d.ts"));
    m.insert(
        "esnext.array.d.ts",
        include_str!("lib-assets/esnext.array.d.ts"),
    );
    m.insert(
        "esnext.collection.d.ts",
        include_str!("lib-assets/esnext.collection.d.ts"),
    );
    m.insert("esnext.d.ts", include_str!("lib-assets/esnext.d.ts"));
    m.insert(
        "esnext.decorators.d.ts",
        include_str!("lib-assets/esnext.decorators.d.ts"),
    );
    m.insert(
        "esnext.disposable.d.ts",
        include_str!("lib-assets/esnext.disposable.d.ts"),
    );
    m.insert(
        "esnext.error.d.ts",
        include_str!("lib-assets/esnext.error.d.ts"),
    );
    m.insert(
        "esnext.float16.d.ts",
        include_str!("lib-assets/esnext.float16.d.ts"),
    );
    m.insert(
        "esnext.full.d.ts",
        include_str!("lib-assets/esnext.full.d.ts"),
    );
    m.insert(
        "esnext.intl.d.ts",
        include_str!("lib-assets/esnext.intl.d.ts"),
    );
    m.insert(
        "esnext.iterator.d.ts",
        include_str!("lib-assets/esnext.iterator.d.ts"),
    );
    m.insert(
        "esnext.promise.d.ts",
        include_str!("lib-assets/esnext.promise.d.ts"),
    );
    m.insert(
        "esnext.sharedmemory.d.ts",
        include_str!("lib-assets/esnext.sharedmemory.d.ts"),
    );
    m.insert(
        "esnext.typedarrays.d.ts",
        include_str!("lib-assets/esnext.typedarrays.d.ts"),
    );
    m.insert(
        "scripthost.d.ts",
        include_str!("lib-assets/scripthost.d.ts"),
    );
    m.insert(
        "tsserverlibrary.d.ts",
        include_str!("lib-assets/tsserverlibrary.d.ts"),
    );
    m.insert(
        "typescript.d.ts",
        include_str!("lib-assets/typescript.d.ts"),
    );
    m.insert(
        "webworker.asynciterable.d.ts",
        include_str!("lib-assets/webworker.asynciterable.d.ts"),
    );
    m.insert("webworker.d.ts", include_str!("lib-assets/webworker.d.ts"));
    m.insert(
        "webworker.importscripts.d.ts",
        include_str!("lib-assets/webworker.importscripts.d.ts"),
    );
    m.insert(
        "webworker.iterable.d.ts",
        include_str!("lib-assets/webworker.iterable.d.ts"),
    );
    m
});
