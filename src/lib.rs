use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const EPSILON: f64 = 1e-9;

// ---------------------------------------------------------------------------
// 1. ProvenanceEntry
// ---------------------------------------------------------------------------

/// A single decision record in the provenance chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    pub id: String,
    pub timestamp: u64,
    pub agent_id: String,
    pub decision: String,
    pub alternatives: Vec<String>,
    pub reasoning: String,
    pub confidence: f64,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub conservation_before: f64,
    pub conservation_after: f64,
    pub room_id: String,
    pub tile_id: Option<String>,
}

static ENTRY_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl ProvenanceEntry {
    pub fn new(agent_id: &str, decision: &str) -> Self {
        let id = format!(
            "pe-{}",
            ENTRY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id,
            timestamp: now,
            agent_id: agent_id.to_string(),
            decision: decision.to_string(),
            alternatives: Vec::new(),
            reasoning: String::new(),
            confidence: 0.0,
            inputs: Vec::new(),
            outputs: Vec::new(),
            conservation_before: 0.0,
            conservation_after: 0.0,
            room_id: String::new(),
            tile_id: None,
        }
    }

    pub fn with_alternatives(&mut self, alts: Vec<String>) {
        self.alternatives = alts;
    }

    pub fn with_reasoning(&mut self, reasoning: &str) {
        self.reasoning = reasoning.to_string();
    }

    pub fn delta(&self) -> f64 {
        self.conservation_after - self.conservation_before
    }

    pub fn was_conservation_neutral(&self) -> bool {
        self.delta().abs() < EPSILON
    }

    pub fn was_good_decision(&self) -> bool {
        self.confidence > 0.7
    }
}

// ---------------------------------------------------------------------------
// 4. DecisionNode (defined before DecisionTree so tree can reference it)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionNode {
    pub decision: String,
    pub confidence: f64,
    pub children: Vec<DecisionNode>,
}

impl DecisionNode {
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    pub fn best_child(&self) -> Option<&DecisionNode> {
        self.children
            .iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
    }
}

// ---------------------------------------------------------------------------
// 3. DecisionTree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTree {
    pub root: DecisionNode,
}

impl DecisionTree {
    pub fn depth(&self) -> usize {
        Self::node_depth(&self.root)
    }

    fn node_depth(node: &DecisionNode) -> usize {
        if node.children.is_empty() {
            1
        } else {
            1 + node.children.iter().map(Self::node_depth).max().unwrap_or(0)
        }
    }

    pub fn node_count(&self) -> usize {
        Self::count_nodes(&self.root)
    }

    fn count_nodes(node: &DecisionNode) -> usize {
        1 + node.children.iter().map(Self::count_nodes).sum::<usize>()
    }

    pub fn find(&self, decision: &str) -> Option<&DecisionNode> {
        Self::find_in(&self.root, decision)
    }

    fn find_in<'a>(node: &'a DecisionNode, decision: &str) -> Option<&'a DecisionNode> {
        if node.decision == decision {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = Self::find_in(child, decision) {
                return Some(found);
            }
        }
        None
    }

    pub fn path_to(&self, decision: &str) -> Vec<&DecisionNode> {
        let mut path = Vec::new();
        Self::build_path(&self.root, decision, &mut path);
        path
    }

    fn build_path<'a>(
        node: &'a DecisionNode,
        decision: &str,
        path: &mut Vec<&'a DecisionNode>,
    ) -> bool {
        path.push(node);
        if node.decision == decision {
            return true;
        }
        for child in &node.children {
            if Self::build_path(child, decision, path) {
                return true;
            }
        }
        path.pop();
        false
    }
}

// ---------------------------------------------------------------------------
// 2. ProvenanceChain
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceChain {
    pub entries: Vec<ProvenanceEntry>,
    pub chain_id: String,
}

impl ProvenanceChain {
    pub fn new(chain_id: &str) -> Self {
        Self {
            entries: Vec::new(),
            chain_id: chain_id.to_string(),
        }
    }

    pub fn push(&mut self, entry: ProvenanceEntry) {
        self.entries.push(entry);
    }

    pub fn last(&self) -> Option<&ProvenanceEntry> {
        self.entries.last()
    }

    pub fn by_agent(&self, agent_id: &str) -> Vec<&ProvenanceEntry> {
        self.entries
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .collect()
    }

    pub fn by_room(&self, room_id: &str) -> Vec<&ProvenanceEntry> {
        self.entries
            .iter()
            .filter(|e| e.room_id == room_id)
            .collect()
    }

    pub fn by_time_range(&self, start: u64, end: u64) -> Vec<&ProvenanceEntry> {
        self.entries
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect()
    }

    pub fn total_conservation_delta(&self) -> f64 {
        self.entries.iter().map(|e| e.delta()).sum()
    }

