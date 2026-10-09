// Copyright Rob Gage 2026

use std::fmt;

/// Invalid configuration or pool creation failure
#[derive(Debug)]
pub enum ProcessorError {
    /// Invalid Processor configuration
    InvalidConfiguration(String),
    /// Rayon could not create a configured pool
    RayonThreadPoolBuild {
        /// Pool that failed to build
        name: String,
        /// Rayon pool creation error
        source: rayon::ThreadPoolBuildError,
    },
}

impl fmt::Display for ProcessorError {

    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) =>
                write!(formatter, "Invalid Processor configuration: {message}"),
            Self::RayonThreadPoolBuild { name, source } =>
                write!(formatter, "Thread pool '{name}' could not be created: {source}"),
        }
    }

}