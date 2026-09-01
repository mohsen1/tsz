//! Compiler-option schema, typed decoding, and pinned TypeScript 7 diagnostics.

use super::{ConfigOptionOccurrence, ConfigOptionSpans, absolute_path, logical_path_from_host};
use crate::diagnostics::Diagnostic;
use crate::program::{CompilerOptions, DeferredCompilerOption, DeferredCompilerOptionValue};
use crate::source::display_path;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

macro_rules! option_schema {
    (@name bool) => { "boolean" };
    (@name array) => { "Array" };
    (@name string) => { "string" };
    (@name enum_string) => { "enum" };
    (@name path) => { "string" };
    (@decode bool, $value:expr, $origin:expr) => { $value.as_bool() };
    (@decode array, $value:expr, $origin:expr) => {
        $value.as_array().and_then(|values| {
            values
                .iter()
                .map(Value::as_str)
                .map(|value| value.map(str::to_string))
                .collect()
        })
    };
    (@decode string, $value:expr, $origin:expr) => { $value.as_str().map(str::to_string) };
    (@decode enum_string, $value:expr, $origin:expr) => { $value.as_str().map(str::to_string) };
    (@decode path, $value:expr, $origin:expr) => {
        $value.as_str().map(|value| absolute_path($origin, Path::new(value)))
    };
    (@apply bool, $source:expr, $target:expr) => {
        if let Some(value) = $source { $target = (*value).into(); }
    };
    (@apply string, $source:expr, $target:expr) => {
        if let Some(value) = $source { $target.clone_from(value); }
    };
    (@apply enum_string, $source:expr, $target:expr) => {
        option_schema!(@apply string, $source, $target);
    };
    (@apply $kind:ident, $source:expr, $target:expr) => {
        if let Some(value) = $source { $target = Some(value.clone()); }
    };
    (@cli bool, $value:expr) => { $value.eq_ignore_ascii_case("true") };
    (@cli array, $value:expr) => {
        $value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect()
    };
    (@cli string, $value:expr) => { $value.to_string() };
    (@cli enum_string, $value:expr) => { $value.to_string() };
    (@cli path, $value:expr) => { PathBuf::from($value) };
    ($($key:ident => $field:ident: $type:ty, $name:literal, $kind:ident;)+) => {
        /// A directly modeled compiler option with source-owned diagnostics.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum CompilerOptionKey { $($key,)+ }

        impl CompilerOptionKey {
            pub(super) const ALL: &'static [Self] = &[$(Self::$key,)+];
            const fn json_name(self) -> &'static str {
                match self { $(Self::$key => $name,)+ }
            }
            const fn kind_name(self) -> &'static str {
                match self { $(Self::$key => option_schema!(@name $kind),)+ }
            }
            fn from_json_name(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|key| key.json_name() == name)
            }
            /// Resolve TypeScript's case-insensitive CLI spelling.
            #[must_use]
            pub fn from_cli_name(name: &str) -> Option<Self> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|key| key.json_name().eq_ignore_ascii_case(name))
            }
            #[must_use]
            pub fn takes_value(self) -> bool {
                self.kind_name() != "boolean"
            }
        }

        /// Explicit config/process values; absence differs from false/default.
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct CompilerOptionPatch {
            $(pub $field: Option<$type>,)+
            pub deferred_options: BTreeMap<DeferredCompilerOption, DeferredCompilerOptionValue>,
        }

        impl CompilerOptionPatch {
            pub(super) const fn contains(&self, key: CompilerOptionKey) -> bool {
                match key { $(CompilerOptionKey::$key => self.$field.is_some(),)+ }
            }
            pub(super) fn merge_from(&mut self, other: &Self) {
                $(if other.$field.is_some() { self.$field.clone_from(&other.$field); })+
                self.deferred_options.extend(other.deferred_options.clone());
            }
            pub(super) fn apply_to(&self, options: &mut CompilerOptions) {
                $(option_schema!(@apply $kind, &self.$field, options.$field);)+
                options.deferred_options.extend(self.deferred_options.clone());
            }
            fn set_config(&mut self, key: CompilerOptionKey, value: &Value, origin: &Path) -> bool {
                match key {
                    $(CompilerOptionKey::$key => {
                        let Some(value) = option_schema!(@decode $kind, value, origin) else {
                            return false;
                        };
                        self.$field = Some(value);
                        true
                    },)+
                }
            }
            /// Decode one process-adapter value through the same schema.
            pub fn set_cli_value(&mut self, key: CompilerOptionKey, value: &str) {
                match key {
                    $(CompilerOptionKey::$key => {
                        self.$field = Some(option_schema!(@cli $kind, value));
                    },)+
                }
            }
        }
    };
}

