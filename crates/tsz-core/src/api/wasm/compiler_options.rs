use serde::Deserialize;
use wasm_bindgen::prelude::JsValue;

/// Compiler options passed from JavaScript/WASM.
/// Maps to TypeScript compiler options.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompilerOptions {
    /// Enable all strict type checking options.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict: Option<bool>,

    /// Raise error on expressions and declarations with an implied 'any' type.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_implicit_any: Option<bool>,

    /// Enable strict null checks.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict_null_checks: Option<bool>,

    /// Enable strict checking of function types.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict_function_types: Option<bool>,

    /// Enable strict property initialization checks in classes.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict_property_initialization: Option<bool>,

    /// Report error when not all code paths in function return a value.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_implicit_returns: Option<bool>,

    /// Raise error on 'this' expressions with an implied 'any' type.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_implicit_this: Option<bool>,

    /// Specify ECMAScript target version (accepts string like "ES5" or numeric).
    #[serde(default, deserialize_with = "deserialize_target")]
    target: Option<u32>,

    /// Specify module code generation mode (accepts string like `ESNext` or numeric).
    #[serde(default, deserialize_with = "deserialize_module")]
    module: Option<u32>,

    /// Interpret optional property types as written, rather than adding 'undefined'.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    exact_optional_property_types: Option<bool>,

    /// When true, do not include any library files.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_lib: Option<bool>,

    /// Add 'undefined' to a type when accessed using an index.
    #[serde(
        default,
        alias = "noUncheckedIndexedAccess",
        deserialize_with = "deserialize_bool_option"
    )]
    no_unchecked_indexed_access: Option<bool>,

    /// Enable Sound Mode for stricter type checking beyond TypeScript's defaults.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    sound_mode: Option<bool>,
}

/// Deserialize a boolean option that can be a boolean, string, or comma-separated string.
/// TypeScript test files often have boolean options like "true, false" for different test cases.
fn deserialize_bool_option<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct BoolOptionVisitor;

    impl<'de> Visitor<'de> for BoolOptionVisitor {
        type Value = Option<bool>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean, string, or comma-separated list of booleans")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Handle comma-separated values like "true, false" - take the first value
            let first_value = value.split(',').next().unwrap_or(value).trim();
            let result = match first_value.to_lowercase().as_str() {
                "true" | "1" => Some(true),
                "false" | "0" => Some(false),
                _ => None,
            };
            Ok(result)
        }
    }

    deserializer.deserialize_any(BoolOptionVisitor)
}

#[derive(Clone, Copy)]
enum WasmCompilerOptionKind {
    Target,
    Module,
}

fn deserialize_target<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_target_or_module(deserializer, WasmCompilerOptionKind::Target)
}

fn deserialize_module<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_target_or_module(deserializer, WasmCompilerOptionKind::Module)
}

/// Deserialize target/module values that can be either strings or numbers.
/// TypeScript test files often use strings like "ES5", "ES2015", "CommonJS", etc.
fn deserialize_target_or_module<'de, D>(
    deserializer: D,
    kind: WasmCompilerOptionKind,
) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct TargetOrModuleVisitor {
        kind: WasmCompilerOptionKind,
    }

    impl<'de> Visitor<'de> for TargetOrModuleVisitor {
        type Value = Option<u32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or integer representing target/module")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value as u32))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value as u32))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let result = match self.kind {
                WasmCompilerOptionKind::Target => crate::common::ScriptTarget::from_ts_str(value)
                    .map(|target| u32::from(target.ts_numeric_value())),
                WasmCompilerOptionKind::Module => crate::common::ModuleKind::from_ts_str(value)
                    .map(crate::common::ModuleKind::ts_numeric_value),
            };
            Ok(result)
        }
    }

    deserializer.deserialize_any(TargetOrModuleVisitor { kind })
}

impl CompilerOptions {
    /// Resolve a boolean option with strict mode fallback.
    /// If the specific option is set, use it; otherwise, fall back to strict mode.
    fn resolve_bool(&self, specific: Option<bool>, strict_implies: bool) -> bool {
        if let Some(value) = specific {
            return value;
        }
        if strict_implies {
            // In TypeScript 6.0+, strict-family flags default to true even
            // without `--strict`. When `strict` is not explicitly set (None),
            // strict-implied flags are enabled by default.
            return self.strict.unwrap_or(true);
        }
        false
    }

    /// Get the effective value for noImplicitAny.
    fn get_no_implicit_any(&self) -> bool {
        self.resolve_bool(self.no_implicit_any, true)
    }

    /// Get the effective value for strictNullChecks.
    fn get_strict_null_checks(&self) -> bool {
        self.resolve_bool(self.strict_null_checks, true)
    }

    /// Get the effective value for strictFunctionTypes.
    fn get_strict_function_types(&self) -> bool {
        self.resolve_bool(self.strict_function_types, true)
    }

    /// Get the effective value for strictPropertyInitialization.
    fn get_strict_property_initialization(&self) -> bool {
        self.resolve_bool(self.strict_property_initialization, true)
    }

