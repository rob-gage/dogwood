// Copyright Rob Gage 2026

//! Runtime material mutation and reaction execution resources.

mod material_mutations;
mod material_reactions;

#[cfg(test)]
pub(crate) mod tests;

pub use material_mutations::MaterialMutations;
pub(crate) use material_reactions::MaterialReactions;
