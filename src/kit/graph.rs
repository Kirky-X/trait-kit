// Copyright © 2026 Kirky.X

//! Dependency graph with topological sort and cycle detection.

use std::any::TypeId;
use std::collections::{HashMap, VecDeque};

/// A node in the dependency graph.
pub struct ModuleEntry {
    /// The module's TypeId.
    pub type_id: TypeId,
    /// The module's diagnostic name.
    pub name: &'static str,
    /// (name, TypeId) pairs of modules this module depends on.
    pub dependencies: Vec<(&'static str, TypeId)>,
}

/// Dependency graph for topological sort and cycle detection.
pub struct DependencyGraph {
    entries: Vec<ModuleEntry>,
    index: HashMap<TypeId, usize>,
}

impl DependencyGraph {
    /// Create an empty graph.
    pub fn new() -> Self {
        DependencyGraph {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Add a module to the graph. Returns an error if the module is already registered.
    pub fn add(&mut self, entry: ModuleEntry) -> Result<(), &'static str> {
        if self.index.contains_key(&entry.type_id) {
            return Err(entry.name);
        }
        let idx = self.entries.len();
        self.index.insert(entry.type_id, idx);
        self.entries.push(entry);
        Ok(())
    }

    /// Validate the graph: check for missing dependencies and cycles.
    /// Returns the topologically sorted TypeIds on success.
    pub fn validate(&self) -> Result<Vec<TypeId>, GraphError> {
        // Check for missing dependencies
        for entry in &self.entries {
            for (dep_name, dep_id) in &entry.dependencies {
                if !self.index.contains_key(dep_id) {
                    return Err(GraphError::DependencyMissing {
                        module: entry.name,
                        missing: dep_name,
                    });
                }
            }
        }

        // Kahn's algorithm for topological sort + cycle detection
        let n = self.entries.len();
        let mut in_degree = vec![0usize; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (i, entry) in self.entries.iter().enumerate() {
            for (_dep_name, dep_id) in &entry.dependencies {
                if let Some(&dep_idx) = self.index.get(dep_id) {
                    adj[dep_idx].push(i);
                    in_degree[i] += 1;
                }
            }
        }

        let mut queue: VecDeque<usize> = VecDeque::new();
        for (i, deg) in in_degree.iter().enumerate().take(n) {
            if *deg == 0 {
                queue.push_back(i);
            }
        }

        let mut sorted = Vec::with_capacity(n);
        while let Some(node) = queue.pop_front() {
            sorted.push(self.entries[node].type_id);
            for &neighbor in &adj[node] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    queue.push_back(neighbor);
                }
            }
        }

        if sorted.len() != n {
            // Cycle detected — find the cycle for a useful error message
            let cycle = self.find_cycle();
            return Err(GraphError::CycleDetected { cycle });
        }

        Ok(sorted)
    }

    /// Find a cycle in the graph using DFS (for error reporting).
    fn find_cycle(&self) -> Vec<&'static str> {
        let n = self.entries.len();
        let mut visited = vec![0u8; n]; // 0=unvisited, 1=in-stack, 2=done
        let mut stack = Vec::new();
        let mut cycle_names = Vec::new();

        fn dfs(
            node: usize,
            entries: &[ModuleEntry],
            index: &HashMap<TypeId, usize>,
            visited: &mut [u8],
            stack: &mut Vec<usize>,
            cycle_names: &mut Vec<&'static str>,
        ) -> bool {
            visited[node] = 1;
            stack.push(node);

            for (_dep_name, dep_id) in &entries[node].dependencies {
                if let Some(&dep_idx) = index.get(dep_id) {
                    if visited[dep_idx] == 1 {
                        // Found cycle — extract it
                        let start = stack.iter().position(|&x| x == dep_idx).unwrap();
                        for &idx in &stack[start..] {
                            cycle_names.push(entries[idx].name);
                        }
                        cycle_names.push(entries[dep_idx].name);
                        return true;
                    }
                    if visited[dep_idx] == 0
                        && dfs(dep_idx, entries, index, visited, stack, cycle_names)
                    {
                        return true;
                    }
                }
            }

            stack.pop();
            visited[node] = 2;
            false
        }

        for i in 0..n {
            if visited[i] == 0
                && dfs(i, &self.entries, &self.index, &mut visited, &mut stack, &mut cycle_names)
            {
                return cycle_names;
            }
        }

        vec!["<unknown cycle>"]
    }

    /// Get the registered names of all dependencies for a module.
    pub fn dependency_names(&self, type_id: TypeId) -> Vec<&'static str> {
        if let Some(&idx) = self.index.get(&type_id) {
            self.entries[idx]
                .dependencies
                .iter()
                .map(|(name, _)| *name)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all entries in registration order.
    pub fn entries(&self) -> &[ModuleEntry] {
        &self.entries
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors from graph validation.
#[derive(Debug)]
pub enum GraphError {
    /// A module depends on an unregistered module.
    DependencyMissing {
        module: &'static str,
        missing: &'static str,
    },
    /// A dependency cycle was detected.
    CycleDetected { cycle: Vec<&'static str> },
}
