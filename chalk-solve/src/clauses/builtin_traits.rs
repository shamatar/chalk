use super::{builder::ClauseBuilder, generalize};
use crate::{CanonicalVarKinds, Interner, RustIrDatabase, TraitRef, WellKnownTrait};
use chalk_ir::{Floundered, Substitution, Ty};

mod clone;
mod copy;
mod discriminant_kind;
mod fn_family;
mod generator;
mod sized;
mod unsize;

/// For well known traits we have special hard-coded impls, either as an
/// optimization or to enforce special rules for correctness.
pub fn add_builtin_program_clauses<I: Interner>(
    db: &dyn RustIrDatabase<I>,
    builder: &mut ClauseBuilder<'_, I>,
    well_known: WellKnownTrait,
    trait_ref: TraitRef<I>,
    binders: &CanonicalVarKinds<I>,
) -> Result<(), Floundered> {
    // If `trait_ref` contains bound vars, we want to universally quantify them.
    // `Generalize` collects them for us.
    let generalized = generalize::Generalize::apply(db.interner(), trait_ref);

    builder.push_binders(generalized, |builder, trait_ref| {
        let self_ty = trait_ref.self_type_parameter(db.interner());
        let ty = self_ty.kind(db.interner()).clone();

        match well_known {
            // There are no builtin impls provided for the following traits:
            WellKnownTrait::Unpin | WellKnownTrait::Drop | WellKnownTrait::CoerceUnsized => (),
            // Built-in traits are non-enumerable.
            _ if self_ty.is_general_var(db.interner(), binders) => return Err(Floundered),
            WellKnownTrait::Sized => {
                sized::add_sized_program_clauses(db, builder, trait_ref, ty, binders)?;
            }
            WellKnownTrait::Copy => {
                copy::add_copy_program_clauses(db, builder, trait_ref, ty, binders)?;
            }
            WellKnownTrait::Clone => {
                clone::add_clone_program_clauses(db, builder, trait_ref, ty, binders)?;
            }
            WellKnownTrait::FnOnce | WellKnownTrait::FnMut | WellKnownTrait::Fn => {
                fn_family::add_fn_trait_program_clauses(db, builder, well_known, self_ty);
            }
            WellKnownTrait::Unsize => {
                unsize::add_unsize_program_clauses(db, builder, trait_ref, ty)
            }
            // DiscriminantKind is automatically implemented for all types
            WellKnownTrait::DiscriminantKind => builder.push_fact(trait_ref),
            WellKnownTrait::Generator => {
                generator::add_generator_program_clauses(db, builder, self_ty)?;
            }
        }
        Ok(())
    })
}

/// Like `add_builtin_program_clauses`, but for `DomainGoal::Normalize` involving
/// a projection (e.g. `<fn(u8) as FnOnce<(u8,)>>::Output`)
pub fn add_builtin_assoc_program_clauses<I: Interner>(
    db: &dyn RustIrDatabase<I>,
    builder: &mut ClauseBuilder<'_, I>,
    well_known: WellKnownTrait,
    self_ty: Ty<I>,
) -> Result<(), Floundered> {
    match well_known {
        WellKnownTrait::FnOnce => {
            // If `self_ty` contains bound vars, we want to universally quantify them.
            // `Generalize` collects them for us.
            let generalized = generalize::Generalize::apply(db.interner(), self_ty);

            builder.push_binders(generalized, |builder, self_ty| {
                fn_family::add_fn_trait_program_clauses(db, builder, well_known, self_ty);
                Ok(())
            })
        }
        WellKnownTrait::DiscriminantKind => {
            discriminant_kind::add_discriminant_clauses(db, builder, self_ty)
        }
        WellKnownTrait::Generator => {
            let generalized = generalize::Generalize::apply(db.interner(), self_ty);

            builder.push_binders(generalized, |builder, self_ty| {
                generator::add_generator_program_clauses(db, builder, self_ty)
            })
        }
        _ => Ok(()),
    }
}

/// Given a trait ref `T0: Trait` and a list of types `U0..Un`, pushes a clause of the form
/// `Implemented(T0: Trait) :- Implemented(U0: Trait) .. Implemented(Un: Trait)`
pub fn needs_impl_for_tys<I: Interner>(
    db: &dyn RustIrDatabase<I>,
    builder: &mut ClauseBuilder<'_, I>,
    trait_ref: TraitRef<I>,
    tys: impl Iterator<Item = Ty<I>>,
) {
    let trait_id = trait_ref.trait_id;

    // The trait must take one parameter (a type)
    debug_assert_eq!(db.trait_datum(trait_id).binders.len(db.interner()), 1,);
    builder.push_clause(
        trait_ref,
        tys.map(|ty| TraitRef {
            trait_id,
            substitution: Substitution::from1(db.interner(), ty),
        }),
    );
}
