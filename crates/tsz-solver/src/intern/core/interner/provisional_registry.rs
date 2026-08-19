//! Provisional (mid-build) class-instance registry accessors (#16055).
//!
//! The registry maps a partial class-instance snapshot `TypeId` — installed
//! by a checker while the class's own instance/constructor resolution window
//! is open — to the class's `DefId` and declared type parameters. While an
//! entry is registered, application evaluation keeps `C[args]` opaque instead
//! of materializing from the partial body, so no durable composite interns a
//! degraded snapshot-derived object beside the completed representation.

use super::TypeInterner;
use crate::def::DefId;
use crate::types::{TypeId, TypeParamInfo};
use std::sync::Arc;

impl TypeInterner {
    /// Look up whether `type_id` is a registered provisional (mid-build) class
    /// instance snapshot (#16055). Returns the class's `DefId` and declared
    /// type parameters when it is.
    #[inline]
    pub fn provisional_class_instance(
        &self,
        type_id: TypeId,
    ) -> Option<(DefId, Arc<[TypeParamInfo]>)> {
        if self.provisional_class_instance_registry.is_empty() {
            return None;
        }
        self.provisional_class_instance_registry
            .get(&type_id)
            .map(|v| (v.0, Arc::clone(&v.1)))
    }

    /// Register `type_id` as the provisional (mid-build) instance snapshot of
    /// the class identified by `def_id` (#16055).
    #[inline]
    pub fn register_provisional_class_instance(
        &self,
        type_id: TypeId,
        def_id: DefId,
        params: Arc<[TypeParamInfo]>,
    ) {
        self.provisional_class_instance_registry
            .insert(type_id, (def_id, params));
    }

    /// Drop every provisional class-instance registration for `def_id`. Called
    /// when the class publishes its completed instance: from that point the
    /// class's applications materialize normally.
    #[inline]
    pub fn unregister_provisional_class_instances_for_def(&self, def_id: DefId) {
        if self.provisional_class_instance_registry.is_empty() {
            return;
        }
        self.provisional_class_instance_registry
            .retain(|_, (def, _)| *def != def_id);
    }
}