    pub fn decision_tree(&self) -> DecisionTree {
        if self.entries.is_empty() {
            return DecisionTree {
                root: DecisionNode {
                    decision: String::new(),
                    confidence: 0.0,
                    children: Vec::new(),
                },
            };
        }

        let first = &self.entries[0];
        let mut root = DecisionNode {
            decision: first.decision.clone(),
            confidence: first.confidence,
            children: Vec::new(),
        };

        for entry in self.entries.iter().skip(1) {
            let new_node = DecisionNode {
                decision: entry.decision.clone(),
                confidence: entry.confidence,
                children: entry
                    .alternatives
                    .iter()
                    .map(|alt| DecisionNode {
                        decision: alt.clone(),
                        confidence: 0.0,
                        children: Vec::new(),
                    })
                    .collect(),
            };
            // Append as child of the last added node (linked-list style tree)
            Self::append_to_last(&mut root, new_node);
        }

        DecisionTree { root }
    }

    fn append_to_last(node: &mut DecisionNode, child: DecisionNode) {
        if node.children.is_empty() {
            node.children.push(child);
        } else {
            // Find the deepest single-child path
            let last = node.children.len() - 1;
            if node.children[last].children.is_empty() {
                node.children[last].children.push(child);
            } else {
                Self::append_to_last(&mut node.children[last], child);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. ProvenanceQuery
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ProvenanceQuery {
    pub agent_id: Option<String>,
    pub room_id: Option<String>,
    pub time_range: Option<(u64, u64)>,
    pub min_confidence: Option<f64>,
    pub decision_contains: Option<String>,
    pub limit: Option<usize>,
}

impl ProvenanceQuery {
    pub fn execute<'a>(&self, chain: &'a ProvenanceChain) -> Vec<&'a ProvenanceEntry> {
        let mut results: Vec<&ProvenanceEntry> = chain
            .entries
            .iter()
            .filter(|e| {
                if let Some(ref agent) = self.agent_id {
                    if e.agent_id != *agent {
                        return false;
                    }
                }
                if let Some(ref room) = self.room_id {
                    if e.room_id != *room {
                        return false;
                    }
                }
                if let Some((start, end)) = self.time_range {
                    if e.timestamp < start || e.timestamp > end {
                        return false;
                    }
                }
                if let Some(min_c) = self.min_confidence {
                    if e.confidence < min_c {
                        return false;
                    }
                }
                if let Some(ref needle) = self.decision_contains {
                    if !e.decision.contains(needle.as_str()) {
                        return false;
                    }
                }
                true
            })
            .collect();

        if let Some(limit) = self.limit {
            results.truncate(limit);
        }

        results
    }
}

// ---------------------------------------------------------------------------
// 7. ProvenanceAudit
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceAudit {
    pub total_entries: usize,
    pub total_chains: usize,
    pub conservation_total_delta: f64,
    pub agents_seen: Vec<String>,
    pub rooms_seen: Vec<String>,
    pub confidence_distribution: Vec<f64>,
    pub gaps: Vec<(u64, u64)>,
}

impl ProvenanceAudit {
    pub fn is_continuous(&self) -> bool {
        self.gaps.is_empty()
    }
}

// ---------------------------------------------------------------------------
// 9. ProvenanceError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ProvenanceError {
    ChainNotFound(String),
    EntryTooLarge(usize),
    ParseError(String),
    StorageFull,
}

impl std::fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChainNotFound(id) => write!(f, "chain not found: {}", id),
            Self::EntryTooLarge(size) => write!(f, "entry too large: {} bytes", size),
            Self::ParseError(msg) => write!(f, "parse error: {}", msg),
            Self::StorageFull => write!(f, "storage full"),
        }
    }
}

impl std::error::Error for ProvenanceError {}

// ---------------------------------------------------------------------------
// 6. ProvenanceStore
// ---------------------------------------------------------------------------

pub struct ProvenanceStore {
    pub chains: HashMap<String, ProvenanceChain>,
    pub max_entries_per_chain: usize,
}

impl ProvenanceStore {
    pub fn new(max_entries_per_chain: usize) -> Self {
        Self {
            chains: HashMap::new(),
            max_entries_per_chain,
        }
    }

    fn default_chain_key(entry: &ProvenanceEntry) -> String {
        format!("{}-{}", entry.agent_id, entry.room_id)
    }

    pub fn record(&mut self, entry: ProvenanceEntry) {
        let key = Self::default_chain_key(&entry);
        let chain = self
            .chains
            .entry(key)
            .or_insert_with(|| ProvenanceChain::new(&format!("chain-{}", entry.agent_id)));
        chain.push(entry);
        // Evict oldest if over limit
        while chain.entries.len() > self.max_entries_per_chain {
            chain.entries.remove(0);
        }
    }

    pub fn get_chain(&self, chain_id: &str) -> Option<&ProvenanceChain> {
        self.chains.get(chain_id)
    }

    pub fn query(&self, query: &ProvenanceQuery) -> Vec<&ProvenanceEntry> {
        let mut results = Vec::new();
        for chain in self.chains.values() {
            results.extend(query.execute(chain));
        }
        if let Some(limit) = query.limit {
            results.truncate(limit);
        }
        results
    }

