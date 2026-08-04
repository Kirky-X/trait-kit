// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Dependency graph with topological sort and cycle detection.

use std::any::TypeId;
use std::collections::{HashMap, VecDeque};

/// A node in the dependency graph.
pub struct ModuleEntry {
    /// The module's `TypeId`.
    pub type_id: TypeId,
    /// The module's diagnostic name.
    pub name: &'static str,
    /// (name, `TypeId`) pairs of modules this module depends on.
    pub dependencies: Vec<(&'static str, TypeId)>,
}

/// Dependency graph for topological sort and cycle detection.
pub struct DependencyGraph {
    entries: Vec<ModuleEntry>,
    index: HashMap<TypeId, usize>,
}

impl DependencyGraph {
    /// Create an empty graph.
    #[must_use]
    pub fn new() -> Self {
        DependencyGraph {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Add a module to the graph.
    ///
    /// # Errors
    ///
    /// Returns the module's name if it is already registered.
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
    /// Returns the topologically sorted `TypeIds` on success.
    ///
    /// # Errors
    ///
    /// Returns `GraphError::DependencyMissing` if a module depends on an unregistered module.
    /// Returns `GraphError::CycleDetected` if a dependency cycle is found.
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
                        let start = stack
                            .iter()
                            .position(|&x| x == dep_idx)
                            .expect("invariant: dep_idx must be in stack (visited[dep_idx] == 1)");
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

        let n = self.entries.len();
        let mut visited = vec![0u8; n]; // 0=unvisited, 1=in-stack, 2=done
        let mut stack = Vec::new();
        let mut cycle_names = Vec::new();

        for i in 0..n {
            if visited[i] == 0
                && dfs(
                    i,
                    &self.entries,
                    &self.index,
                    &mut visited,
                    &mut stack,
                    &mut cycle_names,
                )
            {
                return cycle_names;
            }
        }

        vec!["<unknown cycle>"]
    }

    /// Get the registered names of all dependencies for a module.
    #[must_use]
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
    #[must_use]
    pub fn entries(&self) -> &[ModuleEntry] {
        &self.entries
    }

    /// Look up a module's diagnostic name by `TypeId` in O(1).
    #[must_use]
    pub fn name_of(&self, type_id: TypeId) -> Option<&'static str> {
        self.index.get(&type_id).map(|&idx| self.entries[idx].name)
    }

    /// Export the dependency graph as a Graphviz DOT format string.
    ///
    /// Nodes are module names; directed edges represent dependencies
    /// (dependency → dependent).
    #[must_use]
    pub fn to_dot(&self) -> String {
        use std::fmt::Write as _;
        if self.entries.is_empty() {
            return "digraph {}".to_string();
        }
        let mut out = String::from("digraph {\n");
        // Nodes
        for entry in &self.entries {
            let _ = writeln!(out, "    \"{}\";", entry.name);
        }
        // Edges: dependency → dependent
        for entry in &self.entries {
            for (dep_name, _) in &entry.dependencies {
                let _ = writeln!(out, "    \"{}\" -> \"{}\";", dep_name, entry.name);
            }
        }
        out.push('}');
        out
    }

    /// Export the dependency graph as a Mermaid flowchart format string.
    ///
    /// Uses `graph TD` (top-down) layout. Edges: dependency --> dependent.
    #[must_use]
    pub fn to_mermaid(&self) -> String {
        use std::fmt::Write as _;
        if self.entries.is_empty() {
            return "graph TD".to_string();
        }
        let mut out = String::from("graph TD\n");
        for entry in &self.entries {
            for (dep_name, _) in &entry.dependencies {
                // Mermaid node IDs: replace hyphens with underscores
                let from_id = dep_name.replace('-', "_");
                let to_id = entry.name.replace('-', "_");
                let _ = writeln!(
                    out,
                    "    {}[\"{}\"] --> {}[\"{}\"]",
                    from_id, dep_name, to_id, entry.name
                );
            }
        }
        // Ensure nodes with no dependencies still appear
        for entry in &self.entries {
            if entry.dependencies.is_empty() {
                let id = entry.name.replace('-', "_");
                let _ = writeln!(out, "    {}[\"{}\"]", id, entry.name);
            }
        }
        out
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    fn entry(name: &'static str, deps: Vec<(&'static str, TypeId)>) -> ModuleEntry {
        ModuleEntry {
            type_id: TypeId::of::<u8>(), // dummy — we use unique names instead
            name,
            dependencies: deps,
        }
    }

    /// Each module needs a unique TypeId, so we use distinct zero-sized types.
    mod types {
        pub struct A;
        pub struct B;
        pub struct C;
        pub struct D;
    }

    fn typed_entry<T: 'static>(
        name: &'static str,
        deps: Vec<(&'static str, TypeId)>,
    ) -> ModuleEntry {
        ModuleEntry {
            type_id: TypeId::of::<T>(),
            name,
            dependencies: deps,
        }
    }

    #[test]
    fn graph_new_is_empty() {
        let g = DependencyGraph::new();
        assert!(g.entries().is_empty());
    }

