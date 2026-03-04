//! Error types for render graph operations

use super::{PassId, ResourceId};

/// Errors that can occur during graph construction or compilation
#[derive(Debug, Clone)]
pub enum GraphError {
    /// Detected a cycle in the dependency graph
    CycleDetected,

    /// A resource was written to by multiple passes (WAW hazard)
    WriteAfterWrite {
        resource: ResourceId,
        first: PassId,
        second: PassId,
    },

    /// Referenced a resource that doesn't exist in the graph
    MissingResource(ResourceId),

    /// Referenced a pass that doesn't exist in the graph
    InvalidPass(PassId),

    /// Resource not in the expected state
    InvalidResourceState {
        resource: ResourceId,
        expected: String,
        actual: String,
    },

    /// Dependency validation failed
    InvalidDependency {
        from: PassId,
        to: PassId,
        reason: String,
    },

    /// Resource allocation failed
    AllocationFailed {
        resource: ResourceId,
        reason: String,
    },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::CycleDetected => write!(f, "Cycle detected in render graph"),
            GraphError::WriteAfterWrite {
                resource,
                first,
                second,
            } => {
                write!(
                    f,
                    "Resource {:?} written by both pass {:?} and pass {:?}",
                    resource, first, second
                )
            }
            GraphError::MissingResource(id) => write!(f, "Missing resource {:?}", id),
            GraphError::InvalidPass(id) => write!(f, "Invalid pass {:?}", id),
            GraphError::InvalidResourceState {
                resource,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Resource {:?} in invalid state: expected {}, actual {}",
                    resource, expected, actual
                )
            }
            GraphError::InvalidDependency { from, to, reason } => {
                write!(
                    f,
                    "Invalid dependency from {:?} to {:?}: {}",
                    from, to, reason
                )
            }
            GraphError::AllocationFailed { resource, reason } => {
                write!(f, "Failed to allocate resource {:?}: {}", resource, reason)
            }
        }
    }
}

impl std::error::Error for GraphError {}