option_schema! {
    Strict => strict: bool, "strict", bool;
    StrictNullChecks => strict_null_checks: bool, "strictNullChecks", bool;
    StrictPropertyInitialization => strict_property_initialization: bool, "strictPropertyInitialization", bool;
    NoImplicitAny => no_implicit_any: bool, "noImplicitAny", bool;
    NoUnusedLocals => no_unused_locals: bool, "noUnusedLocals", bool;
    NoUnusedParameters => no_unused_parameters: bool, "noUnusedParameters", bool;
    NoLib => no_lib: bool, "noLib", bool;
    Lib => lib: Vec<String>, "lib", array;
    AllowJs => allow_js: bool, "allowJs", bool;
    CheckJs => check_js: bool, "checkJs", bool;
    NoCheck => no_check: bool, "noCheck", bool;
    SkipLibCheck => skip_lib_check: bool, "skipLibCheck", bool;
    NoEmit => no_emit: bool, "noEmit", bool;
    NoEmitOnError => no_emit_on_error: bool, "noEmitOnError", bool;
    Declaration => declaration: bool, "declaration", bool;
    DeclarationMap => declaration_map: bool, "declarationMap", bool;
    SourceMap => source_map: bool, "sourceMap", bool;
    InlineSourceMap => inline_source_map: bool, "inlineSourceMap", bool;
    RemoveComments => remove_comments: bool, "removeComments", bool;
    UseDefineForClassFields => use_define_for_class_fields: bool, "useDefineForClassFields", bool;
    Target => target: String, "target", enum_string;
    Module => module: String, "module", enum_string;
    RootDir => root_dir: PathBuf, "rootDir", path;
    OutDir => out_dir: PathBuf, "outDir", path;
    DeclarationDir => declaration_dir: PathBuf, "declarationDir", path;
}

impl CompilerOptionPatch {
    /// Preserve one accepted but not-yet-owned process option in the typed snapshot.
    pub fn set_deferred_cli_value(&mut self, key: DeferredCompilerOption, value: &str) {
        let value = if key.is_boolean() {
            DeferredCompilerOptionValue::Boolean(value.eq_ignore_ascii_case("true"))
        } else if key.is_path() {
            DeferredCompilerOptionValue::Path(PathBuf::from(value))
        } else {
            DeferredCompilerOptionValue::String(value.to_string())
        };
        self.deferred_options.insert(key, value);
    }
    pub(super) fn absolutize_deferred_paths(&mut self, origin: &Path) {
        self.deferred_options.values_mut().for_each(|value| {
            if let DeferredCompilerOptionValue::Path(path) = value {
                *path = absolute_path(origin, path);
            }
        });
    }
    fn set_deferred(&mut self, key: DeferredCompilerOption, value: &Value, origin: &Path) -> bool {
        let decoded = if key.is_boolean() {
            value.as_bool().map(DeferredCompilerOptionValue::Boolean)
        } else if key.is_path() {
            value.as_str().map(|value| {
                DeferredCompilerOptionValue::Path(absolute_path(origin, Path::new(value)))
            })
        } else {
            value
                .as_str()
                .map(|value| DeferredCompilerOptionValue::String(value.to_string()))
        };
        decoded.is_some_and(|value| {
            self.deferred_options.insert(key, value);
            true
        })
    }
}

