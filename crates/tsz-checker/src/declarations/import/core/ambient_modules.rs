use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    pub(super) fn any_ambient_module_declared(&self, module_name: &str) -> bool {
        let normalized = module_name.trim_matches('"').trim_matches('\'');

        // Use the pre-built global index for O(1) exact lookup + a small
        // tsc-faithful wildcard-pattern scan.
        if let Some(declared) = &self.ctx.global_declared_modules {
            if declared.exact.contains(normalized) {
                return true;
            }
            return declared.matches_wildcard(normalized);
        }

        let Some(all_binders) = &self.ctx.all_binders else {
            return false;
        };
        for binder in all_binders.iter() {
            for pattern in binder
                .declared_modules
                .iter()
                .chain(binder.shorthand_ambient_modules.iter())
                .chain(binder.module_exports.keys())
            {
                if crate::context::ambient_pattern_matches(pattern, normalized) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn wildcard_ambient_module_declared(&self, module_name: &str) -> bool {
        let normalized = module_name.trim_matches('"').trim_matches('\'');

        if let Some(declared) = &self.ctx.global_declared_modules {
            return declared.matches_wildcard(normalized);
        }

        let Some(all_binders) = &self.ctx.all_binders else {
            return false;
        };
        for binder in all_binders.iter() {
            for pattern in binder
                .declared_modules
                .iter()
                .chain(binder.shorthand_ambient_modules.iter())
                .chain(binder.module_exports.keys())
                .filter(|pattern| pattern.contains('*'))
            {
                if crate::context::ambient_pattern_matches(pattern, normalized) {
                    return true;
                }
            }
        }
        false
    }
}
