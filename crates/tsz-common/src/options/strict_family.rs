//! Single owner of TypeScript's `--strict` umbrella expansion.
//!
//! TypeScript 6.0 defines the strict family via `StrictOptionName` and
//! `getStrictOptionValue` (`TypeScript/src/compiler/utilities.ts`): a member
//! that is not explicitly provided resolves to `strict !== false`; an
//! explicitly provided member always wins over the umbrella value.
//!
//! `alwaysStrict` is intentionally NOT in this table. TypeScript 6.0 removed
//! it from the strict family (`utilities.ts` `computedOptions.alwaysStrict`:
//! "Previously a strict-mode flag, but no longer"); it resolves as
//! `alwaysStrict !== false`, independent of `strict`. Each options surface
//! owns its explicit `alwaysStrict` override separately.
//!
//! `noImplicitReturns` is likewise NOT part of the family in tsc.

use super::checker::CheckerOptions;

/// Explicitly provided strict-family values from one options surface
/// (CLI args, tsconfig, WASM JSON, or the server protocol).
///
/// `None` means "not provided by the user": the member follows the `strict`
/// umbrella when `strict` is provided, and otherwise keeps the value already
/// present on the [`CheckerOptions`] being resolved.
#[derive(Debug, Clone, Copy, Default)]
pub struct StrictFamilyOverrides {
    pub strict: Option<bool>,
    pub no_implicit_any: Option<bool>,
    pub no_implicit_this: Option<bool>,
    pub strict_null_checks: Option<bool>,
    pub strict_function_types: Option<bool>,
    pub strict_bind_call_apply: Option<bool>,
    pub strict_property_initialization: Option<bool>,
    pub strict_builtin_iterator_return: Option<bool>,
    pub use_unknown_in_catch_variables: Option<bool>,
}

/// One strict-family member: the tsc option name plus accessors for the
/// per-surface explicit value and the resolved [`CheckerOptions`] slot.
pub struct StrictFamilyMember {
    /// tsc camelCase option name (one of tsc 6.0's `StrictOptionName`).
    pub name: &'static str,
    explicit: fn(&StrictFamilyOverrides) -> Option<bool>,
    slot: fn(&mut CheckerOptions) -> &mut bool,
}

/// The strict family, one row per tsc 6.0 `StrictOptionName` member
/// (`TypeScript/src/compiler/utilities.ts`).
pub const STRICT_FAMILY: &[StrictFamilyMember] = &[
    StrictFamilyMember {
        name: "noImplicitAny",
        explicit: |overrides| overrides.no_implicit_any,
        slot: |options| &mut options.no_implicit_any,
    },
    StrictFamilyMember {
        name: "noImplicitThis",
        explicit: |overrides| overrides.no_implicit_this,
        slot: |options| &mut options.no_implicit_this,
    },
    StrictFamilyMember {
        name: "strictNullChecks",
        explicit: |overrides| overrides.strict_null_checks,
        slot: |options| &mut options.strict_null_checks,
    },
    StrictFamilyMember {
        name: "strictFunctionTypes",
        explicit: |overrides| overrides.strict_function_types,
        slot: |options| &mut options.strict_function_types,
    },
    StrictFamilyMember {
        name: "strictBindCallApply",
        explicit: |overrides| overrides.strict_bind_call_apply,
        slot: |options| &mut options.strict_bind_call_apply,
    },
    StrictFamilyMember {
        name: "strictPropertyInitialization",
        explicit: |overrides| overrides.strict_property_initialization,
        slot: |options| &mut options.strict_property_initialization,
    },
    StrictFamilyMember {
        name: "strictBuiltinIteratorReturn",
        explicit: |overrides| overrides.strict_builtin_iterator_return,
        slot: |options| &mut options.strict_builtin_iterator_return,
    },
    StrictFamilyMember {
        name: "useUnknownInCatchVariables",
        explicit: |overrides| overrides.use_unknown_in_catch_variables,
        slot: |options| &mut options.use_unknown_in_catch_variables,
    },
];

/// Apply tsc's strict-family resolution to `options`.
///
/// Mirrors tsc 6.0 `getStrictOptionValue`: when `strict` is provided, the
/// umbrella value is expanded to every family member first, then explicitly
/// provided members override it. This bakes in the issue #3861 ordering:
/// `--strict false --strictNullChecks true` keeps `strictNullChecks` on.
/// Members that are not provided and have no umbrella keep the values
/// already present on `options`.
pub fn apply_strict_family(options: &mut CheckerOptions, overrides: &StrictFamilyOverrides) {
    if let Some(strict) = overrides.strict {
        options.strict = strict;
        for member in STRICT_FAMILY {
            *(member.slot)(options) = strict;
        }
    }
    for member in STRICT_FAMILY {
        if let Some(value) = (member.explicit)(overrides) {
            *(member.slot)(options) = value;
        }
    }
}

