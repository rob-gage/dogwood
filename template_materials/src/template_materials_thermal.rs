// Copyright Rob Gage 2026

use crate::engine::physics::materials::{
    MaterialIdentifier, MaterialRegistryBuilder, MaterialThermalProperties,
    MaterialThermalTransition,
};

pub(super) fn register_thermal_properties(
    materials: &mut MaterialRegistryBuilder,
    ice: MaterialIdentifier,
    water: MaterialIdentifier,
    slush: MaterialIdentifier,
    stone: MaterialIdentifier,
    stone_debris: MaterialIdentifier,
    lava: MaterialIdentifier,
    sand: MaterialIdentifier,
    molten_glass: MaterialIdentifier,
    glass: MaterialIdentifier,
    broken_glass: MaterialIdentifier,
    water_vapor: MaterialIdentifier,
    acid: MaterialIdentifier,
    acid_gas: MaterialIdentifier,
    fire: MaterialIdentifier,
    coal: MaterialIdentifier,
    oil: MaterialIdentifier,
    natural_gas: MaterialIdentifier,
    blasting_powder: MaterialIdentifier,
    smoke: MaterialIdentifier,
) -> Result<(), String> {
    materials
        .set_thermal(
            ice,
            MaterialThermalProperties {
                conductivity: 2.2,
                specific_heat_capacity: 2.1,
                default_temperature: Some(263.15),
                cold_transition: None,
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 273.15,
                    target: water,
                    yield_rate: 1.0,
                    latent_energy: 20.0,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            slush,
            MaterialThermalProperties {
                conductivity: 1.6,
                specific_heat_capacity: 2.1,
                default_temperature: Some(268.15),
                cold_transition: None,
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 273.15,
                    target: water,
                    yield_rate: 1.0,
                    latent_energy: 20.0,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            stone,
            MaterialThermalProperties {
                conductivity: 1.4,
                specific_heat_capacity: 0.88,
                default_temperature: Some(293.15),
                cold_transition: None,
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 1473.15,
                    target: lava,
                    yield_rate: 1.0,
                    latent_energy: 120.0,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            stone_debris,
            MaterialThermalProperties {
                conductivity: 1.2,
                specific_heat_capacity: 0.88,
                default_temperature: Some(293.15),
                cold_transition: None,
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 1473.15,
                    target: lava,
                    yield_rate: 1.0,
                    latent_energy: 120.0,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            lava,
            MaterialThermalProperties {
                conductivity: 1.0,
                specific_heat_capacity: 1.1,
                default_temperature: Some(1573.15),
                cold_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 1473.15,
                    target: stone,
                    yield_rate: 1.0,
                    latent_energy: 120.0,
                }),
                hot_transition: None,
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            sand,
            MaterialThermalProperties {
                conductivity: 0.8,
                specific_heat_capacity: 0.83,
                default_temperature: Some(293.15),
                cold_transition: None,
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 1700.0,
                    target: molten_glass,
                    yield_rate: 1.0,
                    latent_energy: 80.0,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            molten_glass,
            MaterialThermalProperties {
                conductivity: 0.7,
                specific_heat_capacity: 1.0,
                default_temperature: Some(1800.0),
                cold_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 1400.0,
                    target: glass,
                    yield_rate: 1.0,
                    latent_energy: 80.0,
                }),
                hot_transition: None,
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            glass,
            MaterialThermalProperties {
                conductivity: 0.9,
                specific_heat_capacity: 0.84,
                default_temperature: Some(293.15),
                cold_transition: None,
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 1400.0,
                    target: molten_glass,
                    yield_rate: 1.0,
                    latent_energy: 80.0,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            broken_glass,
            MaterialThermalProperties {
                conductivity: 0.8,
                specific_heat_capacity: 0.84,
                default_temperature: Some(293.15),
                cold_transition: None,
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 1400.0,
                    target: molten_glass,
                    yield_rate: 1.0,
                    latent_energy: 80.0,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            water,
            MaterialThermalProperties {
                conductivity: 0.6,
                specific_heat_capacity: 4.18,
                default_temperature: Some(293.15),
                cold_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 273.15,
                    target: ice,
                    yield_rate: 1.0,
                    latent_energy: 20.0,
                }),
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 373.15,
                    target: water_vapor,
                    yield_rate: 1.0,
                    latent_energy: 50.0,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            water_vapor,
            MaterialThermalProperties {
                conductivity: 0.025,
                specific_heat_capacity: 2.0,
                default_temperature: Some(393.15),
                cold_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 373.15,
                    target: water,
                    yield_rate: 1.0,
                    latent_energy: 50.0,
                }),
                hot_transition: None,
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            acid,
            MaterialThermalProperties {
                conductivity: 0.55,
                specific_heat_capacity: 3.2,
                default_temperature: Some(293.15),
                cold_transition: None,
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 450.0,
                    target: acid_gas,
                    yield_rate: 1.0,
                    latent_energy: 35.0,
                }),
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            acid_gas,
            MaterialThermalProperties {
                conductivity: 0.03,
                specific_heat_capacity: 1.6,
                default_temperature: Some(500.0),
                cold_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 450.0,
                    target: acid,
                    yield_rate: 1.0,
                    latent_energy: 35.0,
                }),
                hot_transition: None,
            },
        )
        .map_err(|error| error.to_string())?;
    materials
        .set_thermal(
            fire,
            MaterialThermalProperties {
                conductivity: 0.04,
                specific_heat_capacity: 1.2,
                default_temperature: Some(1050.0),
                cold_transition: None,
                hot_transition: None,
            },
        )
        .map_err(|error| error.to_string())?;
    for (material, conductivity, specific_heat_capacity, default_temperature) in [
        (coal, 0.35, 1.5, 293.15),
        (oil, 0.12, 2.0, 293.15),
        (natural_gas, 0.08, 2.2, 293.15),
        (blasting_powder, 0.25, 1.3, 293.15),
        (smoke, 0.05, 1.1, 500.0),
    ] {
        materials
            .set_thermal(
                material,
                MaterialThermalProperties {
                    conductivity,
                    specific_heat_capacity,
                    default_temperature: Some(default_temperature),
                    cold_transition: None,
                    hot_transition: None,
                },
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}
