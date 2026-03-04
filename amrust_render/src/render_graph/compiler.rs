//! Graph compilation: dependency resolution and barrier insertion

use std::collections::{HashMap, HashSet, VecDeque};

use super::{GraphError, PassId, RenderGraph, ResourceHandle, ResourceId};

/// A pipeline barrier between passes
#[derive(Debug, Clone)]
pub struct Barrier {
    pub from_pass: PassId,
    pub to_pass: PassId,
    pub resource_id: ResourceId,
    pub transition: ResourceTransition,
}

/// Resource state transition type
#[derive(Debug, Clone)]
pub enum ResourceTransition {
    /// Transition to color attachment
    ColorAttachmentWrite,
    /// Transition to depth/stencil attachment (read-write)
    DepthAttachmentWrite,
    /// Transition to depth/stencil attachment (read-only)
    DepthAttachmentRead,
    /// Transition to shader read
    ShaderRead,
    /// Transition for shader write
    ShaderWrite,
    /// Transition for presentation
    Present,
}

/// Compiled graph ready for execution
#[derive(Debug)]
pub struct CompiledGraph {
    /// Topologically sorted pass execution order
    pub execution_order: Vec<PassId>,
    /// Barriers to insert between passes
    pub barriers: Vec<Barrier>,
    /// Resource lifetimes: (first_use_pass_index, last_use_pass_index)
    pub resource_lifetimes: HashMap<ResourceId, (usize, usize)>,
}

/// Resource state tracking during compilation
#[derive(Debug, Clone)]
struct ResourceState {
    last_writer: Option<PassId>,
    readers: Vec<PassId>,
    is_color_attachment: bool,
    is_depth_attachment: bool,
    is_read_only: bool,
}

/// Graph compiler for dependency resolution and barrier insertion
pub struct GraphCompiler;

impl GraphCompiler {
    /// Compile the graph: topological sort + barrier insertion
    pub fn compile(graph: &RenderGraph) -> Result<CompiledGraph, GraphError> {
        // Step 1: Build dependency graph and validate
        let deps = Self::build_dependency_graph(graph)?;

        // Step 2: Topological sort
        let order = Self::topological_sort(graph, &deps)?;

        // Step 3: Calculate resource lifetimes
        let lifetimes = Self::calculate_lifetimes(graph, &order);

        // Step 4: Insert barriers
        let barriers = Self::insert_barriers(graph, &order)?;

        Ok(CompiledGraph {
            execution_order: order,
            barriers,
            resource_lifetimes: lifetimes,
        })
    }

    /// Build pass dependency graph based on resource usage
    fn build_dependency_graph(
        graph: &RenderGraph,
    ) -> Result<HashMap<PassId, Vec<PassId>>, GraphError> {
        let mut deps: HashMap<PassId, Vec<PassId>> = HashMap::new();
        let mut resource_writers: HashMap<ResourceId, PassId> = HashMap::new();

        for (pass_id, pass) in &graph.passes {
            let mut pass_deps: HashSet<PassId> = HashSet::new();

            // A pass depends on the last writer of each of its inputs
            for input_id in &pass.inputs {
                if let Some(writer) = resource_writers.get(input_id) {
                    pass_deps.insert(*writer);
                }
            }

            // Depth attachment is also a dependency
            if let Some(depth) = &pass.depth_stencil_attachment
                && depth.depth_load_op == wgpu::LoadOp::Load
                && let Some(writer) = resource_writers.get(&depth.resource_id)
            {
                pass_deps.insert(*writer);
            }

            // Track this pass as writer of its outputs
            for output_id in &pass.outputs {
                if let Some(prev_writer) = resource_writers.get(output_id) {
                    // Check for WAW (write-after-write) hazard
                    return Err(GraphError::WriteAfterWrite {
                        resource: *output_id,
                        first: *prev_writer,
                        second: *pass_id,
                    });
                }
                resource_writers.insert(*output_id, *pass_id);
            }

            // Track depth as writer if it writes
            if let Some(depth) = &pass.depth_stencil_attachment
                && !depth.read_only
                && depth.depth_load_op != wgpu::LoadOp::Load
            {
                resource_writers.insert(depth.resource_id, *pass_id);
            }

            deps.insert(*pass_id, pass_deps.into_iter().collect());
        }

        Ok(deps)
    }

    /// Kahn's algorithm for topological sorting
    fn topological_sort(
        graph: &RenderGraph,
        deps: &HashMap<PassId, Vec<PassId>>,
    ) -> Result<Vec<PassId>, GraphError> {
        let mut in_degree: HashMap<PassId, usize> = HashMap::new();
        let mut adj_list: HashMap<PassId, Vec<PassId>> = HashMap::new();

        // Initialize
        for pass_id in graph.passes.keys() {
            in_degree.entry(*pass_id).or_insert(0);
            adj_list.entry(*pass_id).or_default();
        }

        // Build reverse adjacency list and count in-degrees
        for (pass_id, pass_deps) in deps {
            for dep in pass_deps {
                adj_list.entry(*dep).or_default().push(*pass_id);
                *in_degree.entry(*pass_id).or_insert(0) += 1;
            }
        }

        // Find all nodes with no dependencies
        let mut queue: VecDeque<PassId> = in_degree
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| *id)
            .collect();

