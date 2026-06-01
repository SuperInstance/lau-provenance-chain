# lau-provenance-chain

**Full provenance chains** — every decision recorded, every alternative considered, every reason why. A complete audit trail for agent decision-making with conservation tracking, decision tree extraction, querying, and multi-format export.

## What This Does

When autonomous agents make decisions in a PLATO system, you need to know:

1. **What was decided?** — The chosen action, with timestamp and agent ID
2. **What else was considered?** — Alternatives that were rejected
3. **Why?** — Freeform reasoning attached to each decision
4. **What changed?** — Conservation values before and after (was this decision neutral?)
5. **Can I find it later?** — Query by agent, room, time range, confidence, or decision text
6. **Can I see the whole story?** — Export as JSON, CSV, or Mermaid flowchart; extract decision trees

This library provides all of that with an append-only, serde-serialisable data model.

## Key Idea

Every decision becomes a `ProvenanceEntry`. Entries accumulate in a `ProvenanceChain` (append-only log). Multiple chains live in a `ProvenanceStore` (keyed by agent+room). You can query, audit, and export at any level. The chain can be transformed into a `DecisionTree` for structural analysis.

```
ProvenanceEntry → ProvenanceChain → ProvenanceStore
                     ↓                    ↓
               DecisionTree          ProvenanceQuery → filtered results
                                         ↓
                                    ProvenanceAudit (gap detection, stats)
                                         ↓
                                    ProvenanceExporter (JSON / CSV / Mermaid)
```

## Install

```toml
[dependencies]
lau-provenance-chain = "0.1"
```

Or:

```bash
cargo add lau-provenance-chain
```

Requires **Rust 2021 edition**.

## Quick Start

```rust
use lau_provenance_chain::{
    ProvenanceEntry, ProvenanceStore, ProvenanceExporter, ProvenanceQuery,
};

fn main() {
    let mut store = ProvenanceStore::new(1000);

    // Record a decision
    let mut entry = ProvenanceEntry::new("agent-1", "move to tile B3");
    entry.confidence = 0.92;
    entry.room_id = "room-alpha".into();
    entry.conservation_before = 100.0;
    entry.conservation_after = 100.0; // neutral
    entry.with_alternatives(vec!["stay put".into(), "move to C4".into()]);
    entry.with_reasoning("B3 has the highest expected utility");
    entry.inputs = vec!["sensor-north".into()];
    entry.outputs = vec!["action-move".into()];
    entry.tile_id = Some("B3".into());

    store.record(entry);

    // Record another
    let mut e2 = ProvenanceEntry::new("agent-1", "rotate 90°");
    e2.confidence = 0.78;
    e2.room_id = "room-alpha".into();
    e2.conservation_before = 100.0;
    e2.conservation_after = 99.5;
    e2.with_reasoning("Needed to face the exit");
    store.record(e2);

    // Query
    let decisions = store.agent_history("agent-1");
    println!("Agent-1 made {} decisions", decisions.len());

    // Export
    let chain = store.get_chain("agent-1-room-alpha").unwrap();
    println!("{}", ProvenanceExporter::to_mermaid(chain));
}
```

## API Reference

### `ProvenanceEntry`

A single decision record:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Auto-generated (`pe-N`) |
| `timestamp` | `u64` | Unix epoch seconds (auto-set) |
| `agent_id` | `String` | Who decided |
| `decision` | `String` | What was decided |
| `alternatives` | `Vec<String>` | Rejected options |
| `reasoning` | `String` | Why |
| `confidence` | `f64` | 0.0–1.0 |
| `inputs` / `outputs` | `Vec<String>` | Data in, actions out |
| `conservation_before` / `after` | `f64` | Conservation values |
| `room_id` | `String` | Context room |
| `tile_id` | `Option<String>` | Optional tile reference |

Methods:
- `new(agent_id, decision)` — create with auto-ID and timestamp
- `with_alternatives(alts)` — set alternatives
- `with_reasoning(text)` — set reasoning
- `delta()` — `conservation_after - conservation_before`
- `was_conservation_neutral()` — |delta| < ε
- `was_good_decision()` — confidence > 0.7

### `ProvenanceChain`

An append-only log of entries:

| Method | Description |
|--------|-------------|
| `new(chain_id)` | Create empty chain |
| `push(entry)` | Append an entry |
| `last()` | Most recent entry |
| `by_agent(agent_id)` | Filter by agent |
| `by_room(room_id)` | Filter by room |
| `by_time_range(start, end)` | Filter by timestamp |
| `total_conservation_delta()` | Sum of all deltas |
| `decision_tree()` | Extract as a `DecisionTree` |

### `ProvenanceStore`

Multi-chain storage with eviction:

