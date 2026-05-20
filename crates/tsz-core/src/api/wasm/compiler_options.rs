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

    /// Enable strict checking of `bind`, `call`, and `apply` methods.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict_bind_call_apply: Option<bool>,

    /// Enable strict property initialization checks in classes.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict_property_initialization: Option<bool>,

    /// Report error when not all code paths in function return a value.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_implicit_returns: Option<bool>,

    /// Raise error on 'this' expressions with an implied 'any' type.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_implicit_this: Option<bool>,

    /// Default catch clause variables as `unknown` instead of `any`.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    use_unknown_in_catch_variables: Option<bool>,

    /// Enable strict built-in iterator return types.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict_builtin_iterator_return: Option<bool>,

    /// Specify ECMAScript target version (accepts string like "ES5" or numeric).
    #[serde(default, deserialize_with = "deserialize_target")]
    target: Option<u32>,

    /// Specify module code generation mode (accepts string like `ESNext` or numeric).
    #[serde(default, deserialize_with = "deserialize_module")]
    module: Option<u32>,

    /// Enable full iterator support when targeting ES5/ES3.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    downlevel_iteration: Option<bool>,

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

/// Deserialize an optional boolean option.
///
/// WASM compiler options are user-facing input, so keep this strict:
/// booleans must be actual JSON booleans, not strings.
fn deserialize_bool_option<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<bool>::deserialize(deserializer)
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
    fn resolve_target(&self) -> crate::checker::context::ScriptTarget {
        self.target
            .and_then(crate::checker::context::ScriptTarget::from_ts_numeric)
            .unwrap_or_default()
    }

    fn resolve_module(&self) -> crate::common::ModuleKind {
        self.module
            .and_then(crate::common::ModuleKind::from_ts_numeric)
            .unwrap_or(crate::common::ModuleKind::None)
    }

    const fn apply_strict_option(
        options: &mut crate::checker::context::CheckerOptions,
        strict: bool,
    ) {
        options.strict = strict;
        if strict {
            options.no_implicit_any = true;
            options.strict_null_checks = true;
            options.strict_function_types = true;
            options.strict_bind_call_apply = true;
            options.strict_property_initialization = true;
            options.no_implicit_this = true;
            options.use_unknown_in_catch_variables = true;
            options.always_strict = true;
            options.strict_builtin_iterator_return = true;
        } else {
            options.no_implicit_any = false;
            options.strict_null_checks = false;
            options.strict_function_types = false;
            options.strict_bind_call_apply = false;
            options.strict_property_initialization = false;
            options.no_implicit_this = false;
            options.use_unknown_in_catch_variables = false;
            options.strict_builtin_iterator_return = false;
        }
    }

    /// Convert to `CheckerOptions` for type checking.
    pub(crate) fn to_checker_options(&self) -> crate::checker::context::CheckerOptions {
        let mut options = crate::checker::context::CheckerOptions::default();

        if let Some(strict) = self.strict {
            Self::apply_strict_option(&mut options, strict);
        }

        if let Some(v) = self.no_implicit_any {
            options.no_implicit_any = v;
        }
        if let Some(v) = self.no_implicit_returns {
            options.no_implicit_returns = v;
        }
        if let Some(v) = self.strict_null_checks {
            options.strict_null_checks = v;
        }
        if let Some(v) = self.strict_function_types {
            options.strict_function_types = v;
        }
        if let Some(v) = self.strict_bind_call_apply {
            options.strict_bind_call_apply = v;
        }
        if let Some(v) = self.strict_property_initialization {
            options.strict_property_initialization = v;
        }
        if let Some(v) = self.no_implicit_this {
            options.no_implicit_this = v;
        }
        if let Some(v) = self.use_unknown_in_catch_variables {
            options.use_unknown_in_catch_variables = v;
        }
        if let Some(v) = self.strict_builtin_iterator_return {
            options.strict_builtin_iterator_return = v;
        }
        if let Some(v) = self.no_unchecked_indexed_access {
            options.no_unchecked_indexed_access = v;
        }
        if let Some(v) = self.exact_optional_property_types {
            options.exact_optional_property_types = v;
        }
        if let Some(v) = self.no_lib {
            options.no_lib = v;
        }
        if let Some(v) = self.sound_mode {
            options.sound_mode = v;
        }
        if self.target.is_some() {
            options.target = self.resolve_target();
        }
        if self.module.is_some() {
            options.module = self.resolve_module();
            options.module_explicitly_set = true;
        }
        if let Some(v) = self.downlevel_iteration {
            options.downlevel_iteration = v;
        }

        options
    }
}

