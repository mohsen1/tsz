mod bang_module_reference;
mod system_emit;
mod wrapper_entry;

use std::collections::HashMap;

#[derive(Clone, Default)]
pub(super) struct SystemDependencyPlan {
    pub actions: HashMap<String, Vec<SystemDependencyAction>>,
    pub import_vars: HashMap<u32, String>,
}

#[derive(Clone)]
pub(super) enum SystemDependencyAction {
    Assign(String),
    NamedExports(Vec<(String, String)>),
    NamespaceExport(String),
}
