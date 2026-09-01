// Evidence tests for #13103: per-file and scratch emitters share one
// `Arc<TypeCacheView>` (and the program-wide path maps) instead of
// deep-cloning the view per emitter.

use super::*;
use crate::type_cache_view::TypeCacheView;
use rustc_hash::FxHashSet;

#[test]
fn scratch_emitter_shares_type_cache_view_arc() {
    let mut parser = ParserState::new("test.ts".to_string(), "export const value = 1;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let shared_view = Arc::new(TypeCacheView::default());
    let emitter = DeclarationEmitter::with_shared_type_info(
        &parser.arena,
        Arc::clone(&shared_view),
        &interner,
        &binder,
    );

    let scratch = emitter.scratch_declaration_emitter();
    let parent_view = emitter
        .type_cache
        .as_ref()
        .expect("parent emitter should hold a type cache view");
    let scratch_view = scratch
        .type_cache
        .as_ref()
        .expect("scratch emitter should hold a type cache view");

    // One allocation serves the program view, the per-file emitter, and the
    // scratch emitter: no deep clone of the cache maps.
    assert!(Arc::ptr_eq(&shared_view, parent_view));
    assert!(Arc::ptr_eq(parent_view, scratch_view));
}

#[test]
fn owned_with_type_info_still_constructs_and_shares_with_scratch() {
    let mut parser = ParserState::new("test.ts".to_string(), "export const value = 1;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    // Compatibility path: caller-owned view is wrapped once, then shared.
    let emitter = DeclarationEmitter::with_type_info(
        &parser.arena,
        TypeCacheView::default(),
        &interner,
        &binder,
    );

    let scratch = emitter.scratch_declaration_emitter();
    assert!(Arc::ptr_eq(
        emitter.type_cache.as_ref().expect("parent view"),
        scratch.type_cache.as_ref().expect("scratch view"),
    ));
}

#[test]
fn scratch_emitter_shares_program_wide_path_maps() {
    let mut parser = ParserState::new("test.ts".to_string(), "export const value = 1;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let mut emitter = DeclarationEmitter::with_shared_type_info(
        &parser.arena,
        Arc::new(TypeCacheView::default()),
        &interner,
        &binder,
    );

    let arena_to_path: Arc<FxHashMap<usize, String>> = Arc::new(FxHashMap::default());
    let file_idx_to_path: Arc<FxHashMap<u32, String>> = Arc::new(FxHashMap::default());
    let root_file_paths: Arc<FxHashSet<String>> = Arc::new(FxHashSet::default());
    let files_with_augmentations: Arc<FxHashSet<String>> = Arc::new(FxHashSet::default());
    emitter.set_shared_arena_to_path(Arc::clone(&arena_to_path));
    emitter.set_shared_file_idx_to_path(Arc::clone(&file_idx_to_path));
    emitter.set_shared_root_file_paths(Arc::clone(&root_file_paths));
    emitter.set_shared_files_with_augmentations(Arc::clone(&files_with_augmentations));

    assert!(Arc::ptr_eq(&arena_to_path, &emitter.arena_to_path));
    assert!(Arc::ptr_eq(&file_idx_to_path, &emitter.file_idx_to_path));
    assert!(Arc::ptr_eq(&root_file_paths, &emitter.root_file_paths));
    assert!(Arc::ptr_eq(
        &files_with_augmentations,
        &emitter.files_with_augmentations
    ));

    // Scratch emitters re-share the parent's `arena_to_path` instead of
    // cloning the map contents.
    let scratch = emitter.scratch_declaration_emitter();
    assert!(Arc::ptr_eq(&arena_to_path, &scratch.arena_to_path));
}
