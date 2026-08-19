//! `TypeApplicationEvalCache` implementation for the raw [`TypeInterner`]
//! backend. Split out of `db.rs` to keep that shard under the file-size cap.

use crate::caches::db::TypeApplicationEvalCache;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::types::{TypeId, TypeParamInfo};
use std::sync::Arc;

impl TypeApplicationEvalCache for TypeInterner {
    fn provisional_class_instance(&self, type_id: TypeId) -> Option<(DefId, Arc<[TypeParamInfo]>)> {
        TypeInterner::provisional_class_instance(self, type_id)
    }

    fn register_provisional_class_instance(
        &self,
        type_id: TypeId,
        def_id: DefId,
        params: Arc<[TypeParamInfo]>,
    ) {
        TypeInterner::register_provisional_class_instance(self, type_id, def_id, params);
    }

    fn unregister_provisional_class_instances_for_def(&self, def_id: DefId) {
        TypeInterner::unregister_provisional_class_instances_for_def(self, def_id);
    }

    // #14345: the interner backs the project-wide instantiation cache.
    fn lookup_proto_instantiation_cache(
        &self,
        key: &crate::caches::instantiation_cache::InstantiationCacheKey,
    ) -> Option<TypeId> {
        self.proto_instantiation_memo(key)
    }

    fn insert_proto_instantiation_cache(
        &self,
        key: crate::caches::instantiation_cache::InstantiationCacheKey,
        result: TypeId,
    ) {
        self.set_proto_instantiation_memo(key, result);
    }
}
