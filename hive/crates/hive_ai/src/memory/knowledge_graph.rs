//! GraphRAG v1: a lightweight knowledge-graph engine.
//!
//! This module provides a [`KnowledgeGraph`] that complements Hive's existing
//! vector memory. Where the vector store answers "what text is semantically
//! similar to this query?", the knowledge graph answers *structural* questions:
//! which entities are connected, how to get from one to another, which nodes
//! are hubs, and how the graph clusters into communities.
//!
//! The engine is deliberately generic and dependency-light. Nodes are
//! identified by opaque `String` ids (file paths, symbol names, note titles,
//! memory ids, ...) so callers in other crates can build a graph without taking
//! a dependency on any concrete domain type. An Obsidian `[[wiki-link]]`
//! adapter lives in `hive_integrations` (see that crate's `knowledge` module),
//! which produces `(from, to)` link pairs fed to [`KnowledgeGraph::from_link_pairs`].
//!
//! Backed by [`petgraph`], a pure-Rust graph library.
//!
//! # Out of scope for v1
//!
//! Wiring graph results into the live context engine / prompt assembly
//! (a `SourceType::Graph` variant consumed by `ContextEngine`) is intentionally
//! deferred. v1 is the engine + adapter + tests only.

use std::collections::HashMap;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::unionfind::UnionFind;
use petgraph::visit::EdgeRef;
use petgraph::Direction;

/// The kind of entity a graph node represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A source file.
    File,
    /// A code symbol (function, type, ...).
    Symbol,
    /// A knowledge note (e.g. an Obsidian page).
    Note,
    /// A stored memory entry.
    Memory,
    /// Anything else.
    Other,
}

/// The kind of relationship an edge represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// An explicit link (e.g. an Obsidian `[[wiki-link]]`).
    Link,
    /// A reference (e.g. a symbol referencing another).
    Reference,
    /// Two entities co-occurring in the same context.
    CoOccurrence,
}

/// Data stored on each graph node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Stable identifier for the node (path, name, id, ...).
    pub id: String,
    /// What kind of entity this node represents.
    pub kind: NodeKind,
    /// Human-readable label for display / explanation.
    pub label: String,
}

/// Data stored on each graph edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    /// What kind of relationship this edge represents.
    pub kind: EdgeKind,
    /// Edge weight. Higher means a stronger relationship; used as the cost
    /// basis for shortest-path queries (cost is `1.0 / weight`).
    pub weight: f64,
}

/// A directed knowledge graph of entities and their relationships.
///
/// Node insertion is idempotent: adding an id that already exists returns the
/// existing node rather than creating a duplicate.
#[derive(Debug, Clone, Default)]
pub struct KnowledgeGraph {
    graph: DiGraph<Node, Edge>,
    /// Map from node id to its index in the underlying graph.
    index: HashMap<String, NodeIndex>,
}

impl KnowledgeGraph {
    /// Create an empty knowledge graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Add a node, or return the existing node index if `id` is already present.
    ///
    /// Insertion is idempotent on `id`. If the node already exists its `kind`
    /// and `label` are left untouched (first writer wins), so re-adding a node
    /// that was auto-created as a link target does not clobber metadata set by
    /// a later, more specific `add_node` call — and vice-versa.
    pub fn add_node(&mut self, id: impl Into<String>, kind: NodeKind, label: impl Into<String>) -> NodeIndex {
        let id = id.into();
        if let Some(&idx) = self.index.get(&id) {
            return idx;
        }
        let label = label.into();
        let idx = self.graph.add_node(Node {
            id: id.clone(),
            kind,
            label,
        });
        self.index.insert(id, idx);
        idx
    }

    /// Add a directed edge `from -> to`. Endpoint nodes are created on demand
    /// (as [`NodeKind::Other`], labelled by id) if they do not yet exist.
    ///
    /// Parallel edges are allowed; callers that want to dedupe should check
    /// first. Returns `true` if both endpoints resolved and an edge was added.
    pub fn add_edge(
        &mut self,
        from: impl AsRef<str>,
        to: impl AsRef<str>,
        kind: EdgeKind,
        weight: f64,
    ) -> bool {
        let from = self.ensure_node(from.as_ref());
        let to = self.ensure_node(to.as_ref());
        self.graph.add_edge(from, to, Edge { kind, weight });
        true
    }