    #[test]
    fn graph_add_and_entries() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("a", vec![])).unwrap();
        assert_eq!(g.entries().len(), 1);
    }

    #[test]
    fn graph_add_duplicate_returns_err() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("a", vec![])).unwrap();
        let err = g.add(typed_entry::<types::A>("a2", vec![])).unwrap_err();
        assert_eq!(err, "a2");
    }

    #[test]
    fn graph_validate_empty_succeeds() {
        let g = DependencyGraph::new();
        let sorted = g.validate().unwrap();
        assert!(sorted.is_empty());
    }

    #[test]
    fn graph_validate_single_node() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("a", vec![])).unwrap();
        let sorted = g.validate().unwrap();
        assert_eq!(sorted.len(), 1);
    }

    #[test]
    fn graph_validate_missing_dependency() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>(
            "a",
            vec![("b", TypeId::of::<types::B>())],
        ))
        .unwrap();
        let err = g.validate().unwrap_err();
        assert!(matches!(
            err,
            GraphError::DependencyMissing {
                module: "a",
                missing: "b"
            }
        ));
    }

    #[test]
    fn graph_validate_cycle_two_nodes() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>(
            "a",
            vec![("b", TypeId::of::<types::B>())],
        ))
        .unwrap();
        g.add(typed_entry::<types::B>(
            "b",
            vec![("a", TypeId::of::<types::A>())],
        ))
        .unwrap();
        let err = g.validate().unwrap_err();
        assert!(matches!(err, GraphError::CycleDetected { .. }));
        if let GraphError::CycleDetected { cycle } = err {
            assert!(cycle.len() >= 2, "cycle should contain at least 2 names");
        }
    }

    #[test]
    fn graph_validate_cycle_three_nodes() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>(
            "a",
            vec![("b", TypeId::of::<types::B>())],
        ))
        .unwrap();
        g.add(typed_entry::<types::B>(
            "b",
            vec![("c", TypeId::of::<types::C>())],
        ))
        .unwrap();
        g.add(typed_entry::<types::C>(
            "c",
            vec![("a", TypeId::of::<types::A>())],
        ))
        .unwrap();
        let err = g.validate().unwrap_err();
        assert!(matches!(err, GraphError::CycleDetected { .. }));
    }

    #[test]
    fn graph_validate_topo_order() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("a", vec![])).unwrap();
        g.add(typed_entry::<types::B>(
            "b",
            vec![("a", TypeId::of::<types::A>())],
        ))
        .unwrap();
        let sorted = g.validate().unwrap();
        let a_idx = sorted.iter().position(|t| *t == TypeId::of::<types::A>()).unwrap();
        let b_idx = sorted.iter().position(|t| *t == TypeId::of::<types::B>()).unwrap();
        assert!(a_idx < b_idx, "a should be sorted before b");
    }

    #[test]
    fn graph_dependency_names() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("a", vec![])).unwrap();
        g.add(typed_entry::<types::B>(
            "b",
            vec![("a", TypeId::of::<types::A>())],
        ))
        .unwrap();
        let names = g.dependency_names(TypeId::of::<types::B>());
        assert_eq!(names, vec!["a"]);
    }

    #[test]
    fn graph_dependency_names_unknown_returns_empty() {
        let g = DependencyGraph::new();
        let names = g.dependency_names(TypeId::of::<types::A>());
        assert!(names.is_empty());
    }

    #[test]
    fn graph_name_of() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("module-a", vec![])).unwrap();
        assert_eq!(g.name_of(TypeId::of::<types::A>()), Some("module-a"));
        assert_eq!(g.name_of(TypeId::of::<types::B>()), None);
    }

    #[test]
    fn graph_default_is_empty() {
        let g = DependencyGraph::default();
        assert!(g.entries().is_empty());
    }

    #[test]
    fn graph_to_dot_empty() {
        let g = DependencyGraph::new();
        assert_eq!(g.to_dot(), "digraph {}");
    }

    #[test]
    fn graph_to_dot_with_nodes_and_edges() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("a", vec![])).unwrap();
        g.add(typed_entry::<types::B>(
            "b",
            vec![("a", TypeId::of::<types::A>())],
        ))
        .unwrap();
        let dot = g.to_dot();
        assert!(dot.starts_with("digraph {"));
        assert!(dot.contains("\"a\""));
        assert!(dot.contains("\"b\""));
        assert!(dot.contains("\"a\" -> \"b\""));
        assert!(dot.ends_with('}'));
    }

    #[test]
    fn graph_to_mermaid_empty() {
        let g = DependencyGraph::new();
        assert_eq!(g.to_mermaid(), "graph TD");
    }

    #[test]
    fn graph_to_mermaid_with_nodes_and_edges() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("a", vec![])).unwrap();
        g.add(typed_entry::<types::B>(
            "b",
            vec![("a", TypeId::of::<types::A>())],
        ))
        .unwrap();
        let mermaid = g.to_mermaid();
        assert!(mermaid.starts_with("graph TD"));
        assert!(mermaid.contains("a[\"a\"]"));
        assert!(mermaid.contains("b[\"b\"]"));
        assert!(mermaid.contains("-->"));
    }

    #[test]
    fn graph_to_mermaid_hyphen_replacement() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("my-module", vec![])).unwrap();
        g.add(typed_entry::<types::B>(
            "my-dep",
            vec![("my-module", TypeId::of::<types::A>())],
        ))
        .unwrap();
        let mermaid = g.to_mermaid();
        // Hyphens in names should be replaced with underscores for node IDs
        assert!(mermaid.contains("my_module"));
        assert!(mermaid.contains("my_dep"));
    }

    #[test]
    fn graph_error_debug() {
        let err = GraphError::DependencyMissing {
            module: "a",
            missing: "b",
        };
        let debug = format!("{err:?}");
        assert!(debug.contains("DependencyMissing"));

        let err2 = GraphError::CycleDetected {
            cycle: vec!["a", "b", "a"],
        };
        let debug2 = format!("{err2:?}");
        assert!(debug2.contains("CycleDetected"));
    }
}