/// Expand only the `strict` umbrella across the family, with no explicit
/// member overrides.
///
/// Used by re-application sites (e.g. `CheckerOptions::apply_strict_defaults`)
/// that operate on an already-resolved `CheckerOptions` without an
/// explicitly-set mask. Sites that know which members the user provided must
/// use [`apply_strict_family`] so explicit members win over the umbrella.
pub fn expand_strict(options: &mut CheckerOptions, strict: bool) {
    apply_strict_family(
        options,
        &StrictFamilyOverrides {
            strict: Some(strict),
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{STRICT_FAMILY, StrictFamilyOverrides, apply_strict_family, expand_strict};
    use crate::options::checker::CheckerOptions;

    /// Pin the table to tsc 6.0's `StrictOptionName` union
    /// (`TypeScript/src/compiler/utilities.ts`). `alwaysStrict` must NOT be
    /// present: tsc 6.0 removed it from the strict family.
    #[test]
    fn table_matches_tsc_6_0_strict_option_name() {
        let names: Vec<&str> = STRICT_FAMILY.iter().map(|member| member.name).collect();
        assert_eq!(
            names,
            [
                "noImplicitAny",
                "noImplicitThis",
                "strictNullChecks",
                "strictFunctionTypes",
                "strictBindCallApply",
                "strictPropertyInitialization",
                "strictBuiltinIteratorReturn",
                "useUnknownInCatchVariables",
            ]
        );
        assert!(!names.contains(&"alwaysStrict"));
        assert!(!names.contains(&"noImplicitReturns"));
    }

    /// Issue #3861 ordering: `strict: false` plus an explicit member keeps
    /// the explicit member while contracting the rest of the family.
    #[test]
    fn strict_false_then_explicit_member_true_keeps_member() {
        let mut options = CheckerOptions::default();
        apply_strict_family(
            &mut options,
            &StrictFamilyOverrides {
                strict: Some(false),
                strict_null_checks: Some(true),
                ..Default::default()
            },
        );

        assert!(!options.strict);
        assert!(options.strict_null_checks, "explicit member wins (#3861)");
        assert!(!options.no_implicit_any);
        assert!(!options.no_implicit_this);
        assert!(!options.strict_function_types);
        assert!(!options.strict_bind_call_apply);
        assert!(!options.strict_property_initialization);
        assert!(!options.strict_builtin_iterator_return);
        assert!(!options.use_unknown_in_catch_variables);
    }

    /// Issue #3861 ordering, inverse permutation: `strict: true` plus an
    /// explicit `false` member keeps the member off.
    #[test]
    fn strict_true_then_explicit_member_false_keeps_member_off() {
        let mut options = CheckerOptions::default();
        apply_strict_family(
            &mut options,
            &StrictFamilyOverrides {
                strict: Some(true),
                strict_function_types: Some(false),
                ..Default::default()
            },
        );

        assert!(options.strict);
        assert!(
            !options.strict_function_types,
            "explicit member wins (#3861)"
        );
        assert!(options.strict_null_checks);
        assert!(options.no_implicit_any);
    }

    /// With no `strict` umbrella, explicit members still apply and the rest
    /// of the family keeps the existing values.
    #[test]
    fn no_umbrella_applies_only_explicit_members() {
        let mut options = CheckerOptions {
            strict_null_checks: false,
            no_implicit_any: false,
            ..CheckerOptions::default()
        };
        apply_strict_family(
            &mut options,
            &StrictFamilyOverrides {
                strict_null_checks: Some(true),
                ..Default::default()
            },
        );

        assert!(options.strict, "untouched: umbrella not provided");
        assert!(options.strict_null_checks, "explicit member applied");
        assert!(!options.no_implicit_any, "non-provided member untouched");
    }

    /// Empty overrides are a no-op.
    #[test]
    fn empty_overrides_are_a_no_op() {
        let mut options = CheckerOptions {
            strict: false,
            strict_null_checks: false,
            ..CheckerOptions::default()
        };
        apply_strict_family(&mut options, &StrictFamilyOverrides::default());

        assert!(!options.strict);
        assert!(!options.strict_null_checks);
        assert!(options.no_implicit_any);
    }

    /// tsc 6.0: `alwaysStrict` is not a strict-family member
    /// (`computedOptions.alwaysStrict` resolves `alwaysStrict !== false`
    /// independent of `strict`), so neither umbrella direction touches it.
    #[test]
    fn always_strict_is_independent_of_strict() {
        let mut options = CheckerOptions {
            always_strict: false,
            ..CheckerOptions::default()
        };
        expand_strict(&mut options, true);
        assert!(
            !options.always_strict,
            "strict: true must not force alwaysStrict (tsc 6.0)"
        );

        let mut options = CheckerOptions::default();
        assert!(options.always_strict, "tsc 6.0 default: alwaysStrict on");
        expand_strict(&mut options, false);
        assert!(
            options.always_strict,
            "strict: false must not reset alwaysStrict (tsc 6.0)"
        );
    }

    /// Non-family flags are never touched by the umbrella.
    #[test]
    fn expand_strict_false_leaves_non_family_flags() {
        let mut options = CheckerOptions {
            no_implicit_returns: true,
            exact_optional_property_types: true,
            no_unchecked_indexed_access: true,
            ..CheckerOptions::default()
        };
        expand_strict(&mut options, false);

        assert!(!options.strict);
        assert!(!options.strict_null_checks);
        assert!(options.no_implicit_returns);
        assert!(options.exact_optional_property_types);
        assert!(options.no_unchecked_indexed_access);
    }
}