    pub fn agent_history(&self, agent_id: &str) -> Vec<&ProvenanceEntry> {
        let q = ProvenanceQuery {
            agent_id: Some(agent_id.to_string()),
            ..Default::default()
        };
        self.query(&q)
    }

    pub fn room_history(&self, room_id: &str) -> Vec<&ProvenanceEntry> {
        let q = ProvenanceQuery {
            room_id: Some(room_id.to_string()),
            ..Default::default()
        };
        self.query(&q)
    }

    pub fn audit(&self) -> ProvenanceAudit {
        let mut total_entries = 0usize;
        let mut conservation_total_delta = 0.0f64;
        let mut agents = std::collections::HashSet::new();
        let mut rooms = std::collections::HashSet::new();
        let mut confidences = Vec::new();
        let mut all_timestamps: Vec<u64> = Vec::new();

        for chain in self.chains.values() {
            total_entries += chain.entries.len();
            conservation_total_delta += chain.total_conservation_delta();
            for e in &chain.entries {
                agents.insert(e.agent_id.clone());
                rooms.insert(e.room_id.clone());
                confidences.push(e.confidence);
                all_timestamps.push(e.timestamp);
            }
        }

        all_timestamps.sort();

        // Detect gaps > 3600 seconds (1 hour threshold)
        let gaps: Vec<(u64, u64)> = all_timestamps
            .windows(2)
            .filter(|w| w[1] - w[0] > 3600)
            .map(|w| (w[0], w[1]))
            .collect();

        ProvenanceAudit {
            total_entries,
            total_chains: self.chains.len(),
            conservation_total_delta,
            agents_seen: agents.into_iter().collect(),
            rooms_seen: rooms.into_iter().collect(),
            confidence_distribution: confidences,
            gaps,
        }
    }
}

// ---------------------------------------------------------------------------
// 8. ProvenanceExporter
// ---------------------------------------------------------------------------

pub struct ProvenanceExporter;

impl ProvenanceExporter {
    pub fn to_json(chain: &ProvenanceChain) -> String {
        serde_json::to_string_pretty(chain).unwrap_or_default()
    }