/// Typed result of classifying one `target` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetValueOutcome {
    Accepted,
    Invalid { message: &'static str, code: u32 },
    Removed { message: &'static str, code: u32 },
}

/// Classify `target` once; each consumer owns its product-specific diagnostic.
#[must_use]
pub fn classify_target_value(target: &str) -> TargetValueOutcome {
    match target.to_ascii_lowercase().as_str() {
        "es5" => TargetValueOutcome::Removed {
            message: "Option 'target=ES5' has been removed. Please remove it from your configuration.",
            code: 5108,
        },
        "es6" | "es2015" | "es2016" | "es2017" | "es2018" | "es2019" | "es2020" | "es2021"
        | "es2022" | "es2023" | "es2024" | "es2025" | "esnext" => TargetValueOutcome::Accepted,
        _ => TargetValueOutcome::Invalid {
            message: concat!(
                "Argument for '--target' option must be: 'es6', 'es2015', 'es2016', ",
                "'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', ",
                "'es2023', 'es2024', 'es2025', 'esnext'."
            ),
            code: 6046,
        },
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompilerOptionOrigin {
    file: String,
    source_text: Arc<str>,
    span: ConfigOptionSpans,
}

impl CompilerOptionOrigin {
    fn diagnostic(&self, at_value: bool, message: String, code: u32) -> Diagnostic {
        let (start, length) = if at_value {
            self.span.value.unwrap_or(self.span.key)
        } else {
            self.span.key
        };
        Diagnostic::error_at_text(
            self.file.clone(),
            start,
            length,
            Arc::clone(&self.source_text),
            message,
            code,
        )
    }
    pub(crate) fn diagnostic_at_key(&self, message: String, code: u32) -> Diagnostic {
        self.diagnostic(false, message, code)
    }
    pub(crate) fn diagnostic_at_value(&self, message: String, code: u32) -> Diagnostic {
        self.diagnostic(true, message, code)
    }
    pub(super) const fn for_compiler_options_key(
        file: String,
        source_text: Arc<str>,
        (key_start, key_length): (u32, u32),
    ) -> Self {
        Self {
            file,
            source_text,
            span: ConfigOptionSpans {
                key: (key_start, key_length),
                value: None,
            },
        }
    }
    pub(super) fn belongs_to(&self, config_path: &Path, current_directory: &Path) -> bool {
        self.file == display_path(&logical_path_from_host(current_directory, config_path))
    }
}

#[derive(Debug, Default)]
pub(super) struct DecodedCompilerOptions {
    pub(super) patch: CompilerOptionPatch,
    pub(super) option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
    pub(super) authored_option_origins: BTreeMap<CompilerOptionKey, CompilerOptionOrigin>,
    pub(super) deferred_diagnostics: BTreeMap<DeferredCompilerOption, Option<Diagnostic>>,
}

fn deferred_key(name: &str) -> Option<DeferredCompilerOption> {
    DeferredCompilerOption::ALL
        .iter()
        .copied()
        .find(|key| key.json_name() == name)
}

const fn deferred_kind_name(key: DeferredCompilerOption) -> &'static str {
    if key.is_boolean() {
        "boolean"
    } else if matches!(
        key,
        DeferredCompilerOption::Jsx
            | DeferredCompilerOption::ModuleResolution
            | DeferredCompilerOption::ModuleDetection
    ) {
        "enum"
    } else {
        "string"
    }
}

fn wrong_type(origin: &CompilerOptionOrigin, name: &str, kind: &str) -> Diagnostic {
    origin.diagnostic_at_value(
        format!("Compiler option '{name}' requires a value of type {kind}."),
        5024,
    )
}

pub(super) fn decode_compiler_options(
    occurrences: &[ConfigOptionOccurrence],
    directory: &Path,
    logical_path: &Path,
    source_text: &Arc<str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> DecodedCompilerOptions {
    let mut decoded = DecodedCompilerOptions::default();
    for occurrence in occurrences {
        let origin = CompilerOptionOrigin {
            file: display_path(logical_path),
            source_text: Arc::clone(source_text),
            span: occurrence.span,
        };
        if let Some(key) = CompilerOptionKey::from_json_name(&occurrence.name) {
            decoded.authored_option_origins.insert(key, origin.clone());
            if key == CompilerOptionKey::Target
                && let Some(target) = occurrence.value.as_str()
                && let TargetValueOutcome::Invalid { message, code } = classify_target_value(target)
            {
                diagnostics.push(origin.diagnostic_at_value(message.to_string(), code));
            } else if decoded.patch.set_config(key, &occurrence.value, directory) {
                decoded.option_origins.insert(key, origin);
            } else {
                diagnostics.push(wrong_type(&origin, key.json_name(), key.kind_name()));
            }
            continue;
        }
        if let Some(key) = deferred_key(&occurrence.name) {
            if decoded
                .patch
                .set_deferred(key, &occurrence.value, directory)
            {
                let value = decoded
                    .patch
                    .deferred_options
                    .get(&key)
                    .expect("value was set");
                decoded.deferred_diagnostics.insert(
                    key,
                    removed_option_diagnostic(key, value, &occurrence.value, &origin),
                );
            } else {
                diagnostics.push(wrong_type(
                    &origin,
                    key.json_name(),
                    deferred_kind_name(key),
                ));
            }
            continue;
        }
        let suggestion = CompilerOptionKey::ALL
            .iter()
            .map(|key| key.json_name())
            .chain(
                DeferredCompilerOption::ALL
                    .iter()
                    .map(|key| key.json_name()),
            )
            .find(|known| known.eq_ignore_ascii_case(&occurrence.name));
        let (code, message) = if let Some(known) = suggestion {
            (
                5025,
                format!(
                    "Unknown compiler option '{}'. Did you mean '{known}'?",
                    occurrence.name
                ),
            )
        } else {
            (
                5023,
                format!("Unknown compiler option '{}'.", occurrence.name),
            )
        };
        diagnostics.push(origin.diagnostic_at_key(message, code));
    }
    decoded
}

fn removed_option_diagnostic(
    key: DeferredCompilerOption,
    value: &DeferredCompilerOptionValue,
    authored: &Value,
    origin: &CompilerOptionOrigin,
) -> Option<Diagnostic> {
    let removed = |name: &str| {
        format!("Option '{name}' has been removed. Please remove it from your configuration.")
    };
    match (key, value) {
        (DeferredCompilerOption::DownlevelIteration | DeferredCompilerOption::OutFile, _) => {
            Some(origin.diagnostic_at_key(removed(key.json_name()), 5102))
        }
        (DeferredCompilerOption::BaseUrl, _) => {
            let raw = authored.as_str().unwrap_or_default().trim_end_matches('/');
            let wildcard =
                serde_json::to_string(&format!("{raw}/*")).expect("option string serializes");
            Some(origin.diagnostic_at_key(
                format!(
                    "{}\n  Use '\"paths\": {{\"*\": [{wildcard}]}}' instead.",
                    removed(key.json_name())
                ),
                5102,
            ))
        }
        (
            DeferredCompilerOption::AlwaysStrict | DeferredCompilerOption::EsModuleInterop,
            DeferredCompilerOptionValue::Boolean(false),
        ) => Some(origin.diagnostic_at_value(
            format!(
                "Option '{}=false' has been removed. Please remove it from your configuration.",
                key.json_name()
            ),
            5108,
        )),
        _ => None,
    }
}
