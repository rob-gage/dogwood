// Copyright Rob Gage 2026

use super::*;

impl MaterialRegistry {
    /// Builds graphics properties for all registered materials
    pub fn build_material_graphics(&self, accelerator: &Accelerator) -> MaterialGraphics {
        MaterialGraphics::new(
            accelerator,
            self.cellular_statics
                .iter()
                .map(|material| *material.appearance())
                .collect(),
            self.cellular_dynamics
                .iter()
                .map(|material| *material.appearance())
                .collect(),
            self.fluids
                .iter()
                .map(|material| *material.appearance())
                .collect(),
            self.gases
                .iter()
                .map(|material| *material.appearance())
                .collect(),
            self.fluids
                .iter()
                .map(|material| match material {
                    Material::Fluid {
                        rest_density,
                        artificial_pressure,
                        xsph_smoothing,
                        body_push_speed,
                        friction,
                        restitution,
                        density,
                        viscosity,
                        ..
                    } => [
                        *rest_density,
                        *artificial_pressure,
                        *xsph_smoothing,
                        *body_push_speed,
                        *friction,
                        *restitution,
                        *density,
                        *viscosity,
                    ],
                    _ => unreachable!(),
                })
                .collect(),
            self.gases
                .iter()
                .map(|material| match material {
                    Material::Gas {
                        density,
                        diffusivity,
                        extinction,
                        dissipation,
                        compressibility,
                        ..
                    } => [
                        *density,
                        *diffusivity,
                        *extinction,
                        *dissipation,
                        *compressibility,
                        0.0,
                        0.0,
                        0.0,
                    ],
                    _ => unreachable!(),
                })
                .collect(),
        )
    }
}
