// Copyright Rob Gage 2026

use std::sync::Arc;

use super::Scene;
use crate::scenes::SceneGenerator;

impl Scene {
    pub(crate) fn generator(&self) -> &dyn SceneGenerator {
        self.generator.as_ref()
    }

    /// Sets the scene generator used for creating missing chunks.
    pub fn with_generator(mut self, generator: impl SceneGenerator + 'static) -> Self {
        self.generator = Arc::new(generator);
        self
    }
}
