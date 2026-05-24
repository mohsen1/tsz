use super::Printer;
use tsz_parser::parser::NodeIndex;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn prepare_es5_namespace_directive(
        &mut self,
        namespace_node: NodeIndex,
        should_declare_var: bool,
    ) -> (String, bool) {
        let Some(ns_node) = self.arena.get(namespace_node) else {
            return (String::new(), should_declare_var);
        };
        let Some(ns_data) = self.arena.get_module(ns_node) else {
            return (String::new(), should_declare_var);
        };
        let Some(ns_name) = self.get_module_root_name(ns_data.name) else {
            return (String::new(), should_declare_var);
        };

        let should_declare_var =
            should_declare_var && !self.declared_namespace_names.contains(&ns_name);
        self.declared_namespace_names.insert(ns_name.clone());
        (ns_name, should_declare_var)
    }
}