    pub fn to_csv(chain: &ProvenanceChain) -> String {
        let mut csv = String::from("id,timestamp,agent_id,decision,reasoning,confidence,conservation_before,conservation_after,room_id,tile_id\n");
        for e in &chain.entries {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                e.id,
                e.timestamp,
                e.agent_id,
                e.decision.replace(',', "\\,"),
                e.reasoning.replace(',', "\\,"),
                e.confidence,
                e.conservation_before,
                e.conservation_after,
                e.room_id,
                e.tile_id.as_deref().unwrap_or("")
            ));
        }
        csv
    }

    pub fn to_mermaid(chain: &ProvenanceChain) -> String {
        let mut out = String::from("graph TD\n");
        if chain.entries.is_empty() {
            return out;
        }
        for (i, e) in chain.entries.iter().enumerate() {
            let label = &e.decision;
            out.push_str(&format!("    N{}[\"{}\"]\n", i, label.replace('"', "\\\"")));
            if i > 0 {
                out.push_str(&format!("    N{} --> N{}\n", i - 1, i));
            }
        }
        out
    }

    pub fn from_json(json: &str) -> Result<ProvenanceChain, ProvenanceError> {
        serde_json::from_str(json).map_err(|e| ProvenanceError::ParseError(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(agent: &str, decision: &str, ts: u64, conf: f64, cb: f64, ca: f64) -> ProvenanceEntry {
        let mut e = ProvenanceEntry::new(agent, decision);
        e.timestamp = ts;
        e.confidence = conf;
        e.conservation_before = cb;
        e.conservation_after = ca;
        e.room_id = "room-1".to_string();
        e
    }

    // --- ProvenanceEntry tests ---

    #[test]
    fn entry_new_has_id() {
        let e = ProvenanceEntry::new("agent-a", "decide something");
        assert!(!e.id.is_empty());
        assert!(e.id.starts_with("pe-"));
        assert_eq!(e.agent_id, "agent-a");
        assert_eq!(e.decision, "decide something");
    }

    #[test]
    fn entry_with_alternatives() {
        let mut e = ProvenanceEntry::new("a", "d");
        e.with_alternatives(vec!["alt1".into(), "alt2".into()]);
        assert_eq!(e.alternatives.len(), 2);
        assert_eq!(e.alternatives[0], "alt1");
    }

    #[test]
    fn entry_with_reasoning() {
        let mut e = ProvenanceEntry::new("a", "d");
        e.with_reasoning("because reasons");
        assert_eq!(e.reasoning, "because reasons");
    }

    #[test]
    fn entry_delta() {
        let e = make_entry("a", "d", 100, 0.9, 10.0, 12.0);
        assert!((e.delta() - 2.0).abs() < EPSILON);
    }

    #[test]
    fn entry_conservation_neutral_true() {
        let e = make_entry("a", "d", 100, 0.9, 10.0, 10.0);
        assert!(e.was_conservation_neutral());
    }

    #[test]
    fn entry_conservation_neutral_false() {
        let e = make_entry("a", "d", 100, 0.9, 10.0, 11.0);
        assert!(!e.was_conservation_neutral());
    }

    #[test]
    fn entry_good_decision_true() {
        let e = make_entry("a", "d", 100, 0.8, 10.0, 10.0);
        assert!(e.was_good_decision());
    }

    #[test]
    fn entry_good_decision_false() {
        let e = make_entry("a", "d", 100, 0.5, 10.0, 10.0);
        assert!(!e.was_good_decision());
    }

    #[test]
    fn entry_good_decision_boundary() {
        let e = make_entry("a", "d", 100, 0.7, 10.0, 10.0);
        assert!(!e.was_good_decision()); // strictly > 0.7
    }

    #[test]
    fn entry_tile_id_optional() {
        let mut e = ProvenanceEntry::new("a", "d");
        assert!(e.tile_id.is_none());
        e.tile_id = Some("tile-42".into());
        assert_eq!(e.tile_id.as_deref(), Some("tile-42"));
    }

    #[test]
    fn entry_inputs_outputs() {
        let mut e = ProvenanceEntry::new("a", "d");
        e.inputs = vec!["sensor-1".into()];
        e.outputs = vec!["action-move".into(), "action-turn".into()];
        assert_eq!(e.inputs.len(), 1);
        assert_eq!(e.outputs.len(), 2);
    }

    // --- ProvenanceChain tests ---

    #[test]
    fn chain_new() {
        let c = ProvenanceChain::new("chain-1");
        assert_eq!(c.chain_id, "chain-1");
        assert!(c.entries.is_empty());
    }

    #[test]
    fn chain_push_and_last() {
        let mut c = ProvenanceChain::new("c1");
        let e1 = make_entry("a", "d1", 100, 0.9, 10.0, 10.0);
        c.push(e1);
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.last().unwrap().decision, "d1");
    }

    #[test]
    fn chain_last_empty() {
        let c = ProvenanceChain::new("c1");
        assert!(c.last().is_none());
    }

    #[test]
    fn chain_by_agent() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("agent-a", "d1", 100, 0.9, 0.0, 0.0));
        c.push(make_entry("agent-b", "d2", 101, 0.9, 0.0, 0.0));
        c.push(make_entry("agent-a", "d3", 102, 0.9, 0.0, 0.0));
        let a_entries = c.by_agent("agent-a");
        assert_eq!(a_entries.len(), 2);
        assert!(a_entries.iter().all(|e| e.agent_id == "agent-a"));
    }

    #[test]
    fn chain_by_room() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        let mut e2 = make_entry("a", "d2", 101, 0.9, 0.0, 0.0);
        e2.room_id = "room-2".to_string();
        c.push(e2);
        assert_eq!(c.by_room("room-1").len(), 1);
        assert_eq!(c.by_room("room-2").len(), 1);
    }

    #[test]
    fn chain_by_time_range() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        c.push(make_entry("a", "d2", 200, 0.9, 0.0, 0.0));
        c.push(make_entry("a", "d3", 300, 0.9, 0.0, 0.0));
        let r = c.by_time_range(150, 250);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].decision, "d2");
    }

    #[test]
    fn chain_total_conservation_delta() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 10.0, 12.0)); // +2
        c.push(make_entry("a", "d2", 101, 0.9, 12.0, 10.0)); // -2
        assert!(c.total_conservation_delta().abs() < EPSILON);
    }

    #[test]
    fn chain_append_only() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        let ts = c.entries[0].timestamp;
        c.push(make_entry("a", "d2", 200, 0.9, 0.0, 0.0));
        // First entry unchanged
        assert_eq!(c.entries[0].timestamp, ts);
        assert_eq!(c.entries[0].decision, "d1");
    }

    // --- DecisionTree / DecisionNode tests ---

    #[test]
    fn decision_node_is_leaf() {
        let node = DecisionNode {
            decision: "d".into(),
            confidence: 0.5,
            children: vec![],
        };
        assert!(node.is_leaf());
    }

    #[test]
    fn decision_node_not_leaf() {
        let node = DecisionNode {
            decision: "d".into(),
            confidence: 0.5,
            children: vec![DecisionNode {
                decision: "child".into(),
                confidence: 0.6,
                children: vec![],
            }],
        };
        assert!(!node.is_leaf());
    }

    #[test]
    fn decision_node_best_child() {
        let node = DecisionNode {
            decision: "d".into(),
            confidence: 0.5,
            children: vec![
                DecisionNode { decision: "c1".into(), confidence: 0.3, children: vec![] },
                DecisionNode { decision: "c2".into(), confidence: 0.9, children: vec![] },
                DecisionNode { decision: "c3".into(), confidence: 0.6, children: vec![] },
            ],
        };
        let best = node.best_child().unwrap();
        assert_eq!(best.decision, "c2");
    }

    #[test]
    fn decision_node_best_child_none() {
        let node = DecisionNode {
            decision: "d".into(),
            confidence: 0.5,
            children: vec![],
        };
        assert!(node.best_child().is_none());
    }

    #[test]
    fn tree_depth_single() {
        let tree = DecisionTree {
            root: DecisionNode { decision: "root".into(), confidence: 0.5, children: vec![] },
        };
        assert_eq!(tree.depth(), 1);
    }

    #[test]
    fn tree_depth_nested() {
        let tree = DecisionTree {
            root: DecisionNode {
                decision: "root".into(),
                confidence: 0.5,
                children: vec![DecisionNode {
                    decision: "child".into(),
                    confidence: 0.6,
                    children: vec![DecisionNode {
                        decision: "grandchild".into(),
                        confidence: 0.7,
                        children: vec![],
                    }],
                }],
            },
        };
        assert_eq!(tree.depth(), 3);
    }

    #[test]
    fn tree_node_count() {
        let tree = DecisionTree {
            root: DecisionNode {
                decision: "root".into(),
                confidence: 0.5,
                children: vec![
                    DecisionNode { decision: "c1".into(), confidence: 0.3, children: vec![] },
                    DecisionNode { decision: "c2".into(), confidence: 0.4, children: vec![] },
                ],
            },
        };
        assert_eq!(tree.node_count(), 3);
    }

    #[test]
    fn tree_find_exists() {
        let tree = DecisionTree {
            root: DecisionNode {
                decision: "root".into(),
                confidence: 0.5,
                children: vec![DecisionNode {
                    decision: "target".into(),
                    confidence: 0.8,
                    children: vec![],
                }],
            },
        };
        assert!(tree.find("target").is_some());
        assert_eq!(tree.find("target").unwrap().confidence, 0.8);
    }

    #[test]
    fn tree_find_missing() {
        let tree = DecisionTree {
            root: DecisionNode { decision: "root".into(), confidence: 0.5, children: vec![] },
        };
        assert!(tree.find("nonexistent").is_none());
    }

    #[test]
    fn tree_path_to() {
        let tree = DecisionTree {
            root: DecisionNode {
                decision: "A".into(),
                confidence: 0.5,
                children: vec![DecisionNode {
                    decision: "B".into(),
                    confidence: 0.6,
                    children: vec![DecisionNode {
                        decision: "C".into(),
                        confidence: 0.7,
                        children: vec![],
                    }],
                }],
            },
        };
        let path = tree.path_to("C");
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].decision, "A");
        assert_eq!(path[1].decision, "B");
        assert_eq!(path[2].decision, "C");
    }

    #[test]
    fn tree_path_to_missing() {
        let tree = DecisionTree {
            root: DecisionNode { decision: "root".into(), confidence: 0.5, children: vec![] },
        };
        let path = tree.path_to("missing");
        assert!(path.is_empty());
    }

    #[test]
    fn chain_decision_tree_from_entries() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "decide-A", 100, 0.8, 0.0, 0.0));
        c.push(make_entry("a", "decide-B", 200, 0.9, 0.0, 0.0));
        let tree = c.decision_tree();
        assert_eq!(tree.root.decision, "decide-A");
        assert!(!tree.root.children.is_empty());
    }

    // --- ProvenanceQuery tests ---

    #[test]
    fn query_by_agent() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        c.push(make_entry("b", "d2", 101, 0.9, 0.0, 0.0));
        let q = ProvenanceQuery {
            agent_id: Some("a".into()),
            ..Default::default()
        };
        let r = q.execute(&c);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].agent_id, "a");
    }

    #[test]
    fn query_by_min_confidence() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.5, 0.0, 0.0));
        c.push(make_entry("a", "d2", 101, 0.9, 0.0, 0.0));
        let q = ProvenanceQuery {
            min_confidence: Some(0.8),
            ..Default::default()
        };
        let r = q.execute(&c);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].decision, "d2");
    }

    #[test]
    fn query_by_decision_contains() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "move north", 100, 0.9, 0.0, 0.0));
        c.push(make_entry("a", "turn left", 101, 0.9, 0.0, 0.0));
        let q = ProvenanceQuery {
            decision_contains: Some("move".into()),
            ..Default::default()
        };
        let r = q.execute(&c);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].decision, "move north");
    }

    #[test]
    fn query_with_limit() {
        let mut c = ProvenanceChain::new("c1");
        for i in 0..10 {
            c.push(make_entry("a", &format!("d{}", i), 100 + i, 0.9, 0.0, 0.0));
        }
        let q = ProvenanceQuery {
            limit: Some(3),
            ..Default::default()
        };
        assert_eq!(q.execute(&c).len(), 3);
    }

    #[test]
    fn query_combined_filters() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "move", 100, 0.9, 0.0, 0.0));
        c.push(make_entry("b", "move", 101, 0.5, 0.0, 0.0));
        c.push(make_entry("a", "stay", 102, 0.9, 0.0, 0.0));
        let q = ProvenanceQuery {
            agent_id: Some("a".into()),
            min_confidence: Some(0.8),
            decision_contains: Some("move".into()),
            ..Default::default()
        };
        let r = q.execute(&c);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].agent_id, "a");
    }

    #[test]
    fn query_no_match() {
        let c = ProvenanceChain::new("c1");
        let q = ProvenanceQuery {
            agent_id: Some("nobody".into()),
            ..Default::default()
        };
        assert!(q.execute(&c).is_empty());
    }

    #[test]
    fn query_by_time_range() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        c.push(make_entry("a", "d2", 200, 0.9, 0.0, 0.0));
        let q = ProvenanceQuery {
            time_range: Some((50, 150)),
            ..Default::default()
        };
        let r = q.execute(&c);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].decision, "d1");
    }

    // --- ProvenanceStore tests ---

    #[test]
    fn store_record_and_get() {
        let mut store = ProvenanceStore::new(100);
        let e = make_entry("a", "d1", 100, 0.9, 0.0, 0.0);
        store.record(e);
        // The chain key is "a-room-1"
        let chain = store.get_chain("a-room-1");
        assert!(chain.is_some());
        assert_eq!(chain.unwrap().entries.len(), 1);
    }

    #[test]
    fn store_get_missing_chain() {
        let store = ProvenanceStore::new(100);
        assert!(store.get_chain("nonexistent").is_none());
    }

    #[test]
    fn store_eviction() {
        let mut store = ProvenanceStore::new(3);
        for i in 0..5 {
            store.record(make_entry("a", &format!("d{}", i), 100 + i, 0.9, 0.0, 0.0));
        }
        let chain = store.get_chain("a-room-1").unwrap();
        assert_eq!(chain.entries.len(), 3);
        // Oldest evicted: d0, d1 gone
        assert_eq!(chain.entries[0].decision, "d2");
        assert_eq!(chain.entries[2].decision, "d4");
    }

    #[test]
    fn store_query_across_chains() {
        let mut store = ProvenanceStore::new(100);
        let mut e1 = make_entry("a", "d1", 100, 0.9, 0.0, 0.0);
        e1.room_id = "room-A".into();
        let mut e2 = make_entry("a", "d2", 101, 0.9, 0.0, 0.0);
        e2.room_id = "room-B".into();
        store.record(e1);
        store.record(e2);
        let q = ProvenanceQuery {
            agent_id: Some("a".into()),
            ..Default::default()
        };
        assert_eq!(store.query(&q).len(), 2);
    }

    #[test]
    fn store_agent_history() {
        let mut store = ProvenanceStore::new(100);
        store.record(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        store.record(make_entry("b", "d2", 101, 0.9, 0.0, 0.0));
        store.record(make_entry("a", "d3", 102, 0.9, 0.0, 0.0));
        let hist = store.agent_history("a");
        assert_eq!(hist.len(), 2);
    }

    #[test]
    fn store_room_history() {
        let mut store = ProvenanceStore::new(100);
        store.record(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        let mut e2 = make_entry("a", "d2", 101, 0.9, 0.0, 0.0);
        e2.room_id = "room-2".into();
        store.record(e2);
        let hist = store.room_history("room-1");
        assert_eq!(hist.len(), 1);
    }

    // --- Audit tests ---

    #[test]
    fn audit_empty_store() {
        let store = ProvenanceStore::new(100);
        let audit = store.audit();
        assert_eq!(audit.total_entries, 0);
        assert_eq!(audit.total_chains, 0);
        assert!(audit.conservation_total_delta.abs() < EPSILON);
        assert!(audit.agents_seen.is_empty());
        assert!(audit.rooms_seen.is_empty());
        assert!(audit.confidence_distribution.is_empty());
        assert!(audit.gaps.is_empty());
        assert!(audit.is_continuous());
    }

    #[test]
    fn audit_counts_entries_and_chains() {
        let mut store = ProvenanceStore::new(100);
        store.record(make_entry("a", "d1", 100, 0.9, 10.0, 12.0));
        let mut e2 = make_entry("b", "d2", 101, 0.8, 0.0, 0.0);
        e2.room_id = "room-2".into();
        store.record(e2);
        let audit = store.audit();
        assert_eq!(audit.total_entries, 2);
        assert_eq!(audit.total_chains, 2);
    }

    #[test]
    fn audit_agents_and_rooms() {
        let mut store = ProvenanceStore::new(100);
        store.record(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        store.record(make_entry("b", "d2", 101, 0.9, 0.0, 0.0));
        let audit = store.audit();
        assert_eq!(audit.agents_seen.len(), 2);
        assert!(audit.agents_seen.contains(&"a".to_string()));
        assert!(audit.agents_seen.contains(&"b".to_string()));
    }

    #[test]
    fn audit_confidence_distribution() {
        let mut store = ProvenanceStore::new(100);
        store.record(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        store.record(make_entry("a", "d2", 101, 0.5, 0.0, 0.0));
        let audit = store.audit();
        assert_eq!(audit.confidence_distribution.len(), 2);
        assert!(audit.confidence_distribution.contains(&0.9));
        assert!(audit.confidence_distribution.contains(&0.5));
    }

    #[test]
    fn audit_detects_gaps() {
        let mut store = ProvenanceStore::new(100);
        store.record(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        store.record(make_entry("a", "d2", 5000, 0.9, 0.0, 0.0));
        let audit = store.audit();
        assert!(!audit.gaps.is_empty());
        assert!(!audit.is_continuous());
    }

    #[test]
    fn audit_no_gaps_when_continuous() {
        let mut store = ProvenanceStore::new(100);
        store.record(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        store.record(make_entry("a", "d2", 200, 0.9, 0.0, 0.0));
        let audit = store.audit();
        assert!(audit.gaps.is_empty());
        assert!(audit.is_continuous());
    }

    // --- Conservation theorem ---

    #[test]
    fn conservation_delta_sums_near_zero_over_long_chain() {
        let mut chain = ProvenanceChain::new("c1");
        // Simulate pairs: +X then -X
        for i in 0..50 {
            let gain = (i as f64) * 0.1;
            chain.push(make_entry("a", "gain", 100 + i * 2, 0.9, 100.0, 100.0 + gain));
            chain.push(make_entry("a", "loss", 101 + i * 2, 0.9, 100.0 + gain, 100.0));
        }
        assert!(chain.total_conservation_delta().abs() < EPSILON);
    }

    #[test]
    fn decision_tree_depth_bounded_by_entries() {
        let mut chain = ProvenanceChain::new("c1");
        for i in 0..5 {
            chain.push(make_entry("a", &format!("d{}", i), 100 + i, 0.8, 0.0, 0.0));
        }
        let tree = chain.decision_tree();
        assert!(tree.depth() <= chain.entries.len());
    }

    #[test]
    fn query_by_agent_returns_only_that_agent() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("alice", "d1", 100, 0.9, 0.0, 0.0));
        c.push(make_entry("bob", "d2", 101, 0.9, 0.0, 0.0));
        c.push(make_entry("alice", "d3", 102, 0.9, 0.0, 0.0));
        let r = c.by_agent("alice");
        assert!(r.iter().all(|e| e.agent_id == "alice"));
    }

    #[test]
    fn query_by_time_range_correctness() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "early", 50, 0.9, 0.0, 0.0));
        c.push(make_entry("a", "mid", 150, 0.9, 0.0, 0.0));
        c.push(make_entry("a", "late", 250, 0.9, 0.0, 0.0));
        let r = c.by_time_range(100, 200);
        assert!(r.iter().all(|e| e.timestamp >= 100 && e.timestamp <= 200));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn provenance_chain_is_append_only() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "first", 100, 0.9, 5.0, 5.0));
        let orig_id = c.entries[0].id.clone();
        c.push(make_entry("a", "second", 200, 0.8, 5.0, 5.0));
        assert_eq!(c.entries[0].id, orig_id);
        assert_eq!(c.entries[0].decision, "first");
    }

    // --- Exporter tests ---

    #[test]
    fn json_roundtrip() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 10.0, 12.0));
        c.push(make_entry("a", "d2", 200, 0.8, 12.0, 14.0));
        let json = ProvenanceExporter::to_json(&c);
        let restored = ProvenanceExporter::from_json(&json).unwrap();
        assert_eq!(restored.chain_id, "c1");
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].decision, "d1");
        assert_eq!(restored.entries[1].confidence, 0.8);
    }

    #[test]
    fn json_roundtrip_full_fidelity() {
        let mut c = ProvenanceChain::new("c-full");
        let mut e = make_entry("a", "complex decision", 999, 0.75, 42.0, 43.5);
        e.with_alternatives(vec!["alt-A".into(), "alt-B".into()]);
        e.with_reasoning("it made sense at the time");
        e.inputs = vec!["sensor-1".into()];
        e.outputs = vec!["move".into()];
        e.tile_id = Some("tile-7".into());
        c.push(e);
        let json = ProvenanceExporter::to_json(&c);
        let restored = ProvenanceExporter::from_json(&json).unwrap();
        let re = &restored.entries[0];
        assert_eq!(re.alternatives.len(), 2);
        assert_eq!(re.reasoning, "it made sense at the time");
        assert_eq!(re.tile_id.as_deref(), Some("tile-7"));
        assert!((re.conservation_before - 42.0).abs() < EPSILON);
    }

    #[test]
    fn csv_export_columns() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        let csv = ProvenanceExporter::to_csv(&c);
        let header_line = csv.lines().next().unwrap();
        let col_count = header_line.split(',').count();
        assert_eq!(col_count, 10); // 10 columns
        // Data row should also have 10 columns
        let data_line = csv.lines().nth(1).unwrap();
        assert_eq!(data_line.split(',').count(), 10);
    }

    #[test]
    fn csv_export_multiple_entries() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        c.push(make_entry("b", "d2", 200, 0.8, 0.0, 0.0));
        let csv = ProvenanceExporter::to_csv(&c);
        assert_eq!(csv.lines().count(), 3); // header + 2 data rows
    }

    #[test]
    fn mermaid_starts_with_graph() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        let mermaid = ProvenanceExporter::to_mermaid(&c);
        assert!(mermaid.starts_with("graph"));
    }

    #[test]
    fn mermaid_has_nodes() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "decide", 100, 0.9, 0.0, 0.0));
        c.push(make_entry("a", "act", 200, 0.8, 0.0, 0.0));
        let mermaid = ProvenanceExporter::to_mermaid(&c);
        assert!(mermaid.contains("N0"));
        assert!(mermaid.contains("N1"));
        assert!(mermaid.contains("-->"));
    }

    #[test]
    fn mermaid_empty_chain() {
        let c = ProvenanceChain::new("c1");
        let mermaid = ProvenanceExporter::to_mermaid(&c);
        assert!(mermaid.starts_with("graph"));
    }

    // --- Error tests ---

    #[test]
    fn error_chain_not_found_display() {
        let err = ProvenanceError::ChainNotFound("x".into());
        assert!(err.to_string().contains("x"));
    }

    #[test]
    fn error_parse_error_from_bad_json() {
        let result = ProvenanceExporter::from_json("not json at all");
        assert!(matches!(result, Err(ProvenanceError::ParseError(_))));
    }

    #[test]
    fn error_variants() {
        let _ = ProvenanceError::EntryTooLarge(999);
        let _ = ProvenanceError::StorageFull;
    }

    // --- Additional coverage ---

    #[test]
    fn store_max_entries_eviction_oldest_first() {
        let mut store = ProvenanceStore::new(2);
        store.record(make_entry("a", "first", 100, 0.9, 0.0, 0.0));
        store.record(make_entry("a", "second", 200, 0.9, 0.0, 0.0));
        store.record(make_entry("a", "third", 300, 0.9, 0.0, 0.0));
        let chain = store.get_chain("a-room-1").unwrap();
        assert_eq!(chain.entries.len(), 2);
        assert_eq!(chain.entries[0].decision, "second");
        assert_eq!(chain.entries[1].decision, "third");
    }

    #[test]
    fn decision_tree_best_child_returns_highest_confidence() {
        let node = DecisionNode {
            decision: "root".into(),
            confidence: 0.5,
            children: vec![
                DecisionNode { decision: "low".into(), confidence: 0.2, children: vec![] },
                DecisionNode { decision: "high".into(), confidence: 0.95, children: vec![] },
                DecisionNode { decision: "mid".into(), confidence: 0.6, children: vec![] },
            ],
        };
        let best = node.best_child().unwrap();
        assert_eq!(best.decision, "high");
        assert!((best.confidence - 0.95).abs() < EPSILON);
    }

    #[test]
    fn empty_store_audit_zero_counts() {
        let store = ProvenanceStore::new(100);
        let audit = store.audit();
        assert_eq!(audit.total_entries, 0);
        assert_eq!(audit.total_chains, 0);
    }

    #[test]
    fn entry_timestamp_auto_set() {
        let e = ProvenanceEntry::new("a", "d");
        assert!(e.timestamp > 0);
    }

    #[test]
    fn chain_decision_tree_empty() {
        let c = ProvenanceChain::new("c1");
        let tree = c.decision_tree();
        assert!(tree.root.decision.is_empty());
    }

    #[test]
    fn serde_entry_roundtrip() {
        let mut e = ProvenanceEntry::new("a", "test");
        e.confidence = 0.85;
        e.conservation_before = 10.0;
        e.conservation_after = 12.0;
        e.room_id = "room-1".into();
        e.tile_id = Some("t-1".into());
        e.with_alternatives(vec!["alt1".into()]);
        e.with_reasoning("because");

        let json = serde_json::to_string(&e).unwrap();
        let restored: ProvenanceEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.agent_id, "a");
        assert_eq!(restored.confidence, 0.85);
        assert_eq!(restored.alternatives.len(), 1);
        assert_eq!(restored.reasoning, "because");
        assert_eq!(restored.tile_id.as_deref(), Some("t-1"));
    }

    #[test]
    fn query_empty_chain() {
        let c = ProvenanceChain::new("c1");
        let q = ProvenanceQuery::default();
        assert!(q.execute(&c).is_empty());
    }

    #[test]
    fn query_by_room_filter() {
        let mut c = ProvenanceChain::new("c1");
        c.push(make_entry("a", "d1", 100, 0.9, 0.0, 0.0));
        let mut e2 = make_entry("a", "d2", 101, 0.9, 0.0, 0.0);
        e2.room_id = "room-99".into();
        c.push(e2);
        let q = ProvenanceQuery {
            room_id: Some("room-1".into()),
            ..Default::default()
        };
        let r = q.execute(&c);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].room_id, "room-1");
    }

    #[test]
    fn conservation_audit_delta() {
        let mut store = ProvenanceStore::new(100);
        store.record(make_entry("a", "d1", 100, 0.9, 10.0, 15.0));
        store.record(make_entry("a", "d2", 101, 0.9, 15.0, 10.0));
        let audit = store.audit();
        assert!(audit.conservation_total_delta.abs() < EPSILON);
    }
}
