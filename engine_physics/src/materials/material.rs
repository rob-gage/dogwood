// Copyright Rob Gage 2026

use super::MaterialIdentifier;
use engine_graphics::MaterialAppearance;

/// A material
pub enum Material {
    /// A static cellular material
    CellularStatic {
        /// The name of this `Material`
        name: String,
        /// The graphics information for this `Material`
        graphics: MaterialAppearance,
        /// Load below which this material takes no structural damage
        pressure_ignore_threshold: f32,
        /// Structural integrity assigned to newly placed/generated cells
        default_integrity: f32,
        /// Optional dynamic material produced when this cell fractures
        debris_material: Option<MaterialIdentifier>,
        /// Fraction of failed matter that becomes debris
        debris_yield_rate: f32,
        /// Fraction of pressure that may leave this material each solver iteration
        pressure_transmission: f32,
        /// Tangential contact damping for cellular mechanics
        friction: f32,
        /// Normal contact bounce retained by cellular mechanics
        restitution: f32,
    },
    /// A dynamic cellular material
    CellularDynamic {
        /// The name of this `Material`
        name: String,
        /// The graphics information for this `Material`
        graphics: MaterialAppearance,
        /// Effective per-cell mass used by cellular pressure response
        mass: f32,
        /// Fraction of pressure that may leave this material each solver iteration
        pressure_transmission: f32,
        /// Tangential contact damping for cellular mechanics
        friction: f32,
        /// Normal contact bounce retained by cellular mechanics
        restitution: f32,
    },
    /// A fluid material
    Fluid {
        /// The name of this `Material`
        name: String,
        /// The graphics information for this `Material`
        graphics: MaterialAppearance,
        /// Fraction of pressure that may leave this material each solver iteration
        pressure_transmission: f32,
        /// Tangential damping at solid contacts
        friction: f32,
        /// Normal velocity retained as bounce at solid contacts
        restitution: f32,
        /// PBF rest density in normalized particle units
        rest_density: f32,
        /// Tensile-instability correction strength
        artificial_pressure: f32,
        /// Small neighbor velocity smoothing coefficient
        xsph_smoothing: f32,
        /// Maximum normal velocity imparted by a moving body, in cells/sec
        body_push_speed: f32,
        /// Physical density used by buoyancy
        density: f32,
        /// Physical viscosity used by actor drag and swimmer entrainment
        viscosity: f32,
    },
    /// A gaseous species transported by the shared Eulerian gas mixture
    Gas {
        /// The name of this `Material`
        name: String,
        /// The graphics information for this `Material`
        graphics: MaterialAppearance,
        /// Reference density relative to the implicit ambient atmosphere
        density: f32,
        /// Species mixing beyond semi-Lagrangian numerical diffusion
        diffusivity: f32,
        /// Optical depth contributed by unit concentration
        extinction: f32,
        /// Exponential concentration decay rate per second; zero preserves the species
        dissipation: f32,
    },
}

impl Material {

    /// Returns the graphics information for this `Material`
    pub const fn appearance(&self) -> &MaterialAppearance {
        match self {
            Self::CellularStatic { graphics, .. } => graphics,
            Self::CellularDynamic { graphics, .. } => graphics,
            Self::Fluid { graphics, .. } => graphics,
            Self::Gas { graphics, .. } => graphics,
        }
    }

    /// Returns the name of this `Material`
    pub fn name(&self) -> &str {
        match self {
            Material::CellularStatic { name, .. } => name,
            Material::CellularDynamic { name, .. } => name,
            Material::Fluid { name, .. } => name,
            Material::Gas { name, .. } => name,
        }
    }

}