| Method | Description |
|--------|-------------|
| `new(max_entries_per_chain)` | Create with capacity |
| `record(entry)` | Insert (auto-keys by agent+room) |
| `get_chain(key)` | Retrieve a specific chain |
| `query(query)` | Search across all chains |
| `agent_history(agent_id)` | All entries for an agent |
| `room_history(room_id)` | All entries for a room |
| `audit()` | Full `ProvenanceAudit` |

When a chain exceeds `max_entries_per_chain`, the **oldest entries are evicted** (FIFO).

### `ProvenanceQuery`

Composable filter builder:

```rust
ProvenanceQuery {
    agent_id: Some("agent-1".into()),
    room_id: None,
    time_range: Some((1000, 2000)),
    min_confidence: Some(0.8),
    decision_contains: Some("move".into()),
    limit: Some(10),
}
```

All fields are optional; `Default::default()` matches everything.

### `DecisionTree` and `DecisionNode`

Structural view of the decision chain:

| Method | Description |
|--------|-------------|
| `tree.depth()` | Max depth |
| `tree.node_count()` | Total nodes |
| `tree.find(decision)` | Find a node by decision text |
| `tree.path_to(decision)` | Path from root to that node |
| `node.best_child()` | Highest-confidence child |
| `node.is_leaf()` | No children |

### `ProvenanceAudit`

Snapshot of store health:

| Field | Description |
|-------|-------------|
| `total_entries` | Across all chains |
| `total_chains` | Number of chains |
| `conservation_total_delta` | Aggregate conservation change |
| `agents_seen` / `rooms_seen` | Unique agents and rooms |
| `confidence_distribution` | All confidence values |
| `gaps` | Timestamp gaps > 3600s |

`is_continuous()` returns `true` if no gaps detected.

### `ProvenanceExporter`

| Method | Output |
|--------|--------|
| `to_json(chain)` | Pretty-printed JSON |
| `to_csv(chain)` | 10-column CSV (header + data) |
| `to_mermaid(chain)` | Mermaid `graph TD` flowchart |
| `from_json(json)` | Parse back into `ProvenanceChain` |

### `ProvenanceError`

```rust
pub enum ProvenanceError {
    ChainNotFound(String),
    EntryTooLarge(usize),
    ParseError(String),
    StorageFull,
}
```

Implements `std::error::Error` and `Display`.

## How It Works

### Append-Only Semantics

`ProvenanceChain` is strictly append-only: once an entry is pushed, it is never modified. This guarantees that the provenance trail is tamper-evident — you can always reconstruct the exact sequence of decisions.

### Store Keying

`ProvenanceStore` automatically keys chains as `{agent_id}-{room_id}`. When you call `record()`, the entry is routed to the appropriate chain (creating it if needed).

### Eviction

When a chain exceeds its configured `max_entries_per_chain`, the oldest entries are removed. This bounds memory usage while keeping the most recent history.

### Decision Tree Construction

`chain.decision_tree()` builds a linked-list-style tree: the first entry becomes the root, each subsequent entry is appended as a child of the deepest node. Alternatives appear as sibling children of each decision node.

### Gap Detection

`store.audit()` sorts all timestamps across all chains and looks for gaps > 3600 seconds (1 hour). These gaps indicate periods where no decisions were recorded — potentially signalling downtime or missing data.

### Conservation Tracking

Every entry records `conservation_before` and `conservation_after`. The delta indicates whether the decision preserved, increased, or decreased the system's conservation value. Over a long chain, if the system is well-behaved, the total delta should be near zero (conservation law).

## The Math

### Conservation Delta

For a chain of $n$ entries:

$$
\Delta_{\text{total}} = \sum_{i=1}^{n} (c_{\text{after}}^{(i)} - c_{\text{before}}^{(i)})
$$

A well-behaved system has $\Delta_{\text{total}} \approx 0$.

### Decision Confidence

Entries are classified as "good decisions" when:

$$
\text{confidence} > 0.7
$$

And "conservation neutral" when:

$$
|\Delta_i| < \varepsilon = 10^{-9}
$$

### Gap Detection

A gap is any pair of consecutive timestamps $(t_i, t_{i+1})$ where:

$$
t_{i+1} - t_i > 3600 \text{ seconds}
$$

### Store Eviction

When $\text{len}(chain) > M$ (the configured max), entries are removed from the front (oldest first) until $\text{len}(chain) \leq M$. This is FIFO eviction.

## Testing

74 tests covering entry creation, chain operations, decision trees, querying (single and combined filters), store eviction, audit statistics, gap detection, conservation properties, JSON/CSV/Mermaid export, round-trip fidelity, and error handling.

```bash
cargo test
```

## License

MIT
