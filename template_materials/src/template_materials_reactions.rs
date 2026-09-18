// Copyright Rob Gage 2026

use crate::engine::physics::materials::{
    MaterialIdentifier, MaterialReaction, MaterialReactionProduct, MaterialReactionReactant,
    MaterialReference, MaterialRegistryBuilder,
};

pub(super) fn register_reactions(
    materials: &mut MaterialRegistryBuilder,
    fire: MaterialIdentifier,
    smoke: MaterialIdentifier,
    natural_gas: MaterialIdentifier,
    blasting_powder: MaterialIdentifier,
    acid: MaterialIdentifier,
    acid_gas: MaterialIdentifier,
    water: MaterialIdentifier,
    acid_sludge: MaterialIdentifier,
    sand: MaterialIdentifier,
    stone: MaterialIdentifier,
    stone_debris: MaterialIdentifier,
    oil: MaterialIdentifier,
    glass: MaterialIdentifier,
    broken_glass: MaterialIdentifier,
    coal: MaterialIdentifier,
) -> Result<(), String> {
    let exact: fn(MaterialIdentifier, f32) -> Option<MaterialReactionReactant> =
        |material: MaterialIdentifier, amount: f32| {
            Some(MaterialReactionReactant {
                selector: MaterialReference::Material(material),
                amount,
            })
        };
    let tag: fn(&str, f32) -> Option<MaterialReactionReactant> = |name: &str, amount: f32| {
        Some(MaterialReactionReactant {
            selector: MaterialReference::Tag(name.into()),
            amount,
        })
    };
    let product: fn(MaterialIdentifier, f32) -> Option<MaterialReactionProduct> =
        |material: MaterialIdentifier, amount: f32| {
            Some(MaterialReactionProduct { material, amount })
        };
    materials.register_reaction(MaterialReaction {
        reactants: [exact(fire, 0.08), tag("flammable", 1.0)],
        products: [product(fire, 0.08), product(smoke, 0.12)],
        minimum_air: Some(0.02),
        maximum_extent_per_tick: 0.08,
        thermal_energy: 18.0,
        priority: 80,
        ..Default::default()
    });
    materials.register_reaction(MaterialReaction {
        reactants: [exact(natural_gas, 1.0), None],
        products: [product(fire, 0.18), product(smoke, 0.25)],
        minimum_temperature: Some(430.0),
        minimum_air: Some(0.02),
        maximum_extent_per_tick: 0.35,
        thermal_energy: 95.0,
        pressure_output: 3.0,
        priority: 90,
        ..Default::default()
    });
    materials.register_reaction(MaterialReaction {
        reactants: [exact(blasting_powder, 1.0), None],
        products: [product(fire, 0.2), product(smoke, 0.2)],
        minimum_temperature: Some(420.0),
        maximum_extent_per_tick: 0.8,
        thermal_energy: 160.0,
        pressure_output: 80.0,
        priority: 100,
        ..Default::default()
    });
    materials.register_reaction(MaterialReaction {
        reactants: [exact(blasting_powder, 1.0), None],
        products: [product(fire, 0.2), product(smoke, 0.2)],
        minimum_pressure: Some(12.0),
        maximum_extent_per_tick: 0.8,
        thermal_energy: 180.0,
        pressure_output: 100.0,
        priority: 101,
        ..Default::default()
    });
    materials.register_reaction(MaterialReaction {
        reactants: [exact(acid, 0.2), tag("corrodable", 1.0)],
        products: [None, None],
        maximum_extent_per_tick: 0.15,
        thermal_energy: 0.01,
        priority: 20,
        ..Default::default()
    });
    materials.register_reaction(MaterialReaction {
        reactants: [exact(acid_gas, 0.2), tag("corrodable", 1.0)],
        products: [None, None],
        maximum_extent_per_tick: 0.008,
        thermal_energy: 0.002,
        priority: 20,
        ..Default::default()
    });
    materials.register_reaction(MaterialReaction {
        reactants: [exact(acid, 1.0), exact(water, 1.0)],
        products: [product(acid_sludge, 1.0), None],
        maximum_extent_per_tick: 0.12,
        priority: 30,
        ..Default::default()
    });
    materials.register_reaction(MaterialReaction {
        reactants: [exact(sand, 1.0), exact(acid_sludge, 1.0)],
        products: [product(blasting_powder, 1.0), None],
        minimum_temperature: Some(500.0),
        maximum_temperature: Some(1500.0),
        maximum_extent_per_tick: 1.0,
        priority: 30,
        ..Default::default()
    });
    for (material, name) in [
        (coal, "flammable"),
        (oil, "flammable"),
        (natural_gas, "flammable"),
        (blasting_powder, "flammable"),
        (stone, "corrodable"),
        (stone_debris, "corrodable"),
        (sand, "corrodable"),
        (glass, "corrodable"),
        (broken_glass, "corrodable"),
        (coal, "corrodable"),
        (blasting_powder, "corrodable"),
    ] {
        materials
            .tag(material, name)
            .map_err(|error| error.to_string())?;
    }
    // All identifiers now exist, so transition metadata can be compiled
    // without relying on registration order.
    Ok(())
}
