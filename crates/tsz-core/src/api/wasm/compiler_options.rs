use serde::Deserialize;

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
    #[serde(default, deserialize_with = "deserialize_target_option")]
    target: Option<WasmScriptTargetOption>,

    /// Specify module code generation mode (accepts string like `ESNext` or numeric).
    #[serde(default, deserialize_with = "deserialize_module_option")]
    module: Option<WasmModuleKindOption>,

    /// Interpret optional property types as written, rather than adding 'undefined'.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    exact_optional_property_types: Option<bool>,

    /// When true, do not include any library files.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_lib: Option<bool>,

    /// When true, do not load default types and symbols (test harness directive).
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_types_and_symbols: Option<bool>,

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

#[derive(Clone, Copy, Debug)]
enum WasmScriptTargetOption {
    ES3,
    ES5,
    ES2015,
    ES2016,
    ES2017,
    ES2018,
    ES2019,
    ES2020,
    ES2021,
    ES2022,
    ES2023,
    ESNext,
    Unknown,
}

impl WasmScriptTargetOption {
    const fn from_numeric(value: u32) -> Self {
        match value {
            0 => Self::ES3,
            1 => Self::ES5,
            2 => Self::ES2015,
            3 => Self::ES2016,
            4 => Self::ES2017,
            5 => Self::ES2018,
            6 => Self::ES2019,
            7 => Self::ES2020,
            8 => Self::ES2021,
            9 => Self::ES2022,
            10 => Self::ES2023,
            99 => Self::ESNext,
            _ => {
                let _ = value;
                Self::Unknown
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum WasmModuleKindOption {
    None,
    CommonJS,
    AMD,
    UMD,
    System,
    ES2015,
    ES2020,
    ES2022,
    ESNext,
    Node16,
    NodeNext,
    Unknown,
}

impl WasmModuleKindOption {
    const fn from_numeric(value: u32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::CommonJS,
            2 => Self::AMD,
            3 => Self::UMD,
            4 => Self::System,
            5 => Self::ES2015,
            6 => Self::ES2020,
            7 => Self::ES2022,
            99 => Self::ESNext,
            100 => Self::Node16,
            199 => Self::NodeNext,
            _ => {
                let _ = value;
                Self::Unknown
            }
        }
    }
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

/// Parse legacy WASM playground target/module token strings.
/// This preserves historical compatibility for mixed target/module string values.
fn parse_legacy_target_or_module_token(value: &str) -> Option<u32> {
    match value.to_uppercase().as_str() {
        // ScriptTarget values (0-10, 99) and ModuleKind-specific values
        // Combined arms where ScriptTarget and ModuleKind share the same numeric value
        "ES3" | "NONE" => Some(0),
        "ES5" | "COMMONJS" => Some(1),
        "ES2015" | "ES6" | "AMD" => Some(2),
        "ES2016" | "UMD" => Some(3),
        "ES2017" | "SYSTEM" => Some(4),
        "ES2018" => Some(5),
        "ES2019" => Some(6),
        "ES2020" => Some(7),
        "ES2021" => Some(8),
        "ES2022" => Some(9),
        "ES2023" => Some(10),
        "ESNEXT" => Some(99),
        "NODE16" => Some(100),
        "NODENEXT" => Some(199),
        _ => None,
    }
}

/// Deserialize `target` values that can be either strings or numbers.
fn deserialize_target_option<'de, D>(
    deserializer: D,
) -> Result<Option<WasmScriptTargetOption>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct TargetOptionVisitor;

    impl<'de> Visitor<'de> for TargetOptionVisitor {
        type Value = Option<WasmScriptTargetOption>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or integer representing target")
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
            Ok(Some(WasmScriptTargetOption::from_numeric(value as u32)))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(WasmScriptTargetOption::from_numeric(value as u32)))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(
                parse_legacy_target_or_module_token(value)
                    .map(WasmScriptTargetOption::from_numeric),
            )
        }
    }

    deserializer.deserialize_any(TargetOptionVisitor)
}