    /// Get the effective value for noImplicitReturns.
    fn get_no_implicit_returns(&self) -> bool {
        self.resolve_bool(self.no_implicit_returns, false)
    }

    /// Get the effective value for noImplicitThis.
    fn get_no_implicit_this(&self) -> bool {
        self.resolve_bool(self.no_implicit_this, true)
    }

    fn resolve_target(&self) -> crate::checker::context::ScriptTarget {
        use crate::checker::context::ScriptTarget;

        match self.target {
            Some(0) => ScriptTarget::ES3,
            Some(1) => ScriptTarget::ES5,
            Some(2) => ScriptTarget::ES2015,
            Some(3) => ScriptTarget::ES2016,
            Some(4) => ScriptTarget::ES2017,
            Some(5) => ScriptTarget::ES2018,
            Some(6) => ScriptTarget::ES2019,
            Some(7) => ScriptTarget::ES2020,
            Some(_) => ScriptTarget::ESNext,
            None => ScriptTarget::default(),
        }
    }

    fn resolve_module(&self) -> crate::common::ModuleKind {
        self.module
            .and_then(crate::common::ModuleKind::from_ts_numeric)
            .unwrap_or(crate::common::ModuleKind::None)
    }

    /// Convert to `CheckerOptions` for type checking.
    pub(crate) fn to_checker_options(&self) -> crate::checker::context::CheckerOptions {
        let strict = self.strict.unwrap_or(false);
        let strict_null_checks = self.get_strict_null_checks();
        crate::checker::context::CheckerOptions {
            strict,
            no_implicit_any: self.get_no_implicit_any(),
            no_implicit_returns: self.get_no_implicit_returns(),
            strict_null_checks,
            strict_function_types: self.get_strict_function_types(),
            strict_property_initialization: self.get_strict_property_initialization(),
            no_implicit_this: self.get_no_implicit_this(),
            use_unknown_in_catch_variables: strict_null_checks,
            isolated_modules: false,
            no_unchecked_indexed_access: self.no_unchecked_indexed_access.unwrap_or(false),
            strict_bind_call_apply: false,
            exact_optional_property_types: self.exact_optional_property_types.unwrap_or(false),
            no_lib: self.no_lib.unwrap_or(false),
            no_types_and_symbols: false,
            target: self.resolve_target(),
            module: self.resolve_module(),
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,

            es_module_interop: false,
            allow_synthetic_default_imports: false,
            allow_unreachable_code: None,
            allow_unused_labels: None,
            no_property_access_from_index_signature: false,
            sound_mode: self.sound_mode.unwrap_or(false),
            experimental_decorators: false,
            no_unused_locals: false,
            no_unused_parameters: false,
            always_strict: strict,
            resolve_json_module: false, // WASM API: defaults to false
            check_js: false,            // WASM API: defaults to false
            allow_js: false,
            no_resolve: false,
            isolated_declarations: false,
            emit_declarations: false,
            no_unchecked_side_effect_imports: true,
            no_implicit_override: false,
            jsx_mode: tsz_common::checker_options::JsxMode::None,
            module_explicitly_set: self.module.is_some(),
            suppress_excess_property_errors: false,
            suppress_implicit_any_index_errors: false,
            no_implicit_use_strict: false,
            allow_importing_ts_extensions: false,
            rewrite_relative_import_extensions: false,
            implied_classic_resolution: false,
            jsx_import_source: String::new(),
            verbatim_module_syntax: false,
            ignore_deprecations: false,
            allow_umd_global_access: false,
            preserve_const_enums: false,
            strict_builtin_iterator_return: strict,
            erasable_syntax_only: false,
            no_fallthrough_cases_in_switch: false,
        }
    }
}

pub(crate) fn parse_compiler_options_json(options_json: &str) -> Result<CompilerOptions, JsValue> {
    serde_json::from_str::<CompilerOptions>(options_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse compiler options: {e}")))
}

#[cfg(test)]
mod tests {
    use super::parse_compiler_options_json;

    #[test]
    fn parse_compiler_options_json_accepts_valid_input() {
        let parsed = parse_compiler_options_json(r#"{"strict":true,"module":99}"#);
        assert!(parsed.is_ok(), "valid options JSON should parse");
    }

    #[test]
    fn parse_compiler_options_json_uses_separate_target_and_module_domains() {
        let parsed =
            parse_compiler_options_json(r#"{"target":"ES2015","module":"ES2015"}"#).unwrap();

        assert_eq!(parsed.target, Some(2));
        assert_eq!(parsed.module, Some(5));
        assert_eq!(
            parsed.to_checker_options().module,
            crate::common::ModuleKind::ES2015
        );
    }

    #[test]
    fn parse_compiler_options_json_ignores_no_types_and_symbols() {
        let parsed = parse_compiler_options_json(r#"{"noTypesAndSymbols":true}"#).unwrap();
        assert!(
            !parsed.to_checker_options().no_types_and_symbols,
            "WASM compiler options should ignore noTypesAndSymbols"
        );
    }
}
