// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Dependency graph with topological sort and cycle detection.

use std::any::TypeId;
use std::collections::{HashMap, VecDeque};

/// A node in the dependency graph.
#[derive(Debug, Clone)]
pub struct ModuleEntry {
    /// The module's `TypeId`.
    pub type_id: TypeId,
    /// The module's diagnostic name.
    pub name: &'static str,
    /// (name, `TypeId`) pairs of modules this module depends on.
    pub dependencies: Vec<(&'static str, TypeId)>,
}

/// Dependency graph for topological sort and cycle detection.
#[derive(Debug)]
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
    /// Returns the name of the **already registered** module if the given
    /// entry's `TypeId` is a duplicate (i.e. the conflicting party, so
    /// callers can locate the source of the conflict), not the new entry's
    /// name.
    pub fn add(&mut self, entry: ModuleEntry) -> Result<(), &'static str> {
        if let Some(&existing_idx) = self.index.get(&entry.type_id) {
            // 报告冲突方：返回已注册条目的名字，而非本次被拒绝的新条目名。
            return Err(self.entries[existing_idx].name);
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
        for (i, deg) in in_degree.iter().enumerate() {
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
            stack_pos: &mut HashMap<usize, usize>,
            cycle_names: &mut Vec<&'static str>,
        ) -> bool {
            visited[node] = 1;
            stack_pos.insert(node, stack.len());
            stack.push(node);

            for (_dep_name, dep_id) in &entries[node].dependencies {
                if let Some(&dep_idx) = index.get(dep_id) {
                    if visited[dep_idx] == 1 {
                        // Found cycle — O(1) lookup via stack_pos map
                        let Some(&start) = stack_pos.get(&dep_idx) else {
                            // Invariant violation: dep_idx should be in the
                            // stack when visited[dep_idx] == 1. Fall back to
                            // a generic cycle report instead of panicking.
                            cycle_names.push(entries[dep_idx].name);
                            cycle_names.push(entries[node].name);
                            return true;
                        };
                        for &idx in &stack[start..] {
                            cycle_names.push(entries[idx].name);
                        }
                        cycle_names.push(entries[dep_idx].name);
                        return true;
                    }
                    if visited[dep_idx] == 0
                        && dfs(
                            dep_idx,
                            entries,
                            index,
                            visited,
                            stack,
                            stack_pos,
                            cycle_names,
                        )
                    {
                        return true;
                    }
                }
            }

            stack.pop();
            stack_pos.remove(&node);
            visited[node] = 2;
            false
        }

        let n = self.entries.len();
        let mut visited = vec![0u8; n]; // 0=unvisited, 1=in-stack, 2=done
        let mut stack = Vec::with_capacity(n);
        let mut stack_pos = HashMap::with_capacity(n);
        let mut cycle_names = Vec::new();

        for i in 0..n {
            if visited[i] == 0
                && dfs(
                    i,
                    &self.entries,
                    &self.index,
                    &mut visited,
                    &mut stack,
                    &mut stack_pos,
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
            let _ = writeln!(out, "    \"{}\";", escape_label(entry.name));
        }
        // Edges: dependency → dependent
        for entry in &self.entries {
            for (dep_name, _) in &entry.dependencies {
                let _ = writeln!(
                    out,
                    "    \"{}\" -> \"{}\";",
                    escape_label(dep_name),
                    escape_label(entry.name)
                );
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
        // Use index-based node IDs to avoid collisions when names contain
        // hyphens or other special characters (e.g. 'my-module' vs 'my_module').
        for (idx, entry) in self.entries.iter().enumerate() {
            for (dep_name, dep_id) in &entry.dependencies {
                // O(1) index lookup instead of a linear scan by name.
                // When the dependency is not registered, skip the edge:
                // missing dependencies are `validate()`'s job
                // (`DependencyMissing`), so we must not emit a misleading
                // self-loop here.
                let Some(dep_idx) = self.index.get(dep_id).copied() else {
                    continue;
                };
                let _ = writeln!(
                    out,
                    "    n{dep_idx}[\"{}\"] --> n{idx}[\"{}\"]",
                    escape_label(dep_name),
                    escape_label(entry.name)
                );
            }
        }
        // Ensure nodes with no dependencies still appear
        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.dependencies.is_empty() {
                let _ = writeln!(out, "    n{idx}[\"{}\"]", escape_label(entry.name));
            }
        }
        out
    }
}

/// Escape a module name for use inside a DOT/Mermaid double-quoted label.
///
/// Follows DOT string-literal semantics:
/// - `\` → `\\`
/// - `"` → `\"`
/// - newline → `\n` (literal backslash + `n`, the DOT line-break escape)
///
/// Mermaid accepts the same escapes inside its double-quoted node labels.
fn escape_label(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors from graph validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    /// A module depends on an unregistered module.
    DependencyMissing {
        module: &'static str,
        missing: &'static str,
    },
    /// A dependency cycle was detected.
    CycleDetected { cycle: Vec<&'static str> },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DependencyMissing { module, missing } => {
                write!(
                    f,
                    "module `{module}` depends on unregistered module `{missing}`"
                )
            }
            Self::CycleDetected { cycle } => {
                write!(f, "dependency cycle detected: {}", cycle.join(" -> "))
            }
        }
    }
}