    /// Look up a node id, creating an `Other` node if absent.
    fn ensure_node(&mut self, id: &str) -> NodeIndex {
        if let Some(&idx) = self.index.get(id) {
            return idx;
        }
        self.add_node(id, NodeKind::Other, id)
    }

    /// Return the node metadata for an id, if present.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.index.get(id).and_then(|&idx| self.graph.node_weight(idx))
    }

    /// Iterate over all `(id, node)` pairs in the graph. Useful for callers that
    /// need to match nodes against external signals (e.g. a query's keywords)
    /// before expanding via [`neighbors`]. Order is unspecified.
    ///
    /// [`neighbors`]: KnowledgeGraph::neighbors
    pub fn nodes(&self) -> impl Iterator<Item = (&str, &Node)> {
        self.graph
            .node_indices()
            .map(move |idx| (self.graph[idx].id.as_str(), &self.graph[idx]))
    }

    /// Return the ids of all nodes adjacent to `id`, treating the graph as
    /// undirected (both outgoing and incoming neighbours). Deduplicated and
    /// sorted for deterministic output.
    pub fn neighbors(&self, id: &str) -> Vec<String> {
        let Some(&idx) = self.index.get(id) else {
            return Vec::new();
        };
        let mut out: Vec<String> = self
            .graph
            .neighbors_undirected(idx)
            .map(|n| self.graph[n].id.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Find a shortest path of node ids from `from` to `to`, treating the graph
    /// as undirected. Edge cost is `1.0 / weight` so heavier (stronger) edges
    /// are cheaper to traverse. Returns `None` if either endpoint is unknown or
    /// no path exists. The returned path includes both endpoints.
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let &start = self.index.get(from)?;
        let &goal = self.index.get(to)?;

        // astar with a zero heuristic == Dijkstra. We build an undirected
        // adjacency on the fly because the stored graph is directed but link
        // relationships are conceptually traversable in both directions.
        let path = petgraph::algo::astar(
            &self.graph,
            start,
            |n| n == goal,
            |e| 1.0 / e.weight().weight.max(f64::EPSILON),
            |_| 0.0,
        );

        // `astar` on a DiGraph only follows outgoing edges; fall back to an
        // undirected BFS if the directed search found nothing, so callers get
        // intuitive "are these connected?" behaviour.
        match path {
            Some((_, nodes)) => Some(nodes.into_iter().map(|n| self.graph[n].id.clone()).collect()),
            None => self.undirected_path(start, goal),
        }
    }

    /// Breadth-first shortest path over the undirected projection of the graph.
    fn undirected_path(&self, start: NodeIndex, goal: NodeIndex) -> Option<Vec<String>> {
        use std::collections::{HashSet, VecDeque};
        if start == goal {
            return Some(vec![self.graph[start].id.clone()]);
        }
        let mut prev: HashMap<NodeIndex, NodeIndex> = HashMap::new();
        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited.insert(start);

        while let Some(node) = queue.pop_front() {
            if node == goal {
                // Reconstruct.
                let mut path = vec![goal];
                let mut cur = goal;
                while let Some(&p) = prev.get(&cur) {
                    path.push(p);
                    cur = p;
                    if p == start {
                        break;
                    }
                }
                path.reverse();
                return Some(path.into_iter().map(|n| self.graph[n].id.clone()).collect());
            }
            for nbr in self.graph.neighbors_undirected(node) {
                if visited.insert(nbr) {
                    prev.insert(nbr, node);
                    queue.push_back(nbr);
                }
            }
        }
        None
    }

    /// Return the `top_n` highest-degree nodes as `(id, degree)` pairs, sorted
    /// by degree descending (ties broken by id ascending for determinism).
    /// Degree counts both incoming and outgoing edges (undirected degree).
    pub fn hub_nodes(&self, top_n: usize) -> Vec<(String, usize)> {
        let mut scored: Vec<(String, usize)> = self
            .graph
            .node_indices()
            .map(|idx| {
                let degree = self.graph.edges_directed(idx, Direction::Outgoing).count()
                    + self.graph.edges_directed(idx, Direction::Incoming).count();
                (self.graph[idx].id.clone(), degree)
            })
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        scored.truncate(top_n);
        scored
    }

    /// Partition the graph into communities. v1 uses connected components
    /// (over the undirected projection) via union-find: every group of nodes
    /// reachable from one another forms one community.
    ///
    /// Each community is a sorted `Vec<String>` of node ids; the outer vec is
    /// sorted by the smallest member id for deterministic output.
    pub fn communities(&self) -> Vec<Vec<String>> {
        let n = self.graph.node_count();
        if n == 0 {
            return Vec::new();
        }
        let mut uf = UnionFind::<usize>::new(n);
        for edge in self.graph.edge_references() {
            uf.union(edge.source().index(), edge.target().index());
        }
        let labels = uf.into_labeling();

        let mut groups: HashMap<usize, Vec<String>> = HashMap::new();
        for idx in self.graph.node_indices() {
            let label = labels[idx.index()];
            groups.entry(label).or_default().push(self.graph[idx].id.clone());
        }

        let mut communities: Vec<Vec<String>> = groups
            .into_values()
            .map(|mut g| {
                g.sort();
                g
            })
            .collect();
        // Sort communities by their first (smallest) member for stable output.
        communities.sort_by(|a, b| a.first().cmp(&b.first()));
        communities
    }

    /// Like [`shortest_path`], but returns the path as human-readable node
    /// *labels* rather than ids. Useful for surfacing an explanation of how
    /// two entities are connected.
    ///
    /// [`shortest_path`]: KnowledgeGraph::shortest_path
    pub fn explain_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let ids = self.shortest_path(from, to)?;
        Some(
            ids.into_iter()
                .map(|id| {
                    self.index
                        .get(&id)
                        .map(|&idx| self.graph[idx].label.clone())
                        .unwrap_or(id)
                })
                .collect(),
        )
    }

    /// Build a graph from a list of `(from, to)` link pairs. All nodes are
    /// auto-created as [`NodeKind::Note`] (labelled by id) and every edge gets
    /// the given `kind` with weight `1.0`. This is the primary entry point for
    /// adapters (e.g. the Obsidian wiki-link adapter) that have already reduced
    /// their domain to a flat list of directed links.
    pub fn from_link_pairs(pairs: &[(String, String)], kind: EdgeKind) -> Self {
        let mut graph = Self::new();
        for (from, to) in pairs {
            // Ensure both endpoints exist as Note nodes (not the default Other).
            graph.add_node(from.clone(), NodeKind::Note, from.clone());
            graph.add_node(to.clone(), NodeKind::Note, to.clone());
            graph.add_edge(from, to, kind, 1.0);
        }
        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a small graph with two disconnected clusters:
    ///   Cluster 1: a - b - c, plus a - c (so `a` is the hub, degree 2/3)
    ///   Cluster 2: x - y
    fn sample_graph() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        g.add_node("a", NodeKind::File, "Alpha");
        g.add_node("b", NodeKind::Symbol, "Beta");
        g.add_node("c", NodeKind::Note, "Gamma");
        g.add_edge("a", "b", EdgeKind::Link, 1.0);
        g.add_edge("b", "c", EdgeKind::Link, 1.0);
        g.add_edge("a", "c", EdgeKind::Reference, 1.0);

        // Disconnected second cluster.
        g.add_edge("x", "y", EdgeKind::CoOccurrence, 1.0);
        g
    }

    #[test]
    fn idempotent_node_insertion() {
        let mut g = KnowledgeGraph::new();
        let first = g.add_node("a", NodeKind::File, "Alpha");
        let second = g.add_node("a", NodeKind::Note, "Different");
        assert_eq!(first, second, "re-adding an id must reuse the node");
        assert_eq!(g.node_count(), 1);
        // First writer wins: metadata is not clobbered.
        let node = g.node("a").unwrap();
        assert_eq!(node.kind, NodeKind::File);
        assert_eq!(node.label, "Alpha");
    }

    #[test]
    fn neighbors_are_undirected_and_sorted() {
        let g = sample_graph();
        // a links to b and c.
        assert_eq!(g.neighbors("a"), vec!["b".to_string(), "c".to_string()]);
        // b is linked from a and links to c.
        assert_eq!(g.neighbors("b"), vec!["a".to_string(), "c".to_string()]);
        // Unknown node => empty.
        assert!(g.neighbors("zzz").is_empty());
    }

    #[test]
    fn shortest_path_direct_and_unknown() {
        let g = sample_graph();
        // a -> c is a direct reference edge.
        assert_eq!(
            g.shortest_path("a", "c"),
            Some(vec!["a".to_string(), "c".to_string()])
        );
        // a -> b is direct.
        assert_eq!(
            g.shortest_path("a", "b"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        // Across disconnected clusters => no path.
        assert_eq!(g.shortest_path("a", "x"), None);
        // Unknown endpoint => None.
        assert_eq!(g.shortest_path("a", "nope"), None);
    }

    #[test]
    fn shortest_path_undirected_fallback() {
        // c has only incoming edges; reaching b from c requires undirected travel.
        let g = sample_graph();
        let path = g.shortest_path("c", "b").expect("undirected path should exist");
        assert_eq!(path.first().unwrap(), "c");
        assert_eq!(path.last().unwrap(), "b");
    }

    #[test]
    fn hub_nodes_ordered_by_degree() {
        let g = sample_graph();
        let hubs = g.hub_nodes(3);
        // `a` has degree 2 (edges to b and c) -> highest.
        assert_eq!(hubs[0].0, "a");
        assert_eq!(hubs[0].1, 2);
        // b and c each have degree 2 as well (b: a-in, c-out; c: b-in, a-in).
        // Tie-break is id-ascending, so order after a is b, c.
        let ids: Vec<&str> = hubs.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn hub_nodes_respects_top_n() {
        let g = sample_graph();
        assert_eq!(g.hub_nodes(1).len(), 1);
        assert_eq!(g.hub_nodes(2).len(), 2);
        assert_eq!(g.hub_nodes(100).len(), g.node_count());
    }

    #[test]
    fn communities_split_disconnected_clusters() {
        let g = sample_graph();
        let communities = g.communities();
        assert_eq!(communities.len(), 2, "two disconnected clusters => two communities");
        // First community (smallest member "a") is {a,b,c}.
        assert_eq!(communities[0], vec!["a", "b", "c"]);
        // Second community is {x,y}.
        assert_eq!(communities[1], vec!["x", "y"]);
    }

    #[test]
    fn communities_empty_graph() {
        let g = KnowledgeGraph::new();
        assert!(g.communities().is_empty());
    }

    #[test]
    fn explain_path_returns_labels() {
        let g = sample_graph();
        let labels = g.explain_path("a", "b").expect("path exists");
        assert_eq!(labels, vec!["Alpha".to_string(), "Beta".to_string()]);
        // No path across clusters.
        assert_eq!(g.explain_path("a", "x"), None);
    }

    #[test]
    fn from_link_pairs_builds_structure() {
        let pairs = vec![
            ("note-a".to_string(), "note-b".to_string()),
            ("note-b".to_string(), "note-c".to_string()),
            ("note-a".to_string(), "note-c".to_string()),
        ];
        let g = KnowledgeGraph::from_link_pairs(&pairs, EdgeKind::Link);
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 3);
        // Auto-created nodes default to Note kind.
        assert_eq!(g.node("note-a").unwrap().kind, NodeKind::Note);
        // note-a is the hub (links to b and c).
        let hubs = g.hub_nodes(1);
        assert_eq!(hubs[0].0, "note-a");
        assert_eq!(hubs[0].1, 2);
        // Single connected community.
        assert_eq!(g.communities().len(), 1);
    }

    #[test]
    fn add_edge_auto_creates_endpoints() {
        let mut g = KnowledgeGraph::new();
        assert!(g.add_edge("p", "q", EdgeKind::Link, 1.0));
        assert_eq!(g.node_count(), 2);
        // Auto-created endpoints are Other kind.
        assert_eq!(g.node("p").unwrap().kind, NodeKind::Other);
    }
}
