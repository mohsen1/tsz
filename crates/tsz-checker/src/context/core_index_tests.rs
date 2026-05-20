use std::sync::Arc;
use tsz_binder::{BinderState, ModuleAugmentation, SymbolId};
use tsz_parser::parser::NodeIndex;

type ModuleAugsIndex = rustc_hash::FxHashMap<String, Vec<(usize, ModuleAugmentation)>>;
type AugTargetsIndex = rustc_hash::FxHashMap<String, Vec<(SymbolId, usize)>>;

fn build_module_augmentation_indices(
    binders: &[Arc<BinderState>],
) -> (ModuleAugsIndex, AugTargetsIndex) {
    use rustc_hash::FxHashMap;
    let mut module_augs_index: FxHashMap<String, Vec<(usize, ModuleAugmentation)>> =
        FxHashMap::default();
    let mut aug_targets_index: FxHashMap<String, Vec<(SymbolId, usize)>> = FxHashMap::default();
    for (file_idx, binder) in binders.iter().enumerate() {
        for (module_spec, augmentations) in binder.module_augmentations.iter() {
            module_augs_index
                .entry(module_spec.clone())
                .or_default()
                .extend(augmentations.iter().map(|aug| (file_idx, aug.clone())));
        }
        for (&sym_id, module_spec) in binder.augmentation_target_modules.iter() {
            aug_targets_index
                .entry(module_spec.clone())
                .or_default()
                .push((sym_id, file_idx));
        }
    }
    (module_augs_index, aug_targets_index)
}

#[test]
fn global_module_augmentations_index_merges_across_binders() {
    let mut binder1 = BinderState::new();
    std::sync::Arc::make_mut(&mut binder1.module_augmentations).insert(
        "./module-a".to_string(),
        vec![ModuleAugmentation::new(
            "MyInterface".to_string(),
            NodeIndex(10),
        )],
    );
    let mut binder2 = BinderState::new();
    std::sync::Arc::make_mut(&mut binder2.module_augmentations).insert(
        "./module-a".to_string(),
        vec![ModuleAugmentation::new(
            "MyOtherInterface".to_string(),
            NodeIndex(20),
        )],
    );

    let binders = vec![Arc::new(binder1), Arc::new(binder2)];
    let (aug_index, _) = build_module_augmentation_indices(&binders);

    let entries = &aug_index["./module-a"];
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, 0); // file_idx 0
    assert_eq!(entries[0].1.name, "MyInterface");
    assert_eq!(entries[1].0, 1); // file_idx 1
    assert_eq!(entries[1].1.name, "MyOtherInterface");
}

#[test]
fn global_module_augmentations_index_separates_module_specifiers() {
    let mut binder = BinderState::new();
    std::sync::Arc::make_mut(&mut binder.module_augmentations).insert(
        "./module-a".to_string(),
        vec![ModuleAugmentation::new("Foo".to_string(), NodeIndex(10))],
    );
    std::sync::Arc::make_mut(&mut binder.module_augmentations).insert(
        "./module-b".to_string(),
        vec![ModuleAugmentation::new("Bar".to_string(), NodeIndex(20))],
    );

    let binders = vec![Arc::new(binder)];
    let (aug_index, _) = build_module_augmentation_indices(&binders);

    assert!(aug_index.contains_key("./module-a"));
    assert!(aug_index.contains_key("./module-b"));
    assert!(!aug_index.contains_key("./module-c"));
}

#[test]
fn global_augmentation_targets_index_maps_module_to_symbols() {
    let mut binder1 = BinderState::new();
    std::sync::Arc::make_mut(&mut binder1.augmentation_target_modules)
        .insert(SymbolId(100), "./target".to_string());
    let mut binder2 = BinderState::new();
    std::sync::Arc::make_mut(&mut binder2.augmentation_target_modules)
        .insert(SymbolId(200), "./target".to_string());
    std::sync::Arc::make_mut(&mut binder2.augmentation_target_modules)
        .insert(SymbolId(201), "./other".to_string());

    let binders = vec![Arc::new(binder1), Arc::new(binder2)];
    let (_, targets_index) = build_module_augmentation_indices(&binders);

    let target_entries = &targets_index["./target"];
    assert_eq!(target_entries.len(), 2);
    assert_eq!(target_entries[0], (SymbolId(100), 0));
    assert_eq!(target_entries[1], (SymbolId(200), 1));

    let other_entries = &targets_index["./other"];
    assert_eq!(other_entries.len(), 1);
    assert_eq!(other_entries[0], (SymbolId(201), 1));
}

#[test]
fn global_augmentation_indices_empty_for_no_augmentations() {
    let binder = BinderState::new();
    let binders = vec![Arc::new(binder)];
    let (aug_index, targets_index) = build_module_augmentation_indices(&binders);

    assert!(aug_index.is_empty());
    assert!(targets_index.is_empty());
}
