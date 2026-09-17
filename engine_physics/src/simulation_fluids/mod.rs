// Copyright Rob Gage 2026

//! Authoritative fluid particles, streaming, and fluid solver resources.

mod fluid_authority_view;
mod fluids;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) use fluid_authority_view::FluidAuthorityView;
pub use fluids::Fluids;
