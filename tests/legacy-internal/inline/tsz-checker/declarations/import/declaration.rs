//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/declarations/import/declaration.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN feb2fca705f6bafae2b487290e9ea663258e97009e9df148a79f1739d8c474ba 251 ts_extension_detects_ts
    #[test]
    fn ts_extension_detects_ts() {
        assert_eq!(ts_extension_suffix("./foo.ts"), Some(".ts"));
    }
// TSZ_INLINE_TEST_END feb2fca705f6bafae2b487290e9ea663258e97009e9df148a79f1739d8c474ba

// TSZ_INLINE_TEST_BEGIN c22414edad52b9b81a8796eeb9dead175c7961fb05248c3bbb7763da3ceb86d3 256 ts_extension_detects_tsx
    #[test]
    fn ts_extension_detects_tsx() {
        assert_eq!(ts_extension_suffix("./foo.tsx"), Some(".tsx"));
    }
// TSZ_INLINE_TEST_END c22414edad52b9b81a8796eeb9dead175c7961fb05248c3bbb7763da3ceb86d3

// TSZ_INLINE_TEST_BEGIN a021e934ecb639d7daeed29e0702d10d4bfaed426558c298485a58ce83e2a2e0 261 ts_extension_detects_mts
    #[test]
    fn ts_extension_detects_mts() {
        assert_eq!(ts_extension_suffix("./foo.mts"), Some(".mts"));
    }
// TSZ_INLINE_TEST_END a021e934ecb639d7daeed29e0702d10d4bfaed426558c298485a58ce83e2a2e0

// TSZ_INLINE_TEST_BEGIN 264ee687e256dbf1baec7c97fb7deebccdecb2aaf9b3c291b2d10806ea125e3c 266 ts_extension_detects_cts
    #[test]
    fn ts_extension_detects_cts() {
        assert_eq!(ts_extension_suffix("./foo.cts"), Some(".cts"));
    }
// TSZ_INLINE_TEST_END 264ee687e256dbf1baec7c97fb7deebccdecb2aaf9b3c291b2d10806ea125e3c

// TSZ_INLINE_TEST_BEGIN e7530356edeec00a465f689d393bb53803dbe16d81e025c2f3c7f7bad9387b8f 271 import_external_library_check_uses_node_modules_path_segment
    #[test]
    fn import_external_library_check_uses_node_modules_path_segment() {
        assert!(path_has_node_modules_segment(
            "/repo/node_modules/pkg/index.d.ts"
        ));
        assert!(path_has_node_modules_segment(
            r"C:\repo\node_modules\pkg\index.d.ts"
        ));
        assert!(path_has_node_modules_segment(
            "/repo/packages/app/node_modules/pkg/index.d.ts"
        ));

        assert!(!path_has_node_modules_segment(
            "/repo/fixtures/node_modules_pkg/index.d.ts"
        ));
        assert!(!path_has_node_modules_segment(
            "/repo/fixtures/not_node_modules/index.d.ts"
        ));
    }
// TSZ_INLINE_TEST_END e7530356edeec00a465f689d393bb53803dbe16d81e025c2f3c7f7bad9387b8f

// TSZ_INLINE_TEST_BEGIN 03a11798969eae4fffa86b3f6e1ea595f5893e00d68be8e6efe54073aaf4af7f 291 ts_extension_ignores_dts
    #[test]
    fn ts_extension_ignores_dts() {
        assert_eq!(ts_extension_suffix("./foo.d.ts"), None);
    }
// TSZ_INLINE_TEST_END 03a11798969eae4fffa86b3f6e1ea595f5893e00d68be8e6efe54073aaf4af7f

// TSZ_INLINE_TEST_BEGIN f6a379e058c3b02d6122f1767f48cbf05acc08fe1e87007362b82327dc84a20a 296 ts_extension_ignores_d_mts
    #[test]
    fn ts_extension_ignores_d_mts() {
        assert_eq!(ts_extension_suffix("./foo.d.mts"), None);
    }
