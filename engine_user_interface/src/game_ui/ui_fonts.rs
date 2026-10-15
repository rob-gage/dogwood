// Copyright Rob Gage 2026

use std::sync::Arc;

use egui::{
    Context,
    FontData,
    FontDefinitions,
    FontFamily,
};

/// Owns game UI font definitions and named font families
#[derive(Clone, Debug, PartialEq)]
pub struct UiFonts {
    font_definitions: FontDefinitions,
}

impl UiFonts {

    /// Creates font definitions from an application-customized initial set
    pub fn new(font_definitions: FontDefinitions) -> Self {
        Self {
            font_definitions,
        }
    }

    /// Creates font definitions from the egui bundled defaults
    pub fn from_definitions(font_definitions: FontDefinitions) -> Self {
        Self::new(font_definitions)
    }

    /// Borrows the current font definitions
    pub fn definitions(&self) -> &FontDefinitions {
        &self.font_definitions
    }

    /// Borrows the current font definitions mutably
    pub fn definitions_mut(&mut self) -> &mut FontDefinitions {
        &mut self.font_definitions
    }

    /// Registers a font and returns its named family with proportional fallback
    pub fn register(&mut self, name: impl Into<String>, bytes: Vec<u8>) -> FontFamily {
        let font_name: String = name.into();
        let font_family: FontFamily = FontFamily::Name(Arc::<str>::from(font_name.as_str()));
        let proportional_fallbacks: Vec<String> = self
            .font_definitions
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        let font_data: FontData = FontData::from_owned(bytes);
        self.font_definitions
            .font_data
            .insert(font_name.clone(), Arc::new(font_data));
        let family_fonts: &mut Vec<String> = self
            .font_definitions
            .families
            .entry(font_family.clone())
            .or_default();
        family_fonts.retain(|registered_name: &String| registered_name != &font_name);
        family_fonts.insert(0, font_name.clone());
        for fallback_name in proportional_fallbacks {
            if fallback_name != font_name && !family_fonts.contains(&fallback_name) {
                family_fonts.push(fallback_name);
            }
        }
        font_family
    }

    /// Installs the definitions into an egui context after registration changes
    pub fn install(&self, context: &Context) {
        context.set_fonts(self.font_definitions.clone());
    }

}

impl Default for UiFonts {

    fn default() -> Self {
        Self::new(FontDefinitions::default())
    }

}
