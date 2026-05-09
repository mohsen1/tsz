use super::{NodeIndex, Printer};
use rustc_hash::FxHashSet;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn push_commonjs_exported_var_parameter_shadow_names(
        &mut self,
        params: &[NodeIndex],
    ) {
        let mut shadow_names = FxHashSet::default();
        for &param_idx in params {
            let Some(param) = self.arena.get_parameter_at(param_idx) else {
                continue;
            };
            let mut names = Vec::new();
            self.collect_binding_names(param.name, &mut names);
            for name in names {
                if self.commonjs_exported_var_names.contains(name.as_str()) {
                    shadow_names.insert(name);
                }
            }
        }
        self.commonjs_exported_var_shadow_stack.push(shadow_names);
    }

    pub(in crate::emitter) fn pop_commonjs_exported_var_parameter_shadow_names(&mut self) {
        self.commonjs_exported_var_shadow_stack.pop();
    }
}
