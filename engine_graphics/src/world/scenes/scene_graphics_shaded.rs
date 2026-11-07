// Copyright Rob Gage 2026

/// One body drawn over the cells with a shader the game supplies
#[derive(Clone, Debug, PartialEq)]
pub struct SceneGraphicsShaded<'a> {
    /// WGSL source of the `shade` function
    pub fragment: &'a str,
    /// Values for the shader
    pub parameters: [[f32; 4]; 4],
    /// Whether it is shaded and outlined in whole world cells
    pub is_cell_snapped: bool,
    /// Its closed outline relative to the camera in tiles, its inside to the left
    pub outline: Vec<[f32; 2]>,
}
