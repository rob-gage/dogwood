// Copyright Rob Gage 2026

use super::{
    Material,
    MaterialError,
    MaterialForm,
    MaterialIdentifier,
    MaterialPropertiesCellularDynamic,
    MaterialPropertiesCellularStatic,
    MaterialPropertiesFluid,
    MaterialPropertiesGas,
    MaterialTemperatureColor,
    MaterialAmountColor,
    MaterialReactant,
    MaterialRegistry,
    MaterialTransition,
};
use crate::scenes::cellular_tiles::CellCoordinates;
use engine_graphics::{
    sprites::SpriteSheet,
    world::materials::{
        MaterialColorTransformation,
        pack_radiance,
    },
};
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;
use std::sync::mpsc::{
    Receiver,
    Sender,
    channel,
};

/// Packed cellular albedo pixels and their frame and repetition dimensions
struct MaterialAlbedo {
    /// Packed sRGB pixels in row-major order
    pixels: Vec<u32>,
    /// Frame width in pixels
    width: u32,
    /// Frame height in pixels
    height: u32,
    /// Horizontal repetition width in cells
    repetition_width: f64,
    /// Vertical repetition height in cells
    repetition_height: f64,
}

/// Packed linear emission and occlusion pixels with their frame and repetition dimensions
struct MaterialRadiance {
    /// Packed radiance pixels in row-major order
    pixels: Vec<u32>,
    /// Frame width in pixels
    width: u32,
    /// Frame height in pixels
    height: u32,
    /// Horizontal repetition width in cells
    repetition_width: f64,
    /// Vertical repetition height in cells
    repetition_height: f64,
}

/// Immutable registered materials and compiled GPU resources
pub struct MaterialTable {
    /// Sole owned material definitions with registration inaccessible after compilation
    material_registry: MaterialRegistry,
    /// Identifiers assigned to exact, case-sensitive material names
    identifiers_by_name: HashMap<String, MaterialIdentifier>,
    /// Resolved cellular dynamic debris for static materials
    debris_by_identifier: HashMap<MaterialIdentifier, MaterialIdentifier>,
    /// Mass, absorption, strength, and debris material identifier bits as four 32-bit words per
    /// static cellular index
    cellular_static_properties_buffer: wgpu::Buffer,
    /// Mass, friction, restitution, and padding as four `f32` per dynamic cellular index
    cellular_dynamic_properties_buffer: wgpu::Buffer,
    /// Density, nonlinear XSPH viscosity control, cohesion, artificial pressure, drag, friction,
    /// restitution, and color bits as eight 32-bit words per fluid index
    fluid_properties_buffer: wgpu::Buffer,
    /// Mass, diffusivity, cohesion, and color bits, then opacity, radiance bits, and padding as
    /// eight 32-bit words per gas index
    gas_properties_buffer: wgpu::Buffer,
    /// Heat capacity, conduction, hot and cold transitions, and transition flags as sixteen 32-bit
    /// words per material, indexed by its thermal index
    thermal_properties_buffer: wgpu::Buffer,
    /// Color then radiance transformations of the hot and cold transitions as 28 `vec4<f32>` per
    /// material, indexed by its thermal index
    transition_color_buffer: wgpu::Buffer,
    /// Five `vec4<u32>` records per reaction: conditions and counts, ranges, reactant amounts,
    /// then three product identifiers and amounts
    reaction_buffer: wgpu::Buffer,
    /// Forty-two `vec4<f32>` records per reaction: seven color and seven radiance vectors per
    /// product slot
    reaction_color_buffer: wgpu::Buffer,
    /// Pair headers, unary headers, then reaction index lists; list entries set bit 31 when
    /// reactant order is swapped
    reaction_lookup_buffer: wgpu::Buffer,
    /// Temperature color headers and keys as `vec4<u32>` then `vec4<f32>` records
    temperature_color_buffer: wgpu::Buffer,
    /// Amount color headers and eleven `vec4<f32>` records per key, indexed by thermal index
    amount_color_buffer: wgpu::Buffer,
    /// Number of registered reactions
    reaction_count: u32,
    /// Whether any reaction product is a fluid
    has_fluid_reaction_products: bool,
    /// First thermal index of the static, dynamic, fluid, and gas forms, static index zero being
    /// empty
    thermal_offsets: [u32; 4],
    /// Whether any registered transition starts or ends in a fluid
    has_fluid_transitions: bool,
    /// Packed sRGB albedo pixels, frame size, and repetition size in cells of each static
    /// cellular material, indexed from reserved empty index zero
    cellular_static_albedos: Vec<MaterialAlbedo>,
    /// Packed sRGB albedo pixels, frame size, and repetition size in cells of each dynamic
    /// cellular material
    cellular_dynamic_albedos: Vec<MaterialAlbedo>,
    /// Packed linear radiance pixels, frame size, and repetition size in cells of each static
    /// cellular material, indexed from reserved empty index zero
    cellular_static_radiances: Vec<MaterialRadiance>,
    /// Packed linear radiance pixels, frame size, and repetition size in cells of each dynamic
    /// cellular material
    cellular_dynamic_radiances: Vec<MaterialRadiance>,
}

impl MaterialTable {

