// Copyright © 2026 Kirky.X

//! Kit — the capability and configuration management center.
//!
//! Uses typestate pattern: `Kit` (unbuilt) → `Kit<Ready>` (after `build()`).

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::error::KitError;
use crate::core::meta::{AutoBuilder, BuildFn};

use super::graph::{DependencyGraph, GraphError, ModuleEntry};
use super::typemap::TypeMap;

/// Marker type for the unbuilt state.
pub struct Unbuilt;

/// Marker type for the ready (built) state.
pub struct Ready;

/// The capability and configuration management center.
pub struct Kit<S = Unbuilt> {
    builders: RefCell<HashMap<TypeId, BuildFn>>,
    graph: DependencyGraph,
    configs: TypeMap,
    capabilities: TypeMap,
    _state: std::marker::PhantomData<S>,
}

impl Kit {
    /// Create a new empty Kit.
    pub fn new() -> Self {
        Kit {
            builders: RefCell::new(HashMap::new()),
            graph: DependencyGraph::new(),
            configs: TypeMap::new(),
            capabilities: TypeMap::new(),
            _state: std::marker::PhantomData,
        }
    }

    /// Register a module for construction.
    pub fn register<M: AutoBuilder>(&mut self) -> Result<(), KitError> {
        let entry = ModuleEntry {
            type_id: TypeId::of::<M>(),
            name: M::NAME,
            dependencies: M::dependencies().iter().map(|(n, id)| (*n, *id)).collect(),
        };

        self.graph
            .add(entry)
            .map_err(|name| KitError::AlreadyRegistered { module: name })?;

        let build_fn: BuildFn = Box::new(|kit| {
            let capability = M::build(kit).map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(e)
            })?;
            Ok(Box::new(capability) as Box<dyn Any + Send + Sync>)
        });

        self.builders.borrow_mut().insert(TypeId::of::<M>(), build_fn);
        Ok(())
    }

    /// Set a configuration value.
    pub fn set_config<C: Send + Sync + Clone + 'static>(&self, config: C) {
        self.configs.insert(config);
    }

    /// Retrieve a capability. During build phase, returns already-built capabilities.
    pub fn require<M: AutoBuilder>(&self) -> Result<M::Capability, KitError> {
        self.require_capability::<M>()
    }

    /// Get a configuration value.
    pub fn config<C: Send + Sync + Clone + 'static>(&self) -> Result<C, KitError> {
        self.get_config::<C>()
    }

    /// Validate the dependency graph and build all modules in topological order.
    ///
    /// After this call, all capabilities are available via `require()`.
    pub fn build(self) -> Result<Kit<Ready>, KitError> {
        // Validate the graph
        let sorted = match self.graph.validate() {
            Ok(sorted) => sorted,
            Err(GraphError::DependencyMissing { module, missing }) => {
                return Err(KitError::DependencyMissing { module, missing });
            }
            Err(GraphError::CycleDetected { cycle }) => {
                return Err(KitError::CycleDetected { cycle });
            }
        };

        // Build all modules in topological order.
        // We borrow self immutably to pass &Kit to build functions,
        // and use RefCell to mutably access builders.
        {
            let kit_ref: &Self = &self;

            for type_id in &sorted {
                let build_fn = kit_ref.builders.borrow_mut().remove(type_id).ok_or(
                    KitError::MissingCapability {
                        key: kit_ref.module_name(*type_id),
                    },
                )?;

                let module_name = kit_ref.module_name(*type_id);

                let result = (build_fn)(kit_ref);
                match result {
                    Ok(boxed) => {
                        kit_ref.capabilities.insert_boxed(*type_id, boxed);
                    }
                    Err(e) => {
                        return Err(KitError::BuildFailed {
                            module: module_name,
                            source: e,
                        });
                    }
                }
            }
        }
        // All borrows dropped here. Now we can move fields out of self.

        Ok(Kit {
            builders: self.builders,
            graph: self.graph,
            configs: self.configs,
            capabilities: self.capabilities,
            _state: std::marker::PhantomData,
        })
    }

    fn module_name(&self, type_id: TypeId) -> &'static str {
        self.graph
            .entries()
            .iter()
            .find(|e| e.type_id == type_id)
            .map(|e| e.name)
            .unwrap_or("<unknown>")
    }
}

impl<S> Kit<S> {
    /// Retrieve a capability by its module type.
    fn require_capability<M: AutoBuilder>(&self) -> Result<M::Capability, KitError> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
            .ok_or(KitError::MissingCapability { key: M::NAME })
    }

    /// Get a configuration value.
    fn get_config<C: Send + Sync + Clone + 'static>(&self) -> Result<C, KitError> {
        self.configs
            .get_cloned::<C>()
            .ok_or(KitError::MissingConfig {
                key: std::any::type_name::<C>(),
            })
    }
}

impl Kit<Ready> {
    /// Retrieve a capability by its module type.
    pub fn require<M: AutoBuilder>(&self) -> Result<M::Capability, KitError> {
        self.require_capability::<M>()
    }

    /// Retrieve an optional capability. Returns `None` if not built.
    pub fn optional<M: AutoBuilder>(&self) -> Option<M::Capability> {
        let type_id = TypeId::of::<M>();
        self.capabilities.get_cloned_by_type_id::<M::Capability>(type_id)
    }

    /// Get a configuration value.
    pub fn config<C: Send + Sync + Clone + 'static>(&self) -> Result<C, KitError> {
        self.get_config::<C>()
    }

    /// Check if a capability has been built.
    pub fn contains<M: AutoBuilder>(&self) -> bool {
        self.capabilities.contains_by_type_id(TypeId::of::<M>())
    }

    /// Check if a config is registered.
    pub fn contains_config<C: Send + Sync + Clone + 'static>(&self) -> bool {
        self.configs.get_cloned::<C>().is_some()
    }
}

impl Default for Kit {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Kit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kit").finish()
    }
}

impl std::fmt::Debug for Kit<Ready> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kit<Ready>").finish()
    }
}
