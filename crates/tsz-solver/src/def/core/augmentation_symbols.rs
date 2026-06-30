use super::{DefId, DefinitionStore};
use std::sync::OnceLock;

pub(crate) fn module_augmentation_symbol_edge_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_MODULE_AUG_SYMBOL_EDGE").is_ok_and(|v| v == "1"))
}

impl DefinitionStore {
    pub fn register_module_augmentation_symbol_def(&self, symbol_id: u32, def_id: DefId) {
        if self.find_def_by_symbol(symbol_id).is_some() {
            return;
        }
        self.insert_symbol_only_mapping(symbol_id, def_id);
        self.bump_generation();
    }

    pub fn register_module_augmentation_symbol_def_if_enabled(
        &self,
        symbol_id: u32,
        def_id: DefId,
    ) {
        if module_augmentation_symbol_edge_enabled() {
            self.register_module_augmentation_symbol_def(symbol_id, def_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_augmentation_symbol_def_registers_missing_edge() {
        let store = DefinitionStore::new();
        let def_id = DefId(42);

        store.register_module_augmentation_symbol_def(100, def_id);

        assert_eq!(store.find_def_by_symbol(100), Some(def_id));
    }

    #[test]
    fn module_augmentation_symbol_def_keeps_first_edge() {
        let store = DefinitionStore::new();
        let first = DefId(42);
        let second = DefId(43);

        store.register_module_augmentation_symbol_def(100, first);
        store.register_module_augmentation_symbol_def(100, second);

        assert_eq!(store.find_def_by_symbol(100), Some(first));
    }
}
