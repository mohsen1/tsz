//! Target-derived emit facts.
//!
//! Keep target/version decisions in one small value so emit planning can reason
//! about TypeScript 6+ output policy without scattering raw `supports_es*`
//! checks through new code.

use tsz_common::common::ScriptTarget;

/// TypeScript 6+ treats `ES2015` as the strategic floor for JavaScript emit.
pub const TS6_STRATEGIC_EMIT_FLOOR: ScriptTarget = ScriptTarget::ES2015;

/// Feature gates and policy facts derived from one `ScriptTarget`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmitTargetFacts {
    pub target: ScriptTarget,
    /// `true` for targets below the TS6 strategic emit floor.
    pub legacy_below_ts6_floor: bool,
    /// `true` for the removed TS6 `ES3` target.
    pub removed_in_ts6: bool,
    /// `true` for the deprecated TS6 `ES5` target.
    pub deprecated_in_ts6: bool,
    /// Legacy compatibility lane: targets that require ES5-era lowering.
    pub legacy_es5_or_lower: bool,
    pub needs_es2016_lowering: bool,
    pub needs_es2018_lowering: bool,
    pub needs_es2019_lowering: bool,
    pub needs_es2020_lowering: bool,
    pub needs_es2021_lowering: bool,
    pub needs_es2022_lowering: bool,
    pub needs_async_lowering: bool,
    pub supports_using_declarations: bool,
}

impl EmitTargetFacts {
    #[must_use]
    pub const fn from_target(target: ScriptTarget) -> Self {
        Self {
            target,
            legacy_below_ts6_floor: (target as u8) < (TS6_STRATEGIC_EMIT_FLOOR as u8),
            removed_in_ts6: matches!(target, ScriptTarget::ES3),
            deprecated_in_ts6: matches!(target, ScriptTarget::ES5),
            legacy_es5_or_lower: target.is_es5(),
            needs_es2016_lowering: !target.supports_es2016(),
            needs_es2018_lowering: !target.supports_es2018(),
            needs_es2019_lowering: !target.supports_es2019(),
            needs_es2020_lowering: !target.supports_es2020(),
            needs_es2021_lowering: !target.supports_es2021(),
            needs_es2022_lowering: !target.supports_es2022(),
            needs_async_lowering: !target.supports_es2017(),
            supports_using_declarations: target.supports_es2025(),
        }
    }

    #[must_use]
    pub const fn is_ts6_strategic_target(self) -> bool {
        !self.legacy_below_ts6_floor
    }
}

impl Default for EmitTargetFacts {
    fn default() -> Self {
        Self::from_target(ScriptTarget::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn es2015_is_the_ts6_strategic_floor() {
        let es5 = EmitTargetFacts::from_target(ScriptTarget::ES5);
        assert!(es5.legacy_below_ts6_floor);
        assert!(es5.deprecated_in_ts6);
        assert!(es5.legacy_es5_or_lower);

        let es2015 = EmitTargetFacts::from_target(ScriptTarget::ES2015);
        assert!(es2015.is_ts6_strategic_target());
        assert!(!es2015.legacy_es5_or_lower);
    }

    #[test]
    fn es3_is_removed_not_merely_deprecated() {
        let facts = EmitTargetFacts::from_target(ScriptTarget::ES3);
        assert!(facts.removed_in_ts6);
        assert!(!facts.deprecated_in_ts6);
        assert!(facts.legacy_below_ts6_floor);
    }

    #[test]
    fn es2025_preserves_using_declarations() {
        assert!(!EmitTargetFacts::from_target(ScriptTarget::ES2022).supports_using_declarations);
        assert!(EmitTargetFacts::from_target(ScriptTarget::ES2025).supports_using_declarations);
        assert!(EmitTargetFacts::from_target(ScriptTarget::ESNext).supports_using_declarations);
    }
}
