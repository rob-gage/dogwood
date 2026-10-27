// Copyright Rob Gage 2026

// Material transitions shared by every shader that changes matter: the shader declares
// `thermal_materials: array<vec4<u32>>` with four records per thermal index and
// `transition_colors: array<vec4<f32>>` with seven records per transition

/// A material transition a unit of matter is due for
struct ThermalTransition {
    /// Whether the unit has banked all of the transition's latent heat
    is_due: bool,
    /// Whether the transition happens on heating rather than cooling
    is_hot: bool,
    /// Temperature in kelvin at which the material changes
    temperature: f32,
    /// Heat per unit of amount the transition takes in or gives off
    latent_heat: f32,
    /// Material identifier of the product
    product: u32,
    /// Amount of the product made from each unit of amount
    amount: f32,
};

/// Returns the thermal index of a material identifier: its form's first thermal index, given in
/// form order static, dynamic, fluid, gas, plus its form-specific index
fn thermal_index(material_identifier: u32, thermal_offsets: vec4<u32>) -> u32 {
    return thermal_offsets[material_identifier >> 30u] + (material_identifier & 0x3fffffffu);
}

/// Returns the heat capacity per unit of amount of the material at a thermal index
fn thermal_heat_capacity(thermal_index: u32) -> f32 {
    return bitcast<f32>(thermal_materials[thermal_index * 4u].x);
}

/// Returns the temperature a material at a temperature shows its surroundings: a material changing
/// on heating or cooling holds its transition temperature while its latent heat banks, then shows
/// only the temperature beyond the bank
fn effective_temperature(temperature: f32, thermal_index: u32) -> f32 {
    let properties: vec4<u32> = thermal_materials[thermal_index * 4u];
    let cold: vec4<u32> = thermal_materials[thermal_index * 4u + 1u];
    let flags: u32 = thermal_materials[thermal_index * 4u + 2u].z;
    let heat_capacity: f32 = bitcast<f32>(properties.x);
    // the hot transition's temperature and latent heat sit in words 2 and 3, the cold's in 6 and 7
    if (flags & 1u) != 0u {
        let transition_temperature: f32 = bitcast<f32>(properties.z);
        if temperature >= transition_temperature {
            let bank: f32 = bitcast<f32>(properties.w) / heat_capacity;
            return select(temperature - bank, transition_temperature,
                temperature < transition_temperature + bank);
        }
    }
    if (flags & 2u) != 0u {
        let transition_temperature: f32 = bitcast<f32>(cold.z);
        if temperature <= transition_temperature {
            let bank: f32 = bitcast<f32>(cold.w) / heat_capacity;
            return select(temperature + bank, transition_temperature,
                temperature > transition_temperature - bank);
        }
    }
    return temperature;
}

/// Returns the transition of the material at a thermal index once its transition temperature is
/// reached, or its whole latent heat bank when enabled
fn due_transition(temperature: f32, thermal_index: u32, has_bank: bool) -> ThermalTransition {
    let records: array<vec4<u32>, 3> = array<vec4<u32>, 3>(
        thermal_materials[thermal_index * 4u],
        thermal_materials[thermal_index * 4u + 1u],
        thermal_materials[thermal_index * 4u + 2u],
    );
    let heat_capacity: f32 = bitcast<f32>(records[0].x);
    var transition: ThermalTransition;
    if (records[2].z & 1u) != 0u {
        transition.temperature = bitcast<f32>(records[0].z);
        transition.latent_heat = bitcast<f32>(records[0].w);
        let bank: f32 = select(0.0, transition.latent_heat / heat_capacity, has_bank);
        if temperature >= transition.temperature + bank {
            transition.is_due = true;
            transition.is_hot = true;
            transition.product = records[1].x;
            transition.amount = bitcast<f32>(records[1].y);
            return transition;
        }
    }
    if (records[2].z & 2u) != 0u {
        transition.temperature = bitcast<f32>(records[1].z);
        transition.latent_heat = bitcast<f32>(records[1].w);
        let bank: f32 = select(0.0, transition.latent_heat / heat_capacity, has_bank);
        if temperature <= transition.temperature - bank {
            transition.is_due = true;
            transition.product = records[2].x;
            transition.amount = bitcast<f32>(records[2].y);
            return transition;
        }
    }
    transition.is_due = false;
    return transition;
}

