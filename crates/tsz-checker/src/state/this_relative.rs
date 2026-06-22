use super::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{TypeData, TypeId};

impl CheckerState<'_> {
    /// Choose the binding for `this`-at-return-position substitution given a
    /// call's receiver type.
    ///
    /// When the receiver is itself a *compound* `this`-relative type (e.g.
    /// `this.stack: this[]` accessed inside the class body), the return type's
    /// `this` is the *same* polymorphic `this` and must stay polymorphic;
    /// binding it to the this-bearing receiver would spuriously nest it
    /// (`pop(): this` would become `this[]`, drawing a false TS2322). Falling
    /// back to the canonical `this` makes the substitution an identity for
    /// `this` positions, preserving the polymorphic `this`. A receiver that is
    /// not this-relative (or is exactly `this`) is used verbatim.
    pub(crate) fn this_substitution_target_for_receiver(
        &self,
        receiver_expr: NodeIndex,
        receiver_type: TypeId,
    ) -> TypeId {
        if self.receiver_expr_is_this_relative(receiver_expr)
            && self.type_is_compound_this_relative(receiver_type)
        {
            self.ctx.types.this_type()
        } else {
            receiver_type
        }
    }

    /// True when `type_id` has a structural wrapper over the polymorphic
    /// `this` — e.g. `this[]`, `this | undefined`, or `Foo & this`.
    ///
    /// This intentionally walks only type-constructor surfaces. It must not
    /// inspect object members, lazy class/interface bodies, or type-parameter
    /// constraints: those can mention `this` without making the receiver itself
    /// a `this`-relative wrapper. Substituting a member's `this` with a real
    /// wrapper would nest the polymorphic `this` one level too deep, so the
    /// receiver-`this` binding sites treat those wrappers as "leave `this`
    /// polymorphic" rather than rebinding (issue #14512). A bare `this` is
    /// exempt: substituting `this -> this` is an identity no-op, so the existing
    /// direct-`this`-receiver paths are preserved.
    pub(crate) fn type_is_compound_this_relative(&self, type_id: TypeId) -> bool {
        type_id != self.ctx.types.this_type()
            && self.type_has_surface_this_relative_wrapper(type_id)
    }

    fn type_has_surface_this_relative_wrapper(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }

        let this_type = self.ctx.types.this_type();
        let mut stack = vec![type_id];
        let mut fuel = 64usize;
        while let Some(current) = stack.pop() {
            if current == this_type {
                return true;
            }
            if current.is_intrinsic() || fuel == 0 {
                continue;
            }
            fuel -= 1;

            match self.ctx.types.lookup(current) {
                Some(TypeData::Array(element))
                | Some(TypeData::ReadonlyType(element))
                | Some(TypeData::NoInfer(element))
                | Some(TypeData::KeyOf(element)) => stack.push(element),
                Some(TypeData::Tuple(elements)) => {
                    stack.extend(
                        self.ctx
                            .types
                            .tuple_list(elements)
                            .iter()
                            .map(|element| element.type_id),
                    );
                }
                Some(TypeData::Union(list) | TypeData::Intersection(list)) => {
                    stack.extend(self.ctx.types.type_list(list).iter().copied());
                }
                Some(TypeData::Application(application)) => {
                    stack.extend(
                        self.ctx
                            .types
                            .type_application(application)
                            .args
                            .iter()
                            .copied(),
                    );
                }
                Some(TypeData::IndexAccess(object, index)) => {
                    stack.push(object);
                    stack.push(index);
                }
                Some(TypeData::StringIntrinsic { type_arg, .. }) => stack.push(type_arg),
                Some(TypeData::Substitution {
                    base_type,
                    constraint,
                }) => {
                    stack.push(base_type);
                    stack.push(constraint);
                }
                _ => {}
            }
        }
        false
    }

    pub(crate) fn receiver_expr_is_this_relative(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..32 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            match node.kind {
                kind if kind == SyntaxKind::ThisKeyword as u16 => return true,
                kind if kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
                {
                    let Some(access) = self.ctx.arena.get_access_expr(node) else {
                        return false;
                    };
                    current = access.expression;
                }
                _ => return false,
            }
        }
        false
    }
}