        let mut result = Vec::new();

        while let Some(pass_id) = queue.pop_front() {
            result.push(pass_id);

            // Reduce in-degree of dependents
            if let Some(dependents) = adj_list.get(&pass_id) {
                for dependent in dependents {
                    if let Some(count) = in_degree.get_mut(dependent) {
                        *count -= 1;
                        if *count == 0 {
                            queue.push_back(*dependent);
                        }
                    }
                }
            }
        }

        // Check for cycles
        if result.len() != graph.passes.len() {
            return Err(GraphError::CycleDetected);
        }

        Ok(result)
    }

    /// Calculate first and last use of each resource
    fn calculate_lifetimes(
        graph: &RenderGraph,
        order: &[PassId],
    ) -> HashMap<ResourceId, (usize, usize)> {
        let mut lifetimes: HashMap<ResourceId, (usize, usize)> = HashMap::new();

        for (index, pass_id) in order.iter().enumerate() {
            if let Some(pass) = graph.passes.get(pass_id) {
                // Update lifetimes for all touched resources
                for resource_id in pass.inputs.iter().chain(pass.outputs.iter()) {
                    lifetimes
                        .entry(*resource_id)
                        .and_modify(|(first, last)| {
                            *first = (*first).min(index);
                            *last = (*last).max(index);
                        })
                        .or_insert((index, index));
                }

                if let Some(depth) = &pass.depth_stencil_attachment {
                    lifetimes
                        .entry(depth.resource_id)
                        .and_modify(|(first, last)| {
                            *first = (*first).min(index);
                            *last = (*last).max(index);
                        })
                        .or_insert((index, index));
                }
            }
        }

        lifetimes
    }

    /// Insert pipeline barriers between passes
    fn insert_barriers(graph: &RenderGraph, order: &[PassId]) -> Result<Vec<Barrier>, GraphError> {
        let mut barriers = Vec::new();
        let mut resource_states: HashMap<ResourceId, ResourceState> = HashMap::new();

        // Initialize states for all resources
        for (id, resource) in &graph.resources {
            if let ResourceHandle::Texture(_) = resource {
                resource_states.insert(
                    *id,
                    ResourceState {
                        last_writer: None,
                        readers: Vec::new(),
                        is_color_attachment: false,
                        is_depth_attachment: false,
                        is_read_only: false,
                    },
                );
            }
        }

        // Track transitions between passes
        for pass_id in order.iter() {
            let pass = graph
                .passes
                .get(pass_id)
                .ok_or(GraphError::InvalidPass(*pass_id))?;

            // Handle color attachments
            for attachment in &pass.color_attachments {
                if let Some(state) = resource_states.get_mut(&attachment.resource_id) {
                    // Need barrier if there's a previous writer
                    if let Some(writer) = state.last_writer {
                        barriers.push(Barrier {
                            from_pass: writer,
                            to_pass: *pass_id,
                            resource_id: attachment.resource_id,
                            transition: ResourceTransition::ColorAttachmentWrite,
                        });
                    }

                    state.is_color_attachment = true;
                    state.is_depth_attachment = false;
                    state.is_read_only = false;
                    state.last_writer = Some(*pass_id);
                    state.readers.clear();
                }
            }

            // Handle depth attachments
            if let Some(depth) = &pass.depth_stencil_attachment
                && let Some(state) = resource_states.get_mut(&depth.resource_id)
            {
                let transition = if depth.read_only {
                    ResourceTransition::DepthAttachmentRead
                } else {
                    ResourceTransition::DepthAttachmentWrite
                };

                // Insert barrier if there's a previous writer
                if let Some(writer) = state.last_writer
                    && writer != *pass_id
                {
                    barriers.push(Barrier {
                        from_pass: writer,
                        to_pass: *pass_id,
                        resource_id: depth.resource_id,
                        transition,
                    });
                }

                state.is_depth_attachment = true;
                state.is_color_attachment = false;
                state.is_read_only = depth.read_only;

                if !depth.read_only {
                    state.last_writer = Some(*pass_id);
                    state.readers.clear();
                } else {
                    state.readers.push(*pass_id);
                }
            }

            // Handle shader reads (non-attachment resources)
            for input_id in &pass.inputs {
                // Skip if already handled as attachment
                let is_attachment = pass
                    .color_attachments
                    .iter()
                    .any(|a| a.resource_id == *input_id)
                    || pass
                        .depth_stencil_attachment
                        .as_ref()
                        .map(|d| d.resource_id == *input_id)
                        .unwrap_or(false);

                if !is_attachment && let Some(state) = resource_states.get_mut(input_id) {
                    // If resource was previously written, we need a transition to shader read
                    if state.last_writer.is_some() {
                        let writer = state.last_writer.unwrap();
                        barriers.push(Barrier {
                            from_pass: writer,
                            to_pass: *pass_id,
                            resource_id: *input_id,
                            transition: ResourceTransition::ShaderRead,
                        });
                    }
                    state.readers.push(*pass_id);
                }
            }
        }

        Ok(barriers)
    }
}