// TSZ_INLINE_TEST_END f6a379e058c3b02d6122f1767f48cbf05acc08fe1e87007362b82327dc84a20a

// TSZ_INLINE_TEST_BEGIN c35d87b769bf114ceedd95f097e49d1b45013292e3b1cea8410d691f4febdb41 301 ts_extension_ignores_d_cts
    #[test]
    fn ts_extension_ignores_d_cts() {
        assert_eq!(ts_extension_suffix("./foo.d.cts"), None);
    }
// TSZ_INLINE_TEST_END c35d87b769bf114ceedd95f097e49d1b45013292e3b1cea8410d691f4febdb41

// TSZ_INLINE_TEST_BEGIN 15e56de7d13a5b3998481fc45a0cfd493bdebe6e2c069398d3190ebfc6387b98 306 ts_extension_ignores_js
    #[test]
    fn ts_extension_ignores_js() {
        assert_eq!(ts_extension_suffix("./foo.js"), None);
    }
// TSZ_INLINE_TEST_END 15e56de7d13a5b3998481fc45a0cfd493bdebe6e2c069398d3190ebfc6387b98

// TSZ_INLINE_TEST_BEGIN 953868c02844cffc5bbf96d37588b77e06681f5ef7327a72d03b2d4656bb7523 311 ts_extension_ignores_no_ext
    #[test]
    fn ts_extension_ignores_no_ext() {
        assert_eq!(ts_extension_suffix("./foo"), None);
    }
// TSZ_INLINE_TEST_END 953868c02844cffc5bbf96d37588b77e06681f5ef7327a72d03b2d4656bb7523

// TSZ_INLINE_TEST_BEGIN 0d530198e129d4306f1f143edc2933bda562a48d3ccbd65b371e3fb9143deced 316 ts_extension_ignores_json
    #[test]
    fn ts_extension_ignores_json() {
        assert_eq!(ts_extension_suffix("./data.json"), None);
    }
// TSZ_INLINE_TEST_END 0d530198e129d4306f1f143edc2933bda562a48d3ccbd65b371e3fb9143deced

// TSZ_INLINE_TEST_BEGIN 812aec838aa7af2305d999c14783e470ba14d61c661804ca0f35d8fc592393da 380 import_binding_is_type_only_detects_exported_interface
    #[test]
    fn import_binding_is_type_only_detects_exported_interface() {
        assert!(import_binding_is_type_only_for_named_files(
            &[
                (
                    "mod.d.ts",
                    r#"
export interface WriteFileOptions {}
export function writeFile(path: string, data: any, options: WriteFileOptions, callback: (err: Error) => void): void;
                    "#,
                ),
                (
                    "index.js",
                    r#"
import { writeFile, WriteFileOptions } from "./mod";
writeFile("", "", /** @type {WriteFileOptions} */ ({}), () => {});
                    "#,
                ),
            ],
            "index.js",
            "./mod",
            "WriteFileOptions",
        ));
    }
// TSZ_INLINE_TEST_END 812aec838aa7af2305d999c14783e470ba14d61c661804ca0f35d8fc592393da

// TSZ_INLINE_TEST_BEGIN d02d29a91b7d33ea28d0cce45cb69387ad1fd636e39e6e3be815897e3341c79a 405 import_binding_is_type_only_detects_default_interface_export
    #[test]
    fn import_binding_is_type_only_detects_default_interface_export() {
        assert!(import_binding_is_type_only_for_named_files(
            &[
                (
                    "dep.d.ts",
                    r#"
export default interface TruffleContract {
  foo: number;
}
                    "#,
                ),
                (
                    "caller.js",
                    r#"
import TruffleContract from "./dep";
                    "#,
                ),
            ],
            "caller.js",
            "./dep",
            "default",
        ));
    }
// TSZ_INLINE_TEST_END d02d29a91b7d33ea28d0cce45cb69387ad1fd636e39e6e3be815897e3341c79a