pub(crate) fn parse_compiler_options_json(options_json: &str) -> Result<CompilerOptions, JsValue> {
    serde_json::from_str::<CompilerOptions>(options_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse compiler options: {e}")))
}

#[cfg(test)]
mod tests {
    use super::{CompilerOptions, parse_compiler_options_json};

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
    fn to_checker_options_uses_shared_target_numeric_conversion() {
        let options = parse_compiler_options_json(r#"{"target":12}"#)
            .unwrap()
            .to_checker_options();

        assert_eq!(options.target, crate::common::ScriptTarget::ES2025);
    }

    #[test]
    fn to_checker_options_starts_from_shared_defaults() {
        let options = CompilerOptions::default().to_checker_options();
        let defaults = crate::checker::context::CheckerOptions::default();

        assert!(options.strict);
        assert!(options.no_implicit_any);
        assert!(options.strict_bind_call_apply);
        assert!(options.use_unknown_in_catch_variables);
        assert!(options.always_strict);
        assert!(options.strict_builtin_iterator_return);
        assert_eq!(options.jsx_factory, defaults.jsx_factory);
        assert_eq!(options.jsx_fragment_factory, defaults.jsx_fragment_factory);
        assert_eq!(options.target, defaults.target);
        assert_eq!(options.module, defaults.module);
        assert_eq!(
            options.no_unchecked_side_effect_imports,
            defaults.no_unchecked_side_effect_imports
        );
    }

    #[test]
    fn to_checker_options_strict_false_matches_shared_resolver_shape() {
        let options = parse_compiler_options_json(r#"{"strict":false}"#)
            .unwrap()
            .to_checker_options();

        assert!(!options.strict);
        assert!(!options.no_implicit_any);
        assert!(!options.strict_null_checks);
        assert!(!options.strict_function_types);
        assert!(!options.strict_bind_call_apply);
        assert!(!options.strict_property_initialization);
        assert!(!options.no_implicit_this);
        assert!(!options.use_unknown_in_catch_variables);
        assert!(!options.strict_builtin_iterator_return);
        assert!(
            options.always_strict,
            "strict:false should not clobber the shared alwaysStrict default"
        );
    }

    #[test]
    fn to_checker_options_individual_flags_override_strict() {
        let options = parse_compiler_options_json(
            r#"{
                "strict": true,
                "noImplicitAny": false,
                "strictNullChecks": false,
                "strictBindCallApply": false,
                "strictBuiltinIteratorReturn": false,
                "useUnknownInCatchVariables": false
            }"#,
        )
        .unwrap()
        .to_checker_options();

        assert!(options.strict);
        assert!(!options.no_implicit_any);
        assert!(!options.strict_null_checks);
        assert!(!options.strict_bind_call_apply);
        assert!(!options.strict_builtin_iterator_return);
        assert!(!options.use_unknown_in_catch_variables);
    }

    #[test]
    fn to_checker_options_preserves_downlevel_iteration() {
        let options = parse_compiler_options_json(r#"{"downlevelIteration":true}"#)
            .unwrap()
            .to_checker_options();

        assert!(options.downlevel_iteration);
    }

    #[test]
    fn parse_compiler_options_json_ignores_no_types_and_symbols() {
        let parsed = parse_compiler_options_json(r#"{"noTypesAndSymbols":true}"#).unwrap();
        assert!(
            !parsed.to_checker_options().no_types_and_symbols,
            "WASM compiler options should ignore noTypesAndSymbols"
        );
    }

    #[test]
    fn parse_compiler_options_json_rejects_string_boolean() {
        let parsed = serde_json::from_str::<CompilerOptions>(r#"{"strict":"true"}"#);
        assert!(
            parsed.is_err(),
            "string-typed booleans should be rejected in WASM compiler options"
        );
    }

    #[test]
    fn parse_compiler_options_json_rejects_comma_separated_boolean_string() {
        let parsed = serde_json::from_str::<CompilerOptions>(r#"{"strict":"true, false"}"#);
        assert!(
            parsed.is_err(),
            "comma-separated boolean strings should be rejected in WASM compiler options"
        );
    }
}
