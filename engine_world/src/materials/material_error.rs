// Copyright Rob Gage 2026

/// An error returned while registering or compiling materials
#[derive(Debug)]
pub enum MaterialError {
    /// The form-specific identifier capacity has been exceeded in the `MaterialRegistry`
    CapacityExceeded,
    /// A material name cannot be empty
    EmptyName,
    /// A material with this name has already been registered
    DuplicateName {
        /// The duplicated material name
        name: String,
    },
    /// The material index cannot be represented by a `MaterialIdentifier`
    IdentifierIndexOutOfRange,
    /// A referenced material name is not registered
    UnknownMaterial {
        /// The referenced name that was not registered
        name: String,
        /// The material definition containing the reference
        referenced_by: String,
    },
    /// A reaction names a material that is not registered
    UnknownReactantMaterial {
        /// The referenced material name
        name: String,
    },
    /// No registered material carries a reaction tag
    UnknownTag {
        /// The tag carried by no material
        tag: String,
    },
    /// A reaction reads a tag parameter that none of its reactants' materials sets
    UnknownParameter {
        /// The `tag.parameter` name read by the reaction
        parameter: String,
        /// The reactants' materials that set no such parameter
        materials: Vec<String>,
    },
    /// A reaction product names a material that is not registered
    UnknownProduct {
        /// The referenced product name
        name: String,
    },
    /// A debris reference does not identify a cellular dynamic material
    InvalidDebris {
        /// The debris name that did not identify a dynamic cellular material
        name: String,
        /// The static material definition containing the reference
        referenced_by: String,
    },
    /// A transition contains invalid data
    InvalidTransition {
        /// The material definition containing the invalid transition
        name: String,
        /// Description of the invalid transition data
        message: String,
    },
    /// A reaction contains invalid data
    InvalidReaction {
        /// Description of the invalid reaction data
        message: String,
    },
    /// Graphics resources could not be compiled for a material table
    Compilation(String),
}
