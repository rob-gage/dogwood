// Copyright Rob Gage 2026

use std::collections::HashMap;

use egui::{
    Id,
    Response,
};

/// Responses collected from one game UI composition pass
#[derive(Clone, Debug, Default)]
pub struct UiResponses {
    internal: HashMap<Id, Response>,
}

impl UiResponses {

    /// Creates an empty response collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces the response for its exact egui identifier
    pub fn insert(&mut self, response: Response) {
        self.internal.insert(response.id, response);
    }

    /// Associates a native response with a component identifier without changing its focus ID
    pub fn insert_for(&mut self, component_id: Id, response: Response) {
        self.internal.insert(component_id, response);
    }

    /// Looks up a response by the identifier source used with egui::Id::new
    pub fn get(&self, id: impl std::hash::Hash + std::fmt::Debug) -> Option<&Response> {
        let element_id: Id = Id::new(id);
        self.get_by_id(element_id)
    }

    /// Looks up a response by its egui identifier
    pub fn get_by_id(&self, id: Id) -> Option<&Response> {
        self.internal.get(&id)
    }

    /// Iterates over response identifiers and responses
    pub fn iter(&self) -> impl Iterator<Item = (&Id, &Response)> {
        self.internal.iter()
    }

}
