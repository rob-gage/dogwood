// Copyright Rob Gage 2026

use super::{Material, MaterialRegistry};

impl MaterialRegistry {
    /// Returns whether all simulation properties of a material are valid
    pub(super) fn material_is_valid(material: &Material) -> bool {
        match material {
            Material::CellularStatic {
                mass,
                minimum_rigid_body_cell_count,
                pressure_transmission,
                friction,
                restitution,
                ..
            } => {
                mass.is_finite()
                    && *mass > 0.0
                    && *minimum_rigid_body_cell_count > 0
                    && pressure_transmission.is_finite()
                    && (0.0..=1.0).contains(pressure_transmission)
                    && friction.is_finite()
                    && (0.0..=1.0).contains(friction)
                    && restitution.is_finite()
                    && (0.0..=1.0).contains(restitution)
            }
            Material::CellularDynamic {
                mass,
                pressure_transmission,
                friction,
                restitution,
                ..
            } => {
                mass.is_finite()
                    && *mass > 0.0
                    && pressure_transmission.is_finite()
                    && (0.0..=1.0).contains(pressure_transmission)
                    && friction.is_finite()
                    && (0.0..=1.0).contains(friction)
                    && restitution.is_finite()
                    && (0.0..=1.0).contains(restitution)
            }
            Material::Fluid {
                friction,
                restitution,
                rest_density,
                artificial_pressure,
                xsph_smoothing,
                body_push_speed,
                density,
                viscosity,
                ..
            } => {
                friction.is_finite()
                    && (0.0..=1.0).contains(friction)
                    && restitution.is_finite()
                    && (0.0..=1.0).contains(restitution)
                    && rest_density.is_finite()
                    && *rest_density > 0.0
                    && artificial_pressure.is_finite()
                    && *artificial_pressure >= 0.0
                    && xsph_smoothing.is_finite()
                    && (0.0..=1.0).contains(xsph_smoothing)
                    && body_push_speed.is_finite()
                    && *body_push_speed >= 0.0
                    && density.is_finite()
                    && *density > 0.0
                    && viscosity.is_finite()
                    && *viscosity >= 0.0
            }
            Material::Gas {
                density,
                diffusivity,
                extinction,
                dissipation,
                compressibility,
                ..
            } => {
                density.is_finite()
                    && *density > 0.0
                    && diffusivity.is_finite()
                    && *diffusivity >= 0.0
                    && extinction.is_finite()
                    && *extinction >= 0.0
                    && dissipation.is_finite()
                    && *dissipation >= 0.0
                    && compressibility.is_finite()
                    && (0.0..=1.0).contains(compressibility)
            }
        }
    }
}