/// Deserialize `module` values that can be either strings or numbers.
fn deserialize_module_option<'de, D>(
    deserializer: D,
) -> Result<Option<WasmModuleKindOption>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct ModuleOptionVisitor;

    impl<'de> Visitor<'de> for ModuleOptionVisitor {
        type Value = Option<WasmModuleKindOption>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or integer representing module kind")
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
            Ok(Some(WasmModuleKindOption::from_numeric(value as u32)))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(WasmModuleKindOption::from_numeric(value as u32)))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(parse_legacy_target_or_module_token(value).map(WasmModuleKindOption::from_numeric))
        }
    }

    deserializer.deserialize_any(ModuleOptionVisitor)
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
            Some(WasmScriptTargetOption::ES3) => ScriptTarget::ES3,
            Some(WasmScriptTargetOption::ES5) => ScriptTarget::ES5,
            Some(WasmScriptTargetOption::ES2015) => ScriptTarget::ES2015,
            Some(WasmScriptTargetOption::ES2016) => ScriptTarget::ES2016,
            Some(WasmScriptTargetOption::ES2017) => ScriptTarget::ES2017,
            Some(WasmScriptTargetOption::ES2018) => ScriptTarget::ES2018,
            Some(WasmScriptTargetOption::ES2019) => ScriptTarget::ES2019,
            Some(WasmScriptTargetOption::ES2020) => ScriptTarget::ES2020,
            Some(WasmScriptTargetOption::ES2021)
            | Some(WasmScriptTargetOption::ES2022)
            | Some(WasmScriptTargetOption::ES2023)
            | Some(WasmScriptTargetOption::ESNext)
            | Some(WasmScriptTargetOption::Unknown) => ScriptTarget::ESNext,
            None => ScriptTarget::default(),
        }
    }

    const fn resolve_module(&self) -> crate::common::ModuleKind {
        match self.module {
            Some(WasmModuleKindOption::CommonJS) => crate::common::ModuleKind::CommonJS,
            Some(WasmModuleKindOption::AMD) => crate::common::ModuleKind::AMD,
            Some(WasmModuleKindOption::UMD) => crate::common::ModuleKind::UMD,
            Some(WasmModuleKindOption::System) => crate::common::ModuleKind::System,
            Some(WasmModuleKindOption::ES2015) => crate::common::ModuleKind::ES2015,
            Some(WasmModuleKindOption::ES2020) => crate::common::ModuleKind::ES2020,
            Some(WasmModuleKindOption::ES2022) => crate::common::ModuleKind::ES2022,
            Some(WasmModuleKindOption::ESNext) => crate::common::ModuleKind::ESNext,
            Some(WasmModuleKindOption::Node16) => crate::common::ModuleKind::Node16,
            Some(WasmModuleKindOption::NodeNext) => crate::common::ModuleKind::NodeNext,
            Some(WasmModuleKindOption::None) | Some(WasmModuleKindOption::Unknown) | None => {
                crate::common::ModuleKind::None
            }
        }
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
            no_types_and_symbols: self.no_types_and_symbols.unwrap_or(false),
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

#[cfg(test)]
mod tests {
    use super::CompilerOptions;

    #[test]
    fn preserves_legacy_cross_domain_string_mapping() {
        let options: CompilerOptions =
            serde_json::from_str(r#"{"target":"commonjs","module":"es2021"}"#)
                .expect("compiler options should parse");
        let checker_options = options.to_checker_options();

        assert_eq!(
            checker_options.target,
            crate::checker::context::ScriptTarget::ES5
        );
        assert_eq!(checker_options.module, crate::common::ModuleKind::None);
    }

    #[test]
    fn preserves_unknown_numeric_fallback_behavior() {
        let options: CompilerOptions = serde_json::from_str(r#"{"target":1234,"module":1234}"#)
            .expect("compiler options should parse");
        let checker_options = options.to_checker_options();

        assert_eq!(
            checker_options.target,
            crate::checker::context::ScriptTarget::ESNext
        );
        assert_eq!(checker_options.module, crate::common::ModuleKind::None);
    }
}
