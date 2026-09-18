// Copyright Rob Gage 2026

use engine_compute::Accelerator;
use engine_graphics::{
    MaterialGraphics, MaterialGraphicsCellularDynamic, MaterialGraphicsCellularStatic,
    MaterialGraphicsFluid, MaterialGraphicsGas,
};

use super::Material;
use super::MaterialRegistry;

impl MaterialRegistry {
    /// Builds graphics properties for all registered materials
    pub fn build_material_graphics(&self, accelerator: &Accelerator) -> MaterialGraphics {
        MaterialGraphics::new(
            accelerator,
            self.cellular_statics
                .iter()
                .map(|material| MaterialGraphicsCellularStatic::from(*material.appearance()))
                .collect(),
            self.cellular_dynamics
                .iter()
                .map(|material| MaterialGraphicsCellularDynamic::from(*material.appearance()))
                .collect(),
            self.fluids
                .iter()
                .map(|material| MaterialGraphicsFluid::from(*material.appearance()))
                .collect(),
            self.gases
                .iter()
                .map(|material| MaterialGraphicsGas::from(*material.appearance()))
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
                        dissipation,
                        compressibility,
                        ..
                    } => [
                        *density,
                        *diffusivity,
                        0.0,
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
