use crate::simulation::SceneSimulationConfiguration;

pub(crate) fn scene_test_configuration(
    gravity: [f32; 2],
    width: u16,
    height: u16,
) -> SceneSimulationConfiguration {
    SceneSimulationConfiguration {
        gravity,
        ambient_temperature: 293.15,
        empty_space_thermal_conductivity: 0.0,
        empty_space_heat_capacity: 1.0,
        maximum_gas_concentration: 4.0,
        width,
        height,
        buffer_size: 2,
        streaming_batch_size: 1,
    }
}
