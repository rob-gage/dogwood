// Copyright Rob Gage 2026

use crate::{materials::MaterialIdentifier, scenes::{
    cellular_tiles::CellCoordinates,
    ScenePosition,
    SceneRigidBody,
}};

/// One particle of a new soft body
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SoftBodyParticleDescriptor {
    /// Starting position
    pub position: ScenePosition,
    /// Mass in the same units as actor and rigid body masses
    pub mass: f32,
}

/// Holds a particle to something outside its body
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SoftBodyAnchor {
    /// Fixed at its position for as long as a static cell stays solid
    Cell {
        /// The anchored particle
        particle: u32,
        /// The supporting cell
        cell: CellCoordinates,
    },
    /// Carried by a rigid body, which receives the pull in return
    RigidBody {
        /// The anchored particle
        particle: u32,
        /// The carrying rigid body
        rigid_body: SceneRigidBody,
    },
}

impl SoftBodyAnchor {

    /// Returns the anchored particle
    pub const fn particle(self) -> u32 {
        match self {
            Self::Cell { particle, .. } | Self::RigidBody { particle, .. } => particle,
        }
    }

}

/// One static cell of a soft body
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SoftBodyCell {
    /// Cells to the right of the way from the segment's first particle to its second
    pub x: i32,
    /// Cells along the way from the segment's first particle to its second
    pub y: i32,
    /// Static cellular material, which reacts, burns, and takes damage like any other cell
    pub material: MaterialIdentifier,
    /// Packed color
    pub color: u32,
    /// Packed radiance
    pub radiance: u32,
}

/// The cells carried between two neighboring particles. Cells live in the cell grid while the body
/// is in the active region; a segment parts once no connected path of its cells joins its ends
#[derive(Clone, Debug, PartialEq)]
pub struct SoftBodySegmentDescriptor {
    /// Particle at the segment's origin
    pub first: u32,
    /// Particle the segment's rows run toward
    pub second: u32,
    /// Rows from the first particle to the second
    pub length: i32,
    /// Its cells, including any that hang off its path
    pub cells: Vec<SoftBodyCell>,
}

/// A soft body to create: particles joined by any arrangement of edges, which may form open
/// chains, branches, or closed loops
#[derive(Clone, Debug, PartialEq)]
pub struct SoftBodyDescriptor {
    /// Particles
    pub particles: Vec<SoftBodyParticleDescriptor>,
    /// Particle pairs held at their starting distance
    pub edges: Vec<[u32; 2]>,
    /// Particle pairs resisting bending, normally next-nearest neighbors
    pub bend_edges: Vec<[u32; 2]>,
    /// Particle pairs that collide with actors, an open or closed outline of the body
    pub surface: Vec<[u32; 2]>,
    /// Anchors of particles
    pub anchors: Vec<SoftBodyAnchor>,
    /// Static cells carried between particles
    pub segments: Vec<SoftBodySegmentDescriptor>,
    /// Fewest cells of a piece, below which its cells become their material's debris or nothing
    pub minimum_cell_count: usize,
    /// Kelvin of its cells when created
    pub cell_temperature: f32,
    /// Whether actors are stopped by this body, instead of pushing through it
    pub blocks_actors: bool,
    /// Natural frequency of the edges in hertz; higher is stiffer
    pub edge_frequency: f32,
    /// Natural frequency of the bending edges in hertz; lower bends more freely
    pub bend_frequency: f32,
    /// Fraction of velocity lost per second
    pub damping: f32,
    /// Fraction of a particle's motion along a solid cell lost at each contact
    pub friction: f32,
    /// Share of scene gravity applied to the particles
    pub gravity_scale: f32,
    /// Thickness of particles against actors and rigid bodies in tiles
    pub radius: f32,
    /// Stretch, as a fraction of rest length, past which an edge tears, or `None` for never
    pub tear_strain: Option<f32>,
    /// Whether the enclosed area is preserved, for a closed surface
    pub preserves_area: bool,
    /// Acceleration in tiles per second squared per atmosphere per cell of pressure gradient
    pub pressure_response: f32,
    /// Acceleration in tiles per second squared per tile per second of gas flow past a particle
    pub gas_response: f32,
    /// Rate per second at which a submerged particle takes on the velocity of the fluid
    pub fluid_drag: f32,
    /// Fraction of gravity cancelled while fully submerged
    pub buoyancy: f32,
}

impl SoftBodyDescriptor {

    /// Returns a chain of particles with default properties, whose neighbors are joined by edges
    /// and whose next-nearest neighbors resist bending
    pub fn chain(particles: Vec<SoftBodyParticleDescriptor>) -> Self {
        let count: u32 = particles.len() as u32;
        let edges: Vec<[u32; 2]> = (0..count.saturating_sub(1)).map(|i| [i, i + 1]).collect();
        Self {
            particles,
            surface: edges.clone(),
            edges,
            bend_edges: (0..count.saturating_sub(2)).map(|i| [i, i + 2]).collect(),
            anchors: Vec::new(),
            segments: Vec::new(),
            minimum_cell_count: 12,
            cell_temperature: 293.15,
            blocks_actors: false,
            edge_frequency: 120.0,
            bend_frequency: 4.0,
            damping: 0.3,
            friction: 0.5,
            gravity_scale: 1.0,
            radius: 0.06,
            tear_strain: None,
            preserves_area: false,
            pressure_response: 0.0,
            gas_response: 0.0,
            fluid_drag: 0.0,
            buoyancy: 0.0,
        }
    }

}
