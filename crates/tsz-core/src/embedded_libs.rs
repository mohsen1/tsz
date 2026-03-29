//! Embedded lib.d.ts file contents for zero-I/O startup.
//!
//! Generated automatically from comment-stripped lib files.
//! Comments are removed at build time to reduce parse work by ~58%.
//!
//! Uses a match statement instead of a HashMap for zero-cost initialization
//! (no Lazy, no heap allocation, no once_cell synchronization).

pub const LIB_FILE_COUNT: usize = 103;

/// Look up embedded lib content by filename (e.g., "dom.d.ts", "es5.d.ts").
/// Returns None for unknown filenames.
#[inline]
pub fn get_lib_content(filename: &str) -> Option<&'static str> {
    match filename {
        "decorators.d.ts" => Some(include_str!("lib-assets-stripped/decorators.d.ts")),
        "decorators.legacy.d.ts" => {
            Some(include_str!("lib-assets-stripped/decorators.legacy.d.ts"))
        }
        "dom.asynciterable.d.ts" => {
            Some(include_str!("lib-assets-stripped/dom.asynciterable.d.ts"))
        }
        "dom.d.ts" => Some(include_str!("lib-assets-stripped/dom.d.ts")),
        "dom.iterable.d.ts" => Some(include_str!("lib-assets-stripped/dom.iterable.d.ts")),
        "es2015.collection.d.ts" => {
            Some(include_str!("lib-assets-stripped/es2015.collection.d.ts"))
        }
        "es2015.core.d.ts" => Some(include_str!("lib-assets-stripped/es2015.core.d.ts")),
        "es2015.d.ts" => Some(include_str!("lib-assets-stripped/es2015.d.ts")),
        "es2015.generator.d.ts" => Some(include_str!("lib-assets-stripped/es2015.generator.d.ts")),
        "es2015.iterable.d.ts" => Some(include_str!("lib-assets-stripped/es2015.iterable.d.ts")),
        "es2015.promise.d.ts" => Some(include_str!("lib-assets-stripped/es2015.promise.d.ts")),
        "es2015.proxy.d.ts" => Some(include_str!("lib-assets-stripped/es2015.proxy.d.ts")),
        "es2015.reflect.d.ts" => Some(include_str!("lib-assets-stripped/es2015.reflect.d.ts")),
        "es2015.symbol.d.ts" => Some(include_str!("lib-assets-stripped/es2015.symbol.d.ts")),
        "es2015.symbol.wellknown.d.ts" => Some(include_str!(
            "lib-assets-stripped/es2015.symbol.wellknown.d.ts"
        )),
        "es2016.array.include.d.ts" => Some(include_str!(
            "lib-assets-stripped/es2016.array.include.d.ts"
        )),
        "es2016.d.ts" => Some(include_str!("lib-assets-stripped/es2016.d.ts")),
        "es2016.full.d.ts" => Some(include_str!("lib-assets-stripped/es2016.full.d.ts")),
        "es2016.intl.d.ts" => Some(include_str!("lib-assets-stripped/es2016.intl.d.ts")),
        "es2017.arraybuffer.d.ts" => {
            Some(include_str!("lib-assets-stripped/es2017.arraybuffer.d.ts"))
        }
        "es2017.d.ts" => Some(include_str!("lib-assets-stripped/es2017.d.ts")),
        "es2017.date.d.ts" => Some(include_str!("lib-assets-stripped/es2017.date.d.ts")),
        "es2017.full.d.ts" => Some(include_str!("lib-assets-stripped/es2017.full.d.ts")),
        "es2017.intl.d.ts" => Some(include_str!("lib-assets-stripped/es2017.intl.d.ts")),
        "es2017.object.d.ts" => Some(include_str!("lib-assets-stripped/es2017.object.d.ts")),
        "es2017.sharedmemory.d.ts" => {
            Some(include_str!("lib-assets-stripped/es2017.sharedmemory.d.ts"))
        }
        "es2017.string.d.ts" => Some(include_str!("lib-assets-stripped/es2017.string.d.ts")),
        "es2017.typedarrays.d.ts" => {
            Some(include_str!("lib-assets-stripped/es2017.typedarrays.d.ts"))
        }
        "es2018.asyncgenerator.d.ts" => Some(include_str!(
            "lib-assets-stripped/es2018.asyncgenerator.d.ts"
        )),
        "es2018.asynciterable.d.ts" => Some(include_str!(
            "lib-assets-stripped/es2018.asynciterable.d.ts"
        )),
        "es2018.d.ts" => Some(include_str!("lib-assets-stripped/es2018.d.ts")),
        "es2018.full.d.ts" => Some(include_str!("lib-assets-stripped/es2018.full.d.ts")),
        "es2018.intl.d.ts" => Some(include_str!("lib-assets-stripped/es2018.intl.d.ts")),
        "es2018.promise.d.ts" => Some(include_str!("lib-assets-stripped/es2018.promise.d.ts")),
        "es2018.regexp.d.ts" => Some(include_str!("lib-assets-stripped/es2018.regexp.d.ts")),
        "es2019.array.d.ts" => Some(include_str!("lib-assets-stripped/es2019.array.d.ts")),
        "es2019.d.ts" => Some(include_str!("lib-assets-stripped/es2019.d.ts")),
        "es2019.full.d.ts" => Some(include_str!("lib-assets-stripped/es2019.full.d.ts")),
        "es2019.intl.d.ts" => Some(include_str!("lib-assets-stripped/es2019.intl.d.ts")),
        "es2019.object.d.ts" => Some(include_str!("lib-assets-stripped/es2019.object.d.ts")),
        "es2019.string.d.ts" => Some(include_str!("lib-assets-stripped/es2019.string.d.ts")),
        "es2019.symbol.d.ts" => Some(include_str!("lib-assets-stripped/es2019.symbol.d.ts")),
        "es2020.bigint.d.ts" => Some(include_str!("lib-assets-stripped/es2020.bigint.d.ts")),
        "es2020.d.ts" => Some(include_str!("lib-assets-stripped/es2020.d.ts")),
        "es2020.date.d.ts" => Some(include_str!("lib-assets-stripped/es2020.date.d.ts")),
        "es2020.full.d.ts" => Some(include_str!("lib-assets-stripped/es2020.full.d.ts")),
        "es2020.intl.d.ts" => Some(include_str!("lib-assets-stripped/es2020.intl.d.ts")),
        "es2020.number.d.ts" => Some(include_str!("lib-assets-stripped/es2020.number.d.ts")),
        "es2020.promise.d.ts" => Some(include_str!("lib-assets-stripped/es2020.promise.d.ts")),
        "es2020.sharedmemory.d.ts" => {
            Some(include_str!("lib-assets-stripped/es2020.sharedmemory.d.ts"))
        }
        "es2020.string.d.ts" => Some(include_str!("lib-assets-stripped/es2020.string.d.ts")),
        "es2020.symbol.wellknown.d.ts" => Some(include_str!(
            "lib-assets-stripped/es2020.symbol.wellknown.d.ts"
        )),
        "es2021.d.ts" => Some(include_str!("lib-assets-stripped/es2021.d.ts")),
        "es2021.full.d.ts" => Some(include_str!("lib-assets-stripped/es2021.full.d.ts")),
        "es2021.intl.d.ts" => Some(include_str!("lib-assets-stripped/es2021.intl.d.ts")),
        "es2021.promise.d.ts" => Some(include_str!("lib-assets-stripped/es2021.promise.d.ts")),
        "es2021.string.d.ts" => Some(include_str!("lib-assets-stripped/es2021.string.d.ts")),
        "es2021.weakref.d.ts" => Some(include_str!("lib-assets-stripped/es2021.weakref.d.ts")),
        "es2022.array.d.ts" => Some(include_str!("lib-assets-stripped/es2022.array.d.ts")),
        "es2022.d.ts" => Some(include_str!("lib-assets-stripped/es2022.d.ts")),
        "es2022.error.d.ts" => Some(include_str!("lib-assets-stripped/es2022.error.d.ts")),
        "es2022.full.d.ts" => Some(include_str!("lib-assets-stripped/es2022.full.d.ts")),
        "es2022.intl.d.ts" => Some(include_str!("lib-assets-stripped/es2022.intl.d.ts")),
        "es2022.object.d.ts" => Some(include_str!("lib-assets-stripped/es2022.object.d.ts")),
        "es2022.regexp.d.ts" => Some(include_str!("lib-assets-stripped/es2022.regexp.d.ts")),
        "es2022.string.d.ts" => Some(include_str!("lib-assets-stripped/es2022.string.d.ts")),
        "es2023.array.d.ts" => Some(include_str!("lib-assets-stripped/es2023.array.d.ts")),
        "es2023.collection.d.ts" => {
            Some(include_str!("lib-assets-stripped/es2023.collection.d.ts"))
        }
        "es2023.d.ts" => Some(include_str!("lib-assets-stripped/es2023.d.ts")),
        "es2023.full.d.ts" => Some(include_str!("lib-assets-stripped/es2023.full.d.ts")),
        "es2023.intl.d.ts" => Some(include_str!("lib-assets-stripped/es2023.intl.d.ts")),
        "es2024.arraybuffer.d.ts" => {
            Some(include_str!("lib-assets-stripped/es2024.arraybuffer.d.ts"))
        }
        "es2024.collection.d.ts" => {
            Some(include_str!("lib-assets-stripped/es2024.collection.d.ts"))
        }
        "es2024.d.ts" => Some(include_str!("lib-assets-stripped/es2024.d.ts")),
        "es2024.full.d.ts" => Some(include_str!("lib-assets-stripped/es2024.full.d.ts")),
        "es2024.object.d.ts" => Some(include_str!("lib-assets-stripped/es2024.object.d.ts")),
        "es2024.promise.d.ts" => Some(include_str!("lib-assets-stripped/es2024.promise.d.ts")),
        "es2024.regexp.d.ts" => Some(include_str!("lib-assets-stripped/es2024.regexp.d.ts")),
        "es2024.sharedmemory.d.ts" => {
            Some(include_str!("lib-assets-stripped/es2024.sharedmemory.d.ts"))
        }
        "es2024.string.d.ts" => Some(include_str!("lib-assets-stripped/es2024.string.d.ts")),
        "es5.d.ts" => Some(include_str!("lib-assets-stripped/es5.d.ts")),
        "es5.full.d.ts" => Some(include_str!("lib-assets-stripped/es5.full.d.ts")),
        "es6.d.ts" => Some(include_str!("lib-assets-stripped/es6.d.ts")),
        "esnext.array.d.ts" => Some(include_str!("lib-assets-stripped/esnext.array.d.ts")),
        "esnext.collection.d.ts" => {
            Some(include_str!("lib-assets-stripped/esnext.collection.d.ts"))
        }
        "esnext.d.ts" => Some(include_str!("lib-assets-stripped/esnext.d.ts")),
        "esnext.decorators.d.ts" => {
            Some(include_str!("lib-assets-stripped/esnext.decorators.d.ts"))
        }
        "esnext.disposable.d.ts" => {
            Some(include_str!("lib-assets-stripped/esnext.disposable.d.ts"))
        }
        "esnext.error.d.ts" => Some(include_str!("lib-assets-stripped/esnext.error.d.ts")),
        "esnext.float16.d.ts" => Some(include_str!("lib-assets-stripped/esnext.float16.d.ts")),
        "esnext.full.d.ts" => Some(include_str!("lib-assets-stripped/esnext.full.d.ts")),
        "esnext.intl.d.ts" => Some(include_str!("lib-assets-stripped/esnext.intl.d.ts")),
        "esnext.iterator.d.ts" => Some(include_str!("lib-assets-stripped/esnext.iterator.d.ts")),
        "esnext.promise.d.ts" => Some(include_str!("lib-assets-stripped/esnext.promise.d.ts")),
        "esnext.sharedmemory.d.ts" => {
            Some(include_str!("lib-assets-stripped/esnext.sharedmemory.d.ts"))
        }
        "esnext.typedarrays.d.ts" => {
            Some(include_str!("lib-assets-stripped/esnext.typedarrays.d.ts"))
        }
        "scripthost.d.ts" => Some(include_str!("lib-assets-stripped/scripthost.d.ts")),
        "tsserverlibrary.d.ts" => Some(include_str!("lib-assets-stripped/tsserverlibrary.d.ts")),
        "typescript.d.ts" => Some(include_str!("lib-assets-stripped/typescript.d.ts")),
        "webworker.asynciterable.d.ts" => Some(include_str!(
            "lib-assets-stripped/webworker.asynciterable.d.ts"
        )),
        "webworker.d.ts" => Some(include_str!("lib-assets-stripped/webworker.d.ts")),
        "webworker.importscripts.d.ts" => Some(include_str!(
            "lib-assets-stripped/webworker.importscripts.d.ts"
        )),
        "webworker.iterable.d.ts" => {
            Some(include_str!("lib-assets-stripped/webworker.iterable.d.ts"))
        }
        _ => None,
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
    "es5.d.ts",
    "es5.full.d.ts",
    "es6.d.ts",
    "esnext.array.d.ts",
    "esnext.collection.d.ts",
    "esnext.d.ts",
    "esnext.decorators.d.ts",
    "esnext.disposable.d.ts",
    "esnext.error.d.ts",
    "esnext.float16.d.ts",
    "esnext.full.d.ts",
    "esnext.intl.d.ts",
    "esnext.iterator.d.ts",
    "esnext.promise.d.ts",
    "esnext.sharedmemory.d.ts",
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
        }

        assert_eq!(seen, LIB_FILE_COUNT);
    }
}