impl std::error::Error for GraphError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    /// Each module needs a unique `TypeId`, so we use distinct zero-sized types.
    mod types {
        pub struct A;
        pub struct B;
        pub struct C;
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
        // 语义：Err 携带**已注册（冲突方）**的名字 "a"，而非被拒绝的新条目名
        // "a2"，便于调用方定位冲突来源（kit.rs 将其映射为 AlreadyRegistered）。
        let err = g.add(typed_entry::<types::A>("a2", vec![])).unwrap_err();
        assert_eq!(err, "a");
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
        let a_idx = sorted
            .iter()
            .position(|t| *t == TypeId::of::<types::A>())
            .unwrap();
        let b_idx = sorted
            .iter()
            .position(|t| *t == TypeId::of::<types::B>())
            .unwrap();
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
        // Index-based node IDs: n0 for "a", n1 for "b"
        assert!(mermaid.contains("n0[\"a\"]"));
        assert!(mermaid.contains("n1[\"b\"]"));
        assert!(mermaid.contains("-->"));
    }

    #[test]
    fn graph_to_mermaid_hyphen_names_no_collision() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("my-module", vec![])).unwrap();
        g.add(typed_entry::<types::B>(
            "my-dep",
            vec![("my-module", TypeId::of::<types::A>())],
        ))
        .unwrap();
        let mermaid = g.to_mermaid();
        // Index-based IDs avoid collision between hyphens and underscores
        assert!(mermaid.contains("n0[\"my-module\"]"));
        assert!(mermaid.contains("n1[\"my-dep\"]"));
        // Original names (with hyphens) are preserved in display labels
        assert!(mermaid.contains("my-module"));
        assert!(mermaid.contains("my-dep"));
    }

    #[test]
    fn graph_to_mermaid_unregistered_dependency_no_self_loop() {
        // "a" 依赖未注册的 "ghost"（TypeId 属于未入图的 types::B）。
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>(
            "a",
            vec![("ghost", TypeId::of::<types::B>())],
        ))
        .unwrap();
        let mermaid = g.to_mermaid();
        // 缺失依赖由 validate() 负责（DependencyMissing），导出不得伪造自环；
        // 唯一的边被跳过后 "a" 不再出现在导出中（图本身非法，属可接受行为）。
        assert!(
            !mermaid.contains("-->"),
            "unregistered dependency must be skipped, got: {mermaid}"
        );
        assert!(
            !mermaid.contains("n0 --> n0"),
            "self-loop must not appear, got: {mermaid}"
        );
    }

    /// 校验导出文本的转义合法性：引号成对（忽略 `\"` 转义引号）、
    /// 反斜杠只出现在 `\\`、`\"`、`\n` 三种转义序列中。
    fn assert_label_escapes_valid(output: &str) {
        let mut escaped = false;
        let mut unescaped_quotes = 0usize;
        for c in output.chars() {
            if escaped {
                assert!(
                    matches!(c, '\\' | '"' | 'n'),
                    "bare backslash escape \\{c} in output: {output}"
                );
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                unescaped_quotes += 1;
            }
        }
        assert!(!escaped, "trailing backslash in output: {output}");
        assert_eq!(
            unescaped_quotes % 2,
            0,
            "unpaired quotes in output: {output}"
        );
    }

    #[test]
    fn graph_to_mermaid_unregistered_and_registered_dependencies_mixed() {
        let mut g = DependencyGraph::new();
        // "b" 同时依赖已注册的 "a" 与未注册的 "ghost"。
        g.add(typed_entry::<types::A>("a", vec![])).unwrap();
        g.add(typed_entry::<types::B>(
            "b",
            vec![
                ("a", TypeId::of::<types::A>()),
                ("ghost", TypeId::of::<types::C>()),
            ],
        ))
        .unwrap();
        let mermaid = g.to_mermaid();
        // 已注册依赖的边正常输出
        assert!(mermaid.contains("n0[\"a\"] --> n1[\"b\"]"));
        // 未注册依赖的边被跳过，且绝无自环
        assert_eq!(mermaid.matches("-->").count(), 1, "got: {mermaid}");
        assert!(!mermaid.contains("n1 --> n1"));
    }

    #[test]
    fn graph_to_dot_escapes_special_characters() {
        // 名字含双引号、反斜杠与换行
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("we\"ird\\name", vec![]))
            .unwrap();
        g.add(typed_entry::<types::B>(
            "line\nbreak",
            vec![("we\"ird\\name", TypeId::of::<types::A>())],
        ))
        .unwrap();
        let dot = g.to_dot();

        // 引号被转义为 \"，节点行形如 "we\"ird\\name";
        assert!(dot.contains("\"we\\\"ird\\\\name\""), "got: {dot}");
        // 换行被转义为字面 \n（反斜杠 + n），不再是真实换行符
        assert!(dot.contains("\"line\\nbreak\""), "got: {dot}");
        // 整份输出仍是合法 DOT：引号成对、反斜杠均为合法转义序列
        assert_label_escapes_valid(&dot);
    }

    #[test]
    fn graph_to_mermaid_escapes_special_characters() {
        let mut g = DependencyGraph::new();
        g.add(typed_entry::<types::A>("q\"uote", vec![])).unwrap();
        let mermaid = g.to_mermaid();
        assert!(mermaid.contains("n0[\"q\\\"uote\"]"), "got: {mermaid}");
        assert_label_escapes_valid(&mermaid);
    }

    #[test]
    fn graph_error_display_and_std_error() {
        let err = GraphError::DependencyMissing {
            module: "a",
            missing: "b",
        };
        let msg = format!("{err}");
        assert!(msg.contains('a') && msg.contains('b'), "got: {msg}");

        let cycle = GraphError::CycleDetected {
            cycle: vec!["a", "b", "a"],
        };
        let cycle_msg = format!("{cycle}");
        assert!(cycle_msg.contains("a -> b"), "got: {cycle_msg}");

        // 可作为 `dyn std::error::Error` 使用（错误处理生态兼容）
        let dyn_err: &dyn std::error::Error = &err;
        assert!(dyn_err.to_string().contains("unregistered"));
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