/// Returns the temperature of a transition's product: the heat a source of an amount and heat
/// capacity per unit holds beyond its transition temperature, less the latent heat it took in or
/// gave off, spread over the product of an amount and heat capacity per unit
fn product_temperature(
    transition: ThermalTransition,
    temperature: f32,
    amount: f32,
    heat_capacity: f32,
    product_amount: f32,
    product_heat_capacity: f32,
    has_latent: bool,
) -> f32 {
    let product_capacity: f32 = max(product_amount * product_heat_capacity, 1.0e-6);
    let beyond: f32 = amount * heat_capacity * (temperature - transition.temperature);
    let latent: f32 = select(0.0, amount * transition.latent_heat, has_latent);
    let heat: f32 = select(beyond + latent, beyond - latent, transition.is_hot);
    return max(transition.temperature + heat / product_capacity, 0.0);
}

/// Returns a linear channel of an sRGB channel
fn transition_linear(value: f32) -> f32 {
    return select(pow((value + 0.055) / 1.055, 2.4), value / 12.92, value <= 0.04045);
}

/// Returns an sRGB channel of a linear channel
fn transition_srgb(value: f32) -> f32 {
    return select(1.055 * pow(value, 1.0 / 2.4) - 0.055, value * 12.92, value <= 0.0031308);
}

/// Returns a hash in the range from minus one to one of a seed
fn transition_random(seed: u32) -> f32 {
    var hash: u32 = seed * 747796405u + 2891336453u;
    hash = ((hash >> ((hash >> 28u) + 4u)) ^ hash) * 277803737u;
    hash = (hash >> 22u) ^ hash;
    return f32(hash) / 2147483647.5 - 1.0;
}

/// Returns a packed sRGB color carried through the hot or cold transition of the material at a
/// thermal index: transformed in linear light by the transition's matrix and offset, varied by
/// random amounts from a seed, and clamped when packed
fn transition_color(color: u32, thermal_index: u32, is_hot: bool, seed: u32) -> u32 {
    let base: u32 = (thermal_index * 2u + select(1u, 0u, is_hot)) * 7u;
    let srgb: vec4<f32> = unpack4x8unorm(color);
    let linear: vec4<f32> = vec4<f32>(
        transition_linear(srgb.r),
        transition_linear(srgb.g),
        transition_linear(srgb.b),
        srgb.a,
    );
    var transformed: vec4<f32> = vec4<f32>(
        dot(transition_colors[base], linear),
        dot(transition_colors[base + 1u], linear),
        dot(transition_colors[base + 2u], linear),
        dot(transition_colors[base + 3u], linear),
    ) + transition_colors[base + 4u];
    let channel_variance: vec4<f32> = transition_colors[base + 5u];
    let intensity_variance: f32 = transition_colors[base + 6u].x;
    transformed *= 1.0 + intensity_variance * transition_random(seed);
    transformed += channel_variance * vec4<f32>(
        transition_random(seed ^ 0x68e31da4u),
        transition_random(seed ^ 0xb5297a4du),
        transition_random(seed ^ 0x1b56c4e9u),
        transition_random(seed ^ 0x7f4a7c15u),
    );
    let clamped: vec4<f32> = clamp(transformed, vec4<f32>(0.0), vec4<f32>(1.0));
    return pack4x8unorm(vec4<f32>(
        transition_srgb(clamped.r),
        transition_srgb(clamped.g),
        transition_srgb(clamped.b),
        clamped.a,
    ));
}