    /// Creates a storage buffer from a nonempty byte array
    fn create_storage_buffer(
        device: &wgpu::Device,
        label: &'static str,
        bytes: &[u8],
    ) -> wgpu::Buffer {
        let bytes: &[u8] = if bytes.is_empty() { &[0; 16] } else { bytes };
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Serializes reactions and their thermal-index lookup lists
    fn new_reactions(
        registry: &MaterialRegistry,
        identifiers_by_name: &HashMap<String, MaterialIdentifier>,
        thermal_offsets: [u32; 4],
        reactions: &[super::MaterialReaction],
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, u32, bool), MaterialError> {
        let thermal_count: usize = registry.material_count() + 1;
        let mut reaction_bytes: Vec<u8> = Vec::new();
        let mut reaction_color_bytes: Vec<u8> = Vec::new();
        let mut unary_lists: Vec<Vec<u32>> = vec![Vec::new(); thermal_count];
        let mut pair_lists: Vec<Vec<u32>> = vec![Vec::new(); thermal_count * thermal_count];
        let mut has_fluid_products: bool = false;
        for (reaction_index, reaction) in reactions.iter().enumerate() {
            let matches: Vec<Vec<MaterialIdentifier>> = reaction.reactants.iter().map(|reactant| {
                let mut identifiers: Vec<MaterialIdentifier> = Vec::new();
                match reactant {
                    MaterialReactant::Material { name, .. } => {
                        if let Some(identifier) = identifiers_by_name.get(name) {
                            identifiers.push(*identifier);
                        }
                    }
                    MaterialReactant::Tag { tag, .. } => {
                        for form in [
                            MaterialForm::CellularStatic,
                            MaterialForm::CellularDynamic,
                            MaterialForm::Fluid,
                            MaterialForm::Gas,
                        ]
                        {
                            let first: u32 = u32::from(form == MaterialForm::CellularStatic);
                            for index in first.. {
                                let Ok(identifier) = MaterialIdentifier::new(form, index) else {
                                    break;
                                };
                                let Some(material) = registry.get(identifier) else { break; };
                                let chemical = match material {
                                    Material::CellularStatic { properties, .. } => {
                                        &properties.chemical
                                    }
                                    Material::CellularDynamic { properties, .. } => {
                                        &properties.chemical
                                    }
                                    Material::Fluid { properties, .. } => &properties.chemical,
                                    Material::Gas { properties, .. } => &properties.chemical,
                                };
                                if chemical.tags.iter().any(|candidate| candidate == tag) {
                                    identifiers.push(identifier);
                                }
                            }
                        }
                    }
                }
                identifiers
            }).collect();
            if reaction.reactants.len() == 1 {
                for identifier in &matches[0] {
                    let thermal: usize =
                        (identifier.index() + thermal_offsets[identifier.form() as usize]) as usize;
                    unary_lists[thermal].push(reaction_index as u32);
                }
            } else {
                for first in &matches[0] {
                    for second in &matches[1] {
                        let a: usize =
                            (first.index() + thermal_offsets[first.form() as usize]) as usize;
                        let b: usize =
                            (second.index() + thermal_offsets[second.form() as usize]) as usize;
                        let list_index: usize = a * thermal_count + b;
                        pair_lists[list_index].push(reaction_index as u32);
                    }
                }
                for first in &matches[1] {
                    for second in &matches[0] {
                        let a: usize =
                            (first.index() + thermal_offsets[first.form() as usize]) as usize;
                        let b: usize =
                            (second.index() + thermal_offsets[second.form() as usize]) as usize;
                        // a pair matching both orders already has its unswapped entry
                        if matches[0].contains(first) && matches[1].contains(second) {
                            continue;
                        }
                        pair_lists[a * thermal_count + b].push((reaction_index as u32) | (1 << 31));
                    }
                }
            }
            for product in &reaction.products {
                has_fluid_products |=
                    identifiers_by_name[&product.product].form() == MaterialForm::Fluid;
            }
            let counts: u32 =
                reaction.reactants.len() as u32 | ((reaction.products.len() as u32) << 2);
            for word in [
                reaction.rate.to_bits(),
                reaction.heat.to_bits(),
                reaction.pressure.to_bits(),
                counts,
                finite_max_bits(reaction.temperature_minimum),
                finite_max_bits(reaction.temperature_maximum),
                finite_max_bits(reaction.pressure_minimum),
                finite_max_bits(reaction.pressure_maximum),
                reactant_amount(&reaction.reactants[0]).to_bits(),
                reaction.reactants.get(1).map_or(0.0, reactant_amount).to_bits(),
                0,
                0,
            ]
            {
                reaction_bytes.extend_from_slice(&word.to_le_bytes());
            }
            for slot in 0..3 {
                let product = reaction.products.get(slot);
                let identifier: u32 = product.map_or(0, |product| {
                    identifiers_by_name[&product.product].as_u32()
                });
                let amount: u32 = product.map_or(0.0, |product| product.amount).to_bits();
                reaction_bytes.extend_from_slice(&identifier.to_le_bytes());
                reaction_bytes.extend_from_slice(&amount.to_le_bytes());
            }
            reaction_bytes.extend_from_slice(&[0; 8]);
            for product in &reaction.products {
                append_transform(&mut reaction_color_bytes, &product.color);
                append_transform(&mut reaction_color_bytes, &product.radiance);
            }
            for _ in reaction.products.len()..3 {
                append_transform(&mut reaction_color_bytes, &MaterialColorTransformation::IDENTITY);
                append_transform(&mut reaction_color_bytes, &MaterialColorTransformation::IDENTITY);
            }
        }
        // pair headers, unary headers, each thermal index's reaction flags (bit 0 when it has
        // unary reactions, bit 1 when it takes part in a two-reactant reaction), then its partner
        // classes: bit `partner % 32` for every thermal index it reacts with
        let headers: usize = thermal_count * thermal_count + 3 * thermal_count;
        let mut lookup_words: Vec<u32> = vec![0; headers];
        for thermal in 0..thermal_count {
            let row: &[Vec<u32>] =
                &pair_lists[thermal * thermal_count..(thermal + 1) * thermal_count];
            let has_unary: bool = !unary_lists[thermal].is_empty();
            let has_pair: bool = row.iter().any(|list| !list.is_empty());
            lookup_words[thermal_count * thermal_count + thermal_count + thermal] =
                u32::from(has_unary) | u32::from(has_pair) << 1;
            lookup_words[thermal_count * thermal_count + 2 * thermal_count + thermal] = row.iter()
                .enumerate()
                .filter(|(_, list)| !list.is_empty())
                .fold(0, |classes, (partner, _)| classes | 1 << (partner % 32));
        }
        let mut list_word: usize = headers;
        for (list_base, lists) in [(0usize, &pair_lists),
            (thermal_count * thermal_count, &unary_lists)]
        {
            for (list_index, list) in lists.iter().enumerate() {
                if list.len() > 255 || list_word > 0x00ff_ffff {
                    return Err(MaterialError::InvalidReaction {
                        message: String::from(
                            "reaction lookup lists exceed their GPU encoding capacity",
                        ),
                    });
                }
                let header_index: usize = list_base + list_index;
                if !list.is_empty() {
                    lookup_words[header_index] = ((list_word as u32) << 8) | list.len() as u32;
                    lookup_words.extend_from_slice(list);
                    list_word += list.len();
                }
            }
        }
        if lookup_words.is_empty() { lookup_words.push(0); }
        let mut lookup_bytes: Vec<u8> = Vec::with_capacity(lookup_words.len() * 4);
        for word in lookup_words { lookup_bytes.extend_from_slice(&word.to_le_bytes()); }
        Ok((reaction_bytes, reaction_color_bytes, lookup_bytes, reactions.len() as u32,
            has_fluid_products))
    }

    /// Compiles registered properties and reads every cellular albedo and radiance frame once
    #[doc(hidden)]
    pub fn new(
        accelerator: &Accelerator,
        material_registry: MaterialRegistry,
        identifiers_by_name: HashMap<String, MaterialIdentifier>,
        debris_by_identifier: HashMap<MaterialIdentifier, MaterialIdentifier>,
        transition_products: HashMap<(MaterialIdentifier, bool), MaterialIdentifier>,
        reactions: Vec<super::MaterialReaction>,
    ) -> Result<Self, MaterialError> {
        let device: &wgpu::Device = accelerator.device();
        // collect the albedo frames of static materials after empty index zero, then dynamic ones
        let mut albedo_frames: Vec<Option<(&SpriteSheet, u32, f32, f32)>> = material_registry
            .cellular_static_edit_colors()
            .map(|edit_color| edit_color.map(|edit_color| {
                (&*edit_color.sprite_sheet, edit_color.frame, edit_color.width, edit_color.height)
            }))
            .collect();
        let cellular_static_count: usize = albedo_frames.len();
        let mut radiance_frames: Vec<Option<(&SpriteSheet, u32, f32, f32, f32)>> = vec![None];
        let mut static_property_bytes: Vec<u8> = vec![0; 16];
        let mut property_bytes: Vec<u8> = Vec::new();
        for index in 1.. {
            let Ok(identifier) = MaterialIdentifier::new(MaterialForm::CellularStatic, index)
            else { break; };
            let Some(properties) = material_registry.cellular_static_properties(identifier)
            else { break; };
            if !properties.mass.is_finite() || properties.mass <= 0.0
                || !properties.friction.is_finite() || properties.friction < 0.0
                || !(0.0..=1.0).contains(&properties.restitution)
                || !properties.edit_radiance_intensity.is_finite()
                || properties.edit_radiance_intensity < 0.0
                || properties.strength.is_nan() || properties.strength <= 0.0
                || !properties.absorption.is_finite()
                || !(0.0..=1.0).contains(&properties.absorption)
            {
                return Err(MaterialError::Compilation(String::from(
                    "static cellular materials need a positive mass, a nonnegative friction, \
                    a restitution from zero to one, a positive strength, and a finite absorption \
                    from zero to one, and a finite nonnegative radiance intensity",
                )));
            }
            radiance_frames.push(Some((
                &*properties.edit_radiance.sprite_sheet,
                properties.edit_radiance.frame,
                properties.edit_radiance.width,
                properties.edit_radiance.height,
                properties.edit_radiance_intensity,
            )));
            // the debris material identifier rides in the last component's bits, zero for none
            let debris_bits: f32 = f32::from_bits(
                debris_by_identifier.get(&identifier).map_or(0, |debris| debris.as_u32()),
            );
            for value in [
                properties.mass,
                properties.absorption,
                properties.strength,
                debris_bits,
            ] {
                static_property_bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let cellular_static_properties_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device,
            "`MaterialTable.cellular_static_properties_buffer`",
            &static_property_bytes,
        );
        accelerator.queue().write_buffer(
            &cellular_static_properties_buffer,
            0,
            &static_property_bytes,
        );
        for index in 0.. {
            let Ok(identifier) = MaterialIdentifier::new(MaterialForm::CellularDynamic, index)
            else { break; };
            let Some(properties) = material_registry.cellular_dynamic_properties(identifier)
            else { break; };
            if !properties.mass.is_finite() || properties.mass <= 0.0
                || !properties.friction.is_finite() || properties.friction < 0.0
                || !(0.0..=1.0).contains(&properties.restitution)
                || !properties.edit_radiance_intensity.is_finite()
                || properties.edit_radiance_intensity < 0.0
            {
                return Err(MaterialError::Compilation(String::from(
                    "dynamic cellular materials need a positive mass, a nonnegative friction, \
                    a restitution from zero to one, and a finite nonnegative radiance intensity",
                )));
            }
            // WGSL vec4<f32> records with a 16-byte stride
            for value in [properties.mass, properties.friction, properties.restitution, 0.0] {
                property_bytes.extend_from_slice(&value.to_le_bytes());
            }
            let edit_color = &properties.edit_color;
            albedo_frames.push(Some((
                &*edit_color.sprite_sheet,
                edit_color.frame,
                edit_color.width,
                edit_color.height,
            )));
            let edit_radiance = &properties.edit_radiance;
            radiance_frames.push(Some((
                &*edit_radiance.sprite_sheet,
                edit_radiance.frame,
                edit_radiance.width,
                edit_radiance.height,
                properties.edit_radiance_intensity,
            )));
        }
        // a zeroed record satisfies the storage binding when no dynamic material is registered
        if property_bytes.is_empty() { property_bytes.resize(16, 0); }
        let cellular_dynamic_properties_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device,
            "`MaterialTable.cellular_dynamic_properties_buffer`",
            &property_bytes,
        );
        accelerator.queue().write_buffer(&cellular_dynamic_properties_buffer, 0, &property_bytes);
        // serialize fluid records as density, viscosity, cohesion, and artificial pressure, then
        // drag, friction, restitution, and color bits
        let mut fluid_property_bytes: Vec<u8> = Vec::new();
        for index in 0.. {
            let Ok(identifier) = MaterialIdentifier::new(MaterialForm::Fluid, index)
            else { break; };
            let Some(properties) = material_registry.fluid_properties(identifier) else { break; };
            if !properties.density.is_finite() || properties.density <= 0.0
                || !(0.0..=1.0).contains(&properties.viscosity)
                || !properties.cohesion.is_finite() || properties.cohesion < 0.0
                || !properties.artificial_pressure.is_finite()
                || properties.artificial_pressure < 0.0
                || !properties.drag.is_finite() || properties.drag < 0.0
                || !properties.friction.is_finite() || properties.friction < 0.0
                || !(0.0..=1.0).contains(&properties.restitution)
            {
                return Err(MaterialError::Compilation(String::from(
                    "fluid materials need a positive density, a viscosity from zero to one, \
                    nonnegative cohesion, artificial pressure, drag, and friction, and a \
                    restitution from zero to one",
                )));
            }
            for value in [properties.density, properties.viscosity, properties.cohesion,
                properties.artificial_pressure] {
                fluid_property_bytes.extend_from_slice(&value.to_le_bytes());
            }
            fluid_property_bytes.extend_from_slice(&properties.drag.to_le_bytes());
            fluid_property_bytes.extend_from_slice(&properties.friction.to_le_bytes());
            fluid_property_bytes.extend_from_slice(&properties.restitution.to_le_bytes());
            fluid_property_bytes.extend_from_slice(&properties.edit_color.to_le_bytes());
        }
        // keep the storage binding valid when the registry has no fluid materials
        if fluid_property_bytes.is_empty() { fluid_property_bytes.resize(32, 0); }
        let fluid_properties_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device,
            "`MaterialTable.fluid_properties_buffer`",
            &fluid_property_bytes,
        );
        accelerator.queue().write_buffer(&fluid_properties_buffer, 0, &fluid_property_bytes);
        // serialize gas records as mass, diffusivity, cohesion, and color bits
        let mut gas_property_bytes: Vec<u8> = Vec::new();
        for index in 0.. {
            let Ok(identifier) = MaterialIdentifier::new(MaterialForm::Gas, index)
            else { break; };
            let Some(properties) = material_registry.gas_properties(identifier) else { break; };
            // species slots store gas indices in eight bits, and gas simulation stores each
            // index plus one in eight bits
            if index > 254 {
                return Err(MaterialError::Compilation(String::from(
                    "at most 255 gas materials can be registered",
                )));
            }
            if !properties.mass.is_finite() || properties.mass <= 0.0
                || !(0.0..=0.4).contains(&properties.diffusivity)
                || !(0.0..=1.0).contains(&properties.cohesion)
                || !properties.graphics.opacity.is_finite() || properties.graphics.opacity < 0.0
            {
                return Err(MaterialError::Compilation(String::from(
                    "gas materials need a positive mass, a diffusivity from zero to 0.4, a \
                    cohesion from zero to one, and a nonnegative opacity",
                )));
            }
            for value in [properties.mass, properties.diffusivity, properties.cohesion] {
                gas_property_bytes.extend_from_slice(&value.to_le_bytes());
            }
            gas_property_bytes.extend_from_slice(&properties.graphics.color.to_le_bytes());
            gas_property_bytes.extend_from_slice(&properties.graphics.opacity.to_le_bytes());
            gas_property_bytes.extend_from_slice(&properties.graphics.radiance.to_le_bytes());
            gas_property_bytes.extend_from_slice(&[0; 8]);
        }
        // keep the storage binding valid when the registry has no gas materials
        if gas_property_bytes.is_empty() { gas_property_bytes.resize(32, 0); }
        let gas_properties_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device,
            "`MaterialTable.gas_properties_buffer`",
            &gas_property_bytes,
        );
        accelerator.queue().write_buffer(&gas_properties_buffer, 0, &gas_property_bytes);
        let (
            mut thermal_property_bytes,
            transition_color_bytes,
            temperature_color_bytes,
            amount_color_bytes,
            thermal_offsets,
            has_fluid_transitions,
        ): (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, [u32; 4], bool) =
            Self::new_thermal_properties(&material_registry, &transition_products)?;
        let (reaction_bytes, reaction_color_bytes, reaction_lookup_bytes, reaction_count,
            has_fluid_reaction_products) = Self::new_reactions(
                &material_registry, &identifiers_by_name, thermal_offsets, &reactions,
            )?;
        // the fluid raster also tracks the species of materials that react
        let thermal_count: usize = thermal_property_bytes.len() / 64;
        let flags_base: usize = thermal_count * thermal_count + thermal_count;
        for thermal in 0..thermal_count {
            let flag_offset: usize = (flags_base + thermal) * 4;
            let flags: u32 = u32::from_le_bytes(
                reaction_lookup_bytes[flag_offset..flag_offset + 4].try_into()
                    .expect("reaction flags are four bytes"),
            );
            if flags == 0 { continue; }
            let word_offset: usize = thermal * 64 + 40;
            let word: u32 = u32::from_le_bytes(
                thermal_property_bytes[word_offset..word_offset + 4].try_into()
                    .expect("thermal flags are four bytes"),
            ) | THERMAL_FLAG_TRACKED;
            thermal_property_bytes[word_offset..word_offset + 4]
                .copy_from_slice(&word.to_le_bytes());
        }
        let thermal_properties_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device,
            "`MaterialTable.thermal_properties_buffer`",
            &thermal_property_bytes,
        );
        accelerator.queue().write_buffer(&thermal_properties_buffer, 0, &thermal_property_bytes);
        let transition_color_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device,
            "`MaterialTable.transition_color_buffer`",
            &transition_color_bytes,
        );
        accelerator.queue().write_buffer(&transition_color_buffer, 0, &transition_color_bytes);
        let temperature_color_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device,
            "`MaterialTable.temperature_color_buffer`",
            &temperature_color_bytes,
        );
        accelerator.queue().write_buffer(
            &temperature_color_buffer,
            0,
            &temperature_color_bytes,
        );
        let amount_color_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device, "`MaterialTable.amount_color_buffer`", &amount_color_bytes,
        );
        accelerator.queue().write_buffer(&amount_color_buffer, 0, &amount_color_bytes);

        let reaction_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device, "`MaterialTable.reaction_buffer`", &reaction_bytes,
        );
        accelerator.queue().write_buffer(&reaction_buffer, 0, &reaction_bytes);
        let reaction_color_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device, "`MaterialTable.reaction_color_buffer`", &reaction_color_bytes,
        );
        accelerator.queue().write_buffer(&reaction_color_buffer, 0, &reaction_color_bytes);
        let reaction_lookup_buffer: wgpu::Buffer = Self::create_storage_buffer(
            device, "`MaterialTable.reaction_lookup_buffer`", &reaction_lookup_bytes,
        );
        accelerator.queue().write_buffer(&reaction_lookup_buffer, 0, &reaction_lookup_bytes);
        // copy every frame into a readable buffer with rows padded to the copy alignment
        let mut command_encoder: wgpu::CommandEncoder = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("`MaterialTable` cellular albedo frame copy"),
            },
        );
        let mut frame_copies: Vec<Option<(wgpu::Buffer, u32, u32, u32, f64, f64)>> = Vec::new();
        for albedo_frame in &albedo_frames {
            let Some((sprite_sheet, frame, width, height)) = albedo_frame else {
                frame_copies.push(None);
                continue;
            };
            if !width.is_finite() || !height.is_finite() || *width <= 0.0 || *height <= 0.0 {
                return Err(MaterialError::Compilation(String::from(
                    "cellular material repetition sizes must be finite and positive",
                )));
            }
            let texture_region: [f32; 4] = sprite_sheet.frame_texture_region(*frame)
                .ok_or_else(|| MaterialError::Compilation(String::from(
                    "cellular material frame is outside its sprite sheet",
                )))?;
            let texture: &wgpu::Texture = sprite_sheet.texture_view().texture();
            let frame_x: u32 = (texture_region[0] * texture.width() as f32).round() as u32;
            let frame_y: u32 = (texture_region[1] * texture.height() as f32).round() as u32;
            let frame_width: u32 = sprite_sheet.width();
            let frame_height: u32 = sprite_sheet.height();
            let row_byte_count: u32 = (frame_width * 4)
                .next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            let buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`MaterialTable` cellular albedo frame"),
                size: u64::from(row_byte_count) * u64::from(frame_height),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            command_encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: frame_x, y: frame_y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row_byte_count),
                        rows_per_image: Some(frame_height),
                    },
                },
                wgpu::Extent3d {
                    width: frame_width,
                    height: frame_height,
                    depth_or_array_layers: 1,
                },
            );
            // the scale is in cells per pixel, so one repetition spans the frame times the scale
            frame_copies.push(Some((
                buffer,
                row_byte_count,
                frame_width,
                frame_height,
                f64::from(frame_width) * f64::from(*width),
                f64::from(frame_height) * f64::from(*height),
            )));
        }
        accelerator.queue().submit([command_encoder.finish()]);
        // wait once for every frame, since colors are needed before any cell is created
        let (response_sender, response_receiver): (
            Sender<Result<(), wgpu::BufferAsyncError>>,
            Receiver<Result<(), wgpu::BufferAsyncError>>,
        ) = channel();
        for (buffer, ..) in frame_copies.iter().flatten() {
            let response_sender: Sender<Result<(), wgpu::BufferAsyncError>> =
                response_sender.clone();
            buffer.map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = response_sender.send(result);
            });
        }
        drop(response_sender);
        let read_error = || String::from("cellular material albedo frames could not be read");
        device.poll(wgpu::PollType::wait_indefinitely())
            .map_err(|_| MaterialError::Compilation(read_error()))?;
        for response in response_receiver.try_iter() {
            response.map_err(|_| MaterialError::Compilation(read_error()))?;
        }
        let mut albedos: Vec<MaterialAlbedo> =
            Vec::with_capacity(frame_copies.len());
        for frame_copy in frame_copies {
            let Some((buffer, row_byte_count, width, height, repetition_width, repetition_height)) =
                frame_copy
            else {
                albedos.push(MaterialAlbedo {
                    pixels: Vec::new(), width: 0, height: 0,
                    repetition_width: 1.0, repetition_height: 1.0,
                });
                continue;
            };
            let mut pixels: Vec<u32> = Vec::with_capacity((width * height) as usize);
            {
                let mapped_bytes: wgpu::BufferView = buffer.get_mapped_range(..)
                    .expect("mapped cellular albedo frame should be readable");
                for row in mapped_bytes.chunks_exact(row_byte_count as usize) {
                    for pixel in row[..width as usize * 4].chunks_exact(4) {
                        pixels.push(u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]));
                    }
                }
            }
            buffer.unmap();
            albedos.push(MaterialAlbedo {
                pixels, width, height, repetition_width,
                repetition_height,
            });
        }
        let mut radiance_encoder: wgpu::CommandEncoder = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("`MaterialTable` cellular radiance frame copy"),
            },
        );
        let mut radiance_copies: Vec<Option<(wgpu::Buffer, u32, u32, u32, f64, f64, f32)>> =
            Vec::new();
        for radiance_frame in &radiance_frames {
            let Some((sprite_sheet, frame, width, height, intensity)) = radiance_frame else {
                radiance_copies.push(None);
                continue;
            };
            if !width.is_finite() || !height.is_finite() || *width <= 0.0 || *height <= 0.0
                || !intensity.is_finite() || *intensity < 0.0
            {
                return Err(MaterialError::Compilation(String::from(
                    "cellular radiance repetition sizes must be finite and positive and intensity must be finite and nonnegative",
                )));
            }
            let texture_region: [f32; 4] = sprite_sheet.frame_texture_region(*frame)
                .ok_or_else(|| MaterialError::Compilation(String::from(
                    "cellular material radiance frame is outside its sprite sheet",
                )))?;
            let texture: &wgpu::Texture = sprite_sheet.texture_view().texture();
            let frame_x: u32 = (texture_region[0] * texture.width() as f32).round() as u32;
            let frame_y: u32 = (texture_region[1] * texture.height() as f32).round() as u32;
            let frame_width: u32 = sprite_sheet.width();
            let frame_height: u32 = sprite_sheet.height();
            let row_byte_count: u32 = (frame_width * 4)
                .next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            let buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`MaterialTable` cellular radiance frame"),
                size: u64::from(row_byte_count) * u64::from(frame_height),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            radiance_encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: frame_x, y: frame_y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row_byte_count),
                        rows_per_image: Some(frame_height),
                    },
                },
                wgpu::Extent3d {
                    width: frame_width,
                    height: frame_height,
                    depth_or_array_layers: 1,
                },
            );
            radiance_copies.push(Some((
                buffer,
                row_byte_count,
                frame_width,
                frame_height,
                f64::from(frame_width) * f64::from(*width),
                f64::from(frame_height) * f64::from(*height),
                *intensity,
            )));
        }
        accelerator.queue().submit([radiance_encoder.finish()]);
        let (radiance_sender, radiance_receiver): (
            Sender<Result<(), wgpu::BufferAsyncError>>,
            Receiver<Result<(), wgpu::BufferAsyncError>>,
        ) = channel();
        for (buffer, ..) in radiance_copies.iter().flatten() {
            let radiance_sender: Sender<Result<(), wgpu::BufferAsyncError>> = radiance_sender.clone();
            buffer.map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = radiance_sender.send(result);
            });
        }
        drop(radiance_sender);
        let radiance_read_error = || String::from("cellular material radiance frames could not be read");
        device.poll(wgpu::PollType::wait_indefinitely())
            .map_err(|_| MaterialError::Compilation(radiance_read_error()))?;
        for response in radiance_receiver.try_iter() {
            response.map_err(|_| MaterialError::Compilation(radiance_read_error()))?;
        }
        let mut radiances: Vec<MaterialRadiance> = Vec::with_capacity(radiance_copies.len());
        for radiance_copy in radiance_copies {
            let Some((buffer, row_byte_count, width, height, repetition_width, repetition_height, intensity)) =
                radiance_copy
            else {
                radiances.push(MaterialRadiance {
                    pixels: Vec::new(), width: 0, height: 0,
                    repetition_width: 1.0, repetition_height: 1.0,
                });
                continue;
            };
            let mut pixels: Vec<u32> = Vec::with_capacity((width * height) as usize);
            {
                let mapped_bytes: wgpu::BufferView = buffer.get_mapped_range(..)
                    .expect("mapped cellular radiance frame should be readable");
                for row in mapped_bytes.chunks_exact(row_byte_count as usize) {
                    for pixel in row[..width as usize * 4].chunks_exact(4) {
                        let decode_srgb = |byte: u8| -> f32 {
                            let value: f32 = f32::from(byte) / 255.0;
                            if value <= 0.04045 { value / 12.92 }
                            else { ((value + 0.055) / 1.055).powf(2.4) }
                        };
                        pixels.push(pack_radiance([
                            decode_srgb(pixel[0]) * intensity,
                            decode_srgb(pixel[1]) * intensity,
                            decode_srgb(pixel[2]) * intensity,
                            f32::from(pixel[3]) / 255.0,
                        ]));
                    }
                }
            }
            buffer.unmap();
            radiances.push(MaterialRadiance {
                pixels, width, height, repetition_width, repetition_height,
            });
        }
        let cellular_static_radiances: Vec<MaterialRadiance> = radiances.drain(..cellular_static_count).collect();
        let cellular_dynamic_radiances: Vec<MaterialRadiance> = radiances;
        let cellular_dynamic_albedos: Vec<MaterialAlbedo> =
            albedos.split_off(cellular_static_count);
        Ok(Self {
            material_registry,
            identifiers_by_name,
            debris_by_identifier,
            cellular_static_properties_buffer,
            cellular_dynamic_properties_buffer,
            fluid_properties_buffer,
            gas_properties_buffer,
            thermal_properties_buffer,
            transition_color_buffer,
            reaction_buffer,
            reaction_color_buffer,
            reaction_lookup_buffer,
            temperature_color_buffer,
            amount_color_buffer,
            reaction_count,
            has_fluid_reaction_products,
            thermal_offsets,
            has_fluid_transitions,
            cellular_static_albedos: albedos,
            cellular_dynamic_albedos,
            cellular_static_radiances,
            cellular_dynamic_radiances,
        })
    }

    /// Validates the thermal properties of every material and serializes their records and
    /// transition colors in thermal index order: empty, static, dynamic, fluid, then gas
    fn new_thermal_properties(
        material_registry: &MaterialRegistry,
        transition_products: &HashMap<(MaterialIdentifier, bool), MaterialIdentifier>,
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, [u32; 4], bool), MaterialError> {
        let mut property_bytes: Vec<u8> = vec![0; 64];
        let mut color_bytes: Vec<u8> = vec![0; 448];
        let mut temperature_color_headers: Vec<[u32; 4]> = vec![[0; 4]];
        let mut temperature_color_key_bytes: Vec<u8> = Vec::new();
        let mut amount_color_headers: Vec<[u32; 4]> = vec![[0; 4]];
        let mut amount_color_key_bytes: Vec<u8> = Vec::new();
        let mut offsets: [u32; 4] = [0; 4];
        let mut count: u32 = 1;
        let mut has_fluid_transitions: bool = false;
        for (form_index, form) in [
            MaterialForm::CellularStatic,
            MaterialForm::CellularDynamic,
            MaterialForm::Fluid,
            MaterialForm::Gas,
        ].into_iter().enumerate() {
            let first_index: u32 = if form == MaterialForm::CellularStatic { 1 } else { 0 };
            offsets[form_index] = count - first_index;
            for index in first_index.. {
                let Ok(identifier) = MaterialIdentifier::new(form, index) else { break; };
                let Some(material) = material_registry.get(identifier) else { break; };
                let (heat_capacity, conduction, edit_temperature, hot_transition, cold_transition,
                    temperature_colors):
                    (f32, f32, f32, &Option<MaterialTransition>, &Option<MaterialTransition>,
                    &[MaterialTemperatureColor]) =
                    match material {
                        Material::CellularStatic { properties, .. } => (
                            properties.thermal.heat_capacity,
                            properties.thermal.conduction,
                            properties.thermal.edit_temperature,
                            &properties.thermal.hot_transition,
                            &properties.thermal.cold_transition,
                            &properties.thermal.temperature_colors,
                        ),
                        Material::CellularDynamic { properties, .. } => (
                            properties.thermal.heat_capacity,
                            properties.thermal.conduction,
                            properties.thermal.edit_temperature,
                            &properties.thermal.hot_transition,
                            &properties.thermal.cold_transition,
                            &properties.thermal.temperature_colors,
                        ),
                        Material::Fluid { properties, .. } => (
                            properties.thermal.heat_capacity,
                            properties.thermal.conduction,
                            properties.thermal.edit_temperature,
                            &properties.thermal.hot_transition,
                            &properties.thermal.cold_transition,
                            &properties.thermal.temperature_colors,
                        ),
                        Material::Gas { properties, .. } => (
                            properties.thermal.heat_capacity,
                            properties.thermal.conduction,
                            properties.thermal.edit_temperature,
                            &properties.thermal.hot_transition,
                            &properties.thermal.cold_transition,
                            &properties.thermal.temperature_colors,
                        ),
                    };
                if !heat_capacity.is_finite() || heat_capacity <= 0.0
                    || !conduction.is_finite() || conduction < 0.0
                    || !edit_temperature.is_finite() || edit_temperature < 0.0
                {
                    return Err(MaterialError::Compilation(format!(
                        "material '{}' needs a positive heat capacity, nonnegative conduction, \
                        and nonnegative edit temperature", material.name(),
                    )));
                }
                let first_key_vector_index: u32 = (temperature_color_key_bytes.len() / 16) as u32;
                let mut previous_temperature: Option<f32> = None;
                for key in temperature_colors {
                    if !key.temperature.is_finite() || key.temperature < 0.0
                        || previous_temperature.is_some_and(|temperature| {
                            key.temperature <= temperature
                        })
                        || !valid_visual_transform(&key.color, true)
                        || !valid_visual_transform(&key.radiance, true)
                    {
                        return Err(MaterialError::InvalidTransition {
                            name: material.name().to_owned(),
                            message: String::from(
                                "temperature color keys need finite, nonnegative, strictly increasing \
                                temperatures, finite transformations, and zero variances",
                            ),
                        });
                    }
                    previous_temperature = Some(key.temperature);
                    for value in [key.temperature, 0.0, 0.0, 0.0]
                        .into_iter()
                        .chain(key.color.matrix.iter().flatten().copied())
                        .chain(key.color.offset)
                        .chain(key.radiance.matrix.iter().flatten().copied())
                        .chain(key.radiance.offset)
                    {
                        temperature_color_key_bytes.extend_from_slice(&value.to_le_bytes());
                    }
                }
                temperature_color_headers.push([
                    first_key_vector_index,
                    temperature_colors.len() as u32,
                    0,
                    0,
                ]);
                let amount_colors: &[MaterialAmountColor] = match material {
                    Material::CellularStatic { properties, .. } => {
                        &properties.chemical.amount_colors
                    }
                    Material::CellularDynamic { properties, .. } => {
                        &properties.chemical.amount_colors
                    }
                    Material::Fluid { properties, .. } => &properties.chemical.amount_colors,
                    Material::Gas { .. } => &[],
                };
                let first_amount_vector_index: u32 = (amount_color_key_bytes.len() / 16) as u32;
                let mut previous_amount: Option<f32> = None;
                for key in amount_colors {
                    if !key.amount.is_finite() || key.amount < 0.0
                        || previous_amount.is_some_and(|amount| key.amount <= amount)
                        || !valid_visual_transform(&key.color, true)
                        || !valid_visual_transform(&key.radiance, true)
                    {
                        return Err(MaterialError::InvalidTransition {
                            name: material.name().to_owned(),
                            message: String::from(
                                "amount color keys need finite, nonnegative, strictly increasing \
                                amounts, finite transformations, and zero variances",
                            ),
                        });
                    }
                    previous_amount = Some(key.amount);
                    append_visual_key(&mut amount_color_key_bytes, key.amount, &key.color,
                        &key.radiance);
                }
                amount_color_headers.push([
                    first_amount_vector_index,
                    amount_colors.len() as u32,
                    0,
                    0,
                ]);
                if let (Some(hot_transition), Some(cold_transition)) =
                    (hot_transition, cold_transition)
                    && hot_transition.temperature <= cold_transition.temperature
                {
                    return Err(MaterialError::InvalidTransition {
                        name: material.name().to_owned(),
                        message: String::from(
                            "hot transition temperature must be above cold transition temperature",
                        ),
                    });
                }
                for (hot, transition) in [(true, hot_transition), (false, cold_transition)] {
                    if transition.is_some() && (form == MaterialForm::Fluid
                        || transition_products.get(&(identifier, hot))
                            .is_some_and(|product| product.form() == MaterialForm::Fluid))
                    {
                        has_fluid_transitions = true;
                    }
                }
                let mut flags: u32 = 0;
                let mut words: [u32; 16] = [0; 16];
                words[0] = heat_capacity.to_bits();
                words[1] = conduction.to_bits();
                for (transition_index, transition) in
                    [hot_transition, cold_transition].into_iter().enumerate()
                {
                    let Some(transition) = transition else {
                        color_bytes.extend_from_slice(&[0; 224]);
                        continue;
                    };
                    let color: &MaterialColorTransformation = &transition.color;
                    let radiance: &MaterialColorTransformation = &transition.radiance;
                    if !transition.temperature.is_finite() || transition.temperature < 0.0
                        || !transition.latent_heat.is_finite() || transition.latent_heat < 0.0
                        // the smallest product amount is one gas amount unit, a 4096th of an
                        // atmosphere, so no unit changes into nothing
                        || !transition.amount.is_finite() || transition.amount < 1.0 / 4096.0
                        || !valid_visual_transform(color, false)
                        || !valid_visual_transform(radiance, false)
                    {
                        return Err(MaterialError::InvalidTransition {
                            name: material.name().to_owned(),
                            message: String::from(
                                "transition data need nonnegative temperature and latent heat, an \
                                amount of at least 1/4096, and finite color and radiance \
                                transformations with nonnegative variances",
                            ),
                        });
                    }
                    flags |= 1 << transition_index;
                    let word: usize = 2 + transition_index * 4;
                    words[word] = transition.temperature.to_bits();
                    words[word + 1] = transition.latent_heat.to_bits();
                    words[word + 2] = transition_products[&(identifier, transition_index == 0)]
                        .as_u32();
                    words[word + 3] = transition.amount.to_bits();
                    for transformation in [color, radiance] {
                        for value in transformation.matrix.iter().flatten()
                            .chain(&transformation.offset)
                            .chain(&transformation.channel_variance)
                            .chain([&transformation.intensity_variance, &0.0, &0.0, &0.0])
                        {
                            color_bytes.extend_from_slice(&value.to_le_bytes());
                        }
                    }
                }
                // the fluid raster tracks the species of fluids with amount colors
                if !amount_colors.is_empty() { flags |= THERMAL_FLAG_TRACKED; }
                words[10] = flags;
                for word in words { property_bytes.extend_from_slice(&word.to_le_bytes()); }
                count += 1;
            }
        }
        let mut temperature_color_bytes: Vec<u8> = Vec::new();
        let header_count: u32 = temperature_color_headers.len() as u32;
        for header in temperature_color_headers {
            for (index, word) in header.into_iter().enumerate() {
                let word: u32 = if index == 0 { word + header_count } else { word };
                temperature_color_bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        temperature_color_bytes.extend_from_slice(&temperature_color_key_bytes);
        let mut amount_color_bytes: Vec<u8> = Vec::new();
        let amount_header_count: u32 = amount_color_headers.len() as u32;
        for header in amount_color_headers {
            for (index, word) in header.into_iter().enumerate() {
                let word: u32 = if index == 0 { word + amount_header_count } else { word };
                amount_color_bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        amount_color_bytes.extend_from_slice(&amount_color_key_bytes);
        Ok((property_bytes, color_bytes, temperature_color_bytes, amount_color_bytes,
            offsets, has_fluid_transitions))
    }

    /// Returns the color a cell or fluid particle is created with: its material's albedo at the
    /// cell, or a fluid's edit color, tinted by a packed sRGB color, or the color alone for
    /// materials without either
    pub fn cellular_color(
        &self,
        material_identifier: MaterialIdentifier,
        cell: CellCoordinates,
        color: u32,
    ) -> u32 {
        let index: usize = material_identifier.index() as usize;
        let albedo: u32 = match material_identifier.form() {
            MaterialForm::CellularStatic | MaterialForm::CellularDynamic => {
                let albedo: Option<&MaterialAlbedo> = if material_identifier.form()
                    == MaterialForm::CellularStatic
                {
                    self.cellular_static_albedos.get(index)
                } else {
                    self.cellular_dynamic_albedos.get(index)
                };
                let Some(albedo) = albedo else { return color; };
                if albedo.pixels.is_empty() { return color; }
                // repeat the frame across the scene from the origin, flipping rows since frames
                // start at the top while cell rows count upward
                let repetition_x: f64 =
                    ((cell.x as f64 + 0.5) / albedo.repetition_width).rem_euclid(1.0);
                let repetition_y: f64 =
                    ((cell.y as f64 + 0.5) / albedo.repetition_height).rem_euclid(1.0);
                let texel_x: u32 =
                    ((repetition_x * f64::from(albedo.width)) as u32).min(albedo.width - 1);
                let texel_y: u32 =
                    (((1.0 - repetition_y) * f64::from(albedo.height)) as u32)
                        .min(albedo.height - 1);
                albedo.pixels[(texel_y * albedo.width + texel_x) as usize]
            }
            MaterialForm::Fluid => match self.fluid_properties(material_identifier) {
                Some(properties) => properties.edit_color,
                None => return color,
            },
            MaterialForm::Gas => return color,
        };
        // white leaves the albedo unchanged, and other tints multiply it in linear light
        if color == u32::MAX { return albedo; }
        let [albedo_red, albedo_green, albedo_blue, albedo_alpha]: [u8; 4] = albedo.to_le_bytes();
        let [red, green, blue, alpha]: [u8; 4] = color.to_le_bytes();
        let to_linear = |value: u8| -> f32 {
            let value: f32 = f32::from(value) / 255.0;
            if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        let multiply = |albedo: u8, tint: u8| -> u8 {
            let value: f32 = to_linear(albedo) * to_linear(tint);
            let value: f32 = if value <= 0.0031308 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            };
            (value * 255.0).round().clamp(0.0, 255.0) as u8
        };
        u32::from_le_bytes([
            multiply(albedo_red, red),
            multiply(albedo_green, green),
            multiply(albedo_blue, blue),
            ((u16::from(albedo_alpha) * u16::from(alpha) + 127) / 255) as u8,
        ])
    }

    /// Returns the radiance a cell or fluid particle is created with from its material, or the
    /// supplied radiance when it is not `u32::MAX`
    pub fn cellular_radiance(
        &self,
        material_identifier: MaterialIdentifier,
        cell: CellCoordinates,
        radiance: u32,
    ) -> u32 {
        if radiance != u32::MAX { return radiance; }
        let index: usize = material_identifier.index() as usize;
        match material_identifier.form() {
            MaterialForm::CellularStatic | MaterialForm::CellularDynamic => {
                let radiance_data: Option<&MaterialRadiance> = if material_identifier.form()
                    == MaterialForm::CellularStatic
                {
                    self.cellular_static_radiances.get(index)
                } else {
                    self.cellular_dynamic_radiances.get(index)
                };
                let Some(radiance_data) = radiance_data else { return radiance; };
                if radiance_data.pixels.is_empty() { return 0; }
                // repeat the frame across the scene from the origin, flipping rows since frames
                // start at the top while cell rows count upward
                let repetition_x: f64 =
                    ((cell.x as f64 + 0.5) / radiance_data.repetition_width).rem_euclid(1.0);
                let repetition_y: f64 =
                    ((cell.y as f64 + 0.5) / radiance_data.repetition_height).rem_euclid(1.0);
                let texel_x: u32 =
                    ((repetition_x * f64::from(radiance_data.width)) as u32)
                        .min(radiance_data.width - 1);
                let texel_y: u32 =
                    (((1.0 - repetition_y) * f64::from(radiance_data.height)) as u32)
                        .min(radiance_data.height - 1);
                radiance_data.pixels[(texel_y * radiance_data.width + texel_x) as usize]
            }
            MaterialForm::Fluid => self.fluid_properties(material_identifier)
                .map_or(radiance, |properties| properties.edit_radiance),
            MaterialForm::Gas => self.gas_properties(material_identifier)
                .map_or(radiance, |properties| properties.graphics.radiance),
        }
    }

    /// Returns the temperature in kelvin edits create a registered material with
    pub fn edit_temperature(&self, identifier: MaterialIdentifier) -> Option<f32> {
        Some(match self.get(identifier)? {
            Material::CellularStatic { properties, .. } => properties.thermal.edit_temperature,
            Material::CellularDynamic { properties, .. } => properties.thermal.edit_temperature,
            Material::Fluid { properties, .. } => properties.thermal.edit_temperature,
            Material::Gas { properties, .. } => properties.thermal.edit_temperature,
        })
    }

    /// Returns the identifier for an exact, case-sensitive material name
    pub fn lookup(&self, name: &str) -> Option<MaterialIdentifier> {
        self.identifiers_by_name.get(name).copied()
    }

    /// Iterates over materials in identifier order
    pub fn materials(&self) -> impl Iterator<Item = (MaterialIdentifier, &str, MaterialForm)> {
        let mut materials: Vec<(MaterialIdentifier, &str, MaterialForm)> = Vec::new();
        for (form, first_index) in [
            (MaterialForm::CellularStatic, 1),
            (MaterialForm::CellularDynamic, 0),
            (MaterialForm::Fluid, 0),
            (MaterialForm::Gas, 0),
        ] {
            for index in first_index.. {
                let Ok(identifier) = MaterialIdentifier::new(form, index) else { break; };
                let Some(material) = self.material_registry.get(identifier) else { break; };
                materials.push((identifier, material.name(), form));
            }
        }
        materials.into_iter()
    }

    /// Returns the resolved cellular dynamic debris material of a static material
    pub fn debris(&self, identifier: MaterialIdentifier) -> Option<MaterialIdentifier> {
        self.debris_by_identifier.get(&identifier).copied()
    }

    /// Returns the material represented by a valid registered identifier
    pub fn get(&self, identifier: MaterialIdentifier) -> Option<&Material> {
        self.material_registry.get(identifier)
    }

    /// Returns the number of registered materials
    pub fn material_count(&self) -> usize {
        self.material_registry.material_count()
    }

    /// Returns the properties of a registered cellular static material
    pub fn cellular_static_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesCellularStatic> {
        self.material_registry.cellular_static_properties(identifier)
    }

    /// Returns the properties of a registered cellular dynamic material
    pub fn cellular_dynamic_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesCellularDynamic> {
        self.material_registry.cellular_dynamic_properties(identifier)
    }

    /// Returns the properties of a registered fluid material
    pub fn fluid_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesFluid> {
        self.material_registry.fluid_properties(identifier)
    }

    /// Returns the properties of a registered gas material
    pub fn gas_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesGas> {
        self.material_registry.gas_properties(identifier)
    }

    /// Returns static cellular GPU properties as mass, absorption, strength, and debris material
    /// identifier bits, zero for none
    pub fn cellular_static_properties_buffer(&self) -> &wgpu::Buffer {
        &self.cellular_static_properties_buffer
    }

    /// Returns dynamic cellular GPU properties as mass, friction, restitution, and padding
    pub fn cellular_dynamic_properties_buffer(&self) -> &wgpu::Buffer {
        &self.cellular_dynamic_properties_buffer
    }

    /// Returns fluid GPU properties as density, viscosity, cohesion, artificial pressure, drag,
    /// friction, restitution, and packed color bits
    pub fn fluid_properties_buffer(&self) -> &wgpu::Buffer {
        &self.fluid_properties_buffer
    }

    /// Returns gas GPU properties as mass, diffusivity, cohesion, and packed color bits, then
    /// opacity, packed radiance bits, and padding
    pub fn gas_properties_buffer(&self) -> &wgpu::Buffer {
        &self.gas_properties_buffer
    }

    /// Returns thermal GPU properties as heat capacity, conduction, then temperature, latent heat,
    /// product identifier bits, and amount of the hot and cold transitions, flags with bit zero
    /// for a hot transition, bit one for a cold one, and bit two for materials whose amount the
    /// fluid raster tracks, and padding, indexed by thermal index
    pub fn thermal_properties_buffer(&self) -> &wgpu::Buffer {
        &self.thermal_properties_buffer
    }

    /// Returns the color then radiance transformations of each material's hot then cold transition
    /// as row-major matrix, offset, channel variance, intensity variance, and padding, indexed by
    /// thermal index
    pub fn transition_color_buffer(&self) -> &wgpu::Buffer {
        &self.transition_color_buffer
    }

    /// Returns five `vec4<u32>` records per reaction: rate, heat, pressure, and counts; condition
    /// ranges; reactant amounts; then the three product identifier and amount pairs
    pub fn reaction_buffer(&self) -> &wgpu::Buffer { &self.reaction_buffer }

    /// Returns 42 `vec4<f32>` records per reaction, with seven color and seven radiance vectors
    /// for each of three product slots
    pub fn reaction_color_buffer(&self) -> &wgpu::Buffer { &self.reaction_color_buffer }

    /// Returns `N*N` pair headers, `N` unary headers, `N` reaction flags (bit 0 unary, bit 1
    /// paired), and `N` partner class masks (bit `partner % 32` per reacting partner) followed by
    /// reaction indices in registration order; each header packs the first list word and count
    pub fn reaction_lookup_buffer(&self) -> &wgpu::Buffer { &self.reaction_lookup_buffer }

    /// Returns temperature-key headers and key data: one `vec4<u32>` per thermal index containing
    /// the first key vector index, key count, and two zero words, then eleven `vec4<f32>` per key
    /// containing temperature, color matrix rows and offset, then radiance matrix rows and offset;
    /// indices count `vec4`s from the buffer start, whose bytes are compatible with `array<vec4<u32>>`
    /// and `array<vec4<f32>>` storage bindings through bitcasts
    pub fn temperature_color_buffer(&self) -> &wgpu::Buffer {
        &self.temperature_color_buffer
    }

    /// Returns one `vec4<u32>` header per thermal index followed by eleven `vec4<f32>` records per
    /// amount key: amount, color matrix and offset, then radiance matrix and offset
    pub fn amount_color_buffer(&self) -> &wgpu::Buffer { &self.amount_color_buffer }

    /// Returns the number of compiled reactions
    pub fn reaction_count(&self) -> u32 { self.reaction_count }

    /// Returns whether any reaction is registered
    pub fn has_reactions(&self) -> bool { self.reaction_count > 0 }

    /// Returns whether any reaction product is a fluid
    pub fn has_fluid_reaction_products(&self) -> bool { self.has_fluid_reaction_products }

    /// Returns the first thermal index of the static, dynamic, fluid, and gas forms
    pub fn thermal_offsets(&self) -> [u32; 4] { self.thermal_offsets }

    /// Returns whether any registered transition starts or ends in a fluid
    pub fn has_fluid_transitions(&self) -> bool { self.has_fluid_transitions }

}

/// Thermal record flag of materials whose amount a fluid raster tracks per cell, because they
/// react or have amount colors
const THERMAL_FLAG_TRACKED: u32 = 4;

/// Appends the seven `vec4<f32>` records of a color transformation
fn append_transform(bytes: &mut Vec<u8>, transformation: &MaterialColorTransformation) {
    for value in transformation.matrix.iter().flatten()
        .chain(&transformation.offset)
        .chain(&transformation.channel_variance)
        .chain([&transformation.intensity_variance, &0.0, &0.0, &0.0])
    {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

/// Appends an amount or temperature key and its two visual transformations
fn append_visual_key(
    bytes: &mut Vec<u8>,
    value: f32,
    color: &MaterialColorTransformation,
    radiance: &MaterialColorTransformation,
) {
    for component in [value, 0.0, 0.0, 0.0] {
        bytes.extend_from_slice(&component.to_le_bytes());
    }
    append_visual_color(bytes, color);
    append_visual_color(bytes, radiance);
}

/// Appends a zero-variance transformation as its matrix rows and offset
fn append_visual_color(bytes: &mut Vec<u8>, transformation: &MaterialColorTransformation) {
    for value in transformation.matrix.iter().flatten().chain(&transformation.offset) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

/// Returns whether a visual key transformation has finite values and the required variances
fn valid_visual_transform(
    transformation: &MaterialColorTransformation,
    zero_variance: bool,
) -> bool {
    transformation.matrix.iter().flatten().chain(&transformation.offset)
        .all(|value| value.is_finite())
        && transformation.channel_variance.iter()
            .chain([&transformation.intensity_variance])
            .all(|value| {
                value.is_finite() && if zero_variance { *value == 0.0 } else { *value >= 0.0 }
            })
}

/// Stores an unbounded maximum as the largest finite float in the reaction table
fn finite_max_bits(value: f32) -> u32 {
    if value == f32::INFINITY { f32::MAX.to_bits() } else { value.to_bits() }
}

/// Returns the amount consumed by a reaction reactant
fn reactant_amount(reactant: &MaterialReactant) -> f32 {
    match reactant {
        MaterialReactant::Material { amount, .. } | MaterialReactant::Tag { amount, .. } => *amount,
    }
}
