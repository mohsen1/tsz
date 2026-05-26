//! Variance analysis for type parameters.
//!
//! Computes whether a type parameter occurs covariantly, contravariantly,
//! invariantly, or bivariantly within a type. Split out of `infer_resolve`
//! to keep that module under the repository file-size ceiling; this is a
//! cohesive, self-contained concern that only reads the type structure.

use crate::inference::infer::InferenceContext;
use crate::types::{TypeData, TypeId};
use tsz_common::interner::Atom;

#[allow(dead_code)] // Reserved for variance analysis in inference resolution
struct VarianceState<'a> {
    target_param: Atom,
    covariant: &'a mut u32,
    contravariant: &'a mut u32,
}

impl InferenceContext<'_> {
    /// Compute the variance of a type parameter within a type.
    /// Returns (`covariant_count`, `contravariant_count`, `invariant_count`, `bivariant_count`)
    #[allow(dead_code)] // Reserved for variance analysis in inference
    pub fn compute_variance(&self, ty: TypeId, target_param: Atom) -> (u32, u32, u32, u32) {
        let mut covariant = 0u32;
        let mut contravariant = 0u32;
        let invariant = 0u32;
        let bivariant = 0u32;
        let mut state = VarianceState {
            target_param,
            covariant: &mut covariant,
            contravariant: &mut contravariant,
        };

        self.compute_variance_helper(ty, true, &mut state);

        (covariant, contravariant, invariant, bivariant)
    }

    fn compute_variance_helper(
        &self,
        ty: TypeId,
        polarity: bool, // true = covariant, false = contravariant
        state: &mut VarianceState<'_>,
    ) {
        // Intrinsics never reference any type parameter — skip the dyn lookup.
        if ty.is_intrinsic() {
            return;
        }
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info)) if info.name == state.target_param => {
                if polarity {
                    *state.covariant += 1;
                } else {
                    *state.contravariant += 1;
                }
            }
            Some(TypeData::Array(elem)) => {
                self.compute_variance_helper(elem, polarity, state);
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                for elem in elements.iter() {
                    self.compute_variance_helper(elem.type_id, polarity, state);
                }
            }
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.compute_variance_helper(member, polarity, state);
                }
            }
            Some(TypeData::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    // Properties are covariant in their type (read position)
                    self.compute_variance_helper(prop.type_id, polarity, state);
                    // Properties are contravariant in their write type (write position)
                    if prop.write_type != prop.type_id && !prop.readonly {
                        self.compute_variance_helper(prop.write_type, !polarity, state);
                    }
                }
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.compute_variance_helper(prop.type_id, polarity, state);
                    if prop.write_type != prop.type_id && !prop.readonly {
                        self.compute_variance_helper(prop.write_type, !polarity, state);
                    }
                }
                if let Some(index) = shape.string_index.as_ref() {
                    self.compute_variance_helper(index.value_type, polarity, state);
                }
                if let Some(index) = shape.number_index.as_ref() {
                    self.compute_variance_helper(index.value_type, polarity, state);
                }
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                // Variance depends on the generic type definition
                // For now, assume covariant for all type arguments
                for &arg in &app.args {
                    self.compute_variance_helper(arg, polarity, state);
                }
            }
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                // Parameters are contravariant
                for param in &shape.params {
                    self.compute_variance_helper(param.type_id, !polarity, state);
                }
                // Return type is covariant
                self.compute_variance_helper(shape.return_type, polarity, state);
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.get_conditional(cond_id);
                // Conditional types are invariant in their type parameters
                self.compute_variance_helper(cond.check_type, false, state);
                self.compute_variance_helper(cond.extends_type, false, state);
                // But can be either in the result
                self.compute_variance_helper(cond.true_type, polarity, state);
                self.compute_variance_helper(cond.false_type, polarity, state);
            }
            _ => {}
        }
    }

    /// Get the variance of a type parameter as a string.
    #[allow(dead_code)] // Reserved for variance analysis in inference
    pub fn get_variance(&self, ty: TypeId, target_param: Atom) -> &'static str {
        let (covariant, contravariant, invariant, bivariant) =
            self.compute_variance(ty, target_param);

        if invariant > 0 {
            "invariant"
        } else if bivariant > 0 {
            "bivariant"
        } else if covariant > 0 && contravariant > 0 {
            "invariant" // Both covariant and contravariant means invariant
        } else if covariant > 0 {
            "covariant"
        } else if contravariant > 0 {
            "contravariant"
        } else {
            "unused"
        }
    }
}
