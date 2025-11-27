use std::mem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeDirection {
    Bullish,
    Bearish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeState {
    Open,
    Closed,
}

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub id: u64,
    pub created_at: i64,
    pub direction: NodeDirection,
    pub anchor_at_creation: f64,
    pub extreme_at_creation: f64,
    pub distance_pct: f64,
    pub state: NodeState,
    pub closed_at: Option<i64>,
}

impl Node {
    /// Creates a new node from A0 (anchor) and E0 (extreme),
    /// detects direction and computes frozen distance_pct.
    ///
    /// If the anchor and extreme are identical, the node defaults to a bullish
    /// direction with a zero distance so callers can decide how to handle it.
    pub(crate) fn new(
        id: u64,
        created_at: i64,
        anchor_at_creation: f64,
        extreme_at_creation: f64,
    ) -> Self {
        let (direction, distance_pct) = if extreme_at_creation > anchor_at_creation {
            (
                NodeDirection::Bullish,
                (extreme_at_creation - anchor_at_creation) / anchor_at_creation,
            )
        } else if extreme_at_creation < anchor_at_creation {
            (
                NodeDirection::Bearish,
                (anchor_at_creation - extreme_at_creation) / anchor_at_creation,
            )
        } else {
            (NodeDirection::Bullish, 0.0)
        };

        Self {
            id,
            created_at,
            direction,
            anchor_at_creation,
            extreme_at_creation,
            distance_pct,
            state: NodeState::Open,
            closed_at: None,
        }
    }

    /// Returns the projected price P for a given current anchor `current_anchor`.
    /// Bullish: P = current_anchor * (1.0 + distance_pct)
    /// Bearish: P = current_anchor * (1.0 - distance_pct)
    pub(crate) fn projected_price(&self, current_anchor: f64) -> f64 {
        match self.direction {
            NodeDirection::Bullish => current_anchor * (1.0 + self.distance_pct),
            NodeDirection::Bearish => current_anchor * (1.0 - self.distance_pct),
        }
    }

    /// Updates the node with a single bar (or a synthetic bar).
    ///
    /// If the node is already CLOSED:
    ///   - do nothing, return false.
    ///
    /// If the node is OPEN:
    ///   - compute projection P via projected_price(current_anchor).
    ///   - For bullish: if high >= P -> mark CLOSED, set closed_at = timestamp, return true.
    ///   - For bearish: if low  <= P -> mark CLOSED, set closed_at = timestamp, return true.
    ///   - Otherwise remain OPEN and return false.
    pub(crate) fn update_with_bar(
        &mut self,
        current_anchor: f64,
        high: f64,
        low: f64,
        timestamp: i64,
    ) -> bool {
        if self.state == NodeState::Closed {
            return false;
        }

        let projection = self.projected_price(current_anchor);
        let breached = match self.direction {
            NodeDirection::Bullish => high >= projection,
            NodeDirection::Bearish => low <= projection,
        };

        if breached {
            self.state = NodeState::Closed;
            self.closed_at = Some(timestamp);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum NodeEvent {
    Created(Node),
    Closed {
        node: Node,
        last_anchor_used: f64,
        projected_price: f64,
    },
}

#[derive(Debug, Default)]
pub(crate) struct NodeManager {
    open_nodes: Vec<Node>,
    closed_nodes: Vec<Node>,
    next_id: u64,
    recent_events: Vec<NodeEvent>,
}

impl NodeManager {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Creates and adds a new node, returns its id.
    pub(crate) fn create_node(
        &mut self,
        created_at: i64,
        anchor_at_creation: f64,
        extreme_at_creation: f64,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let node = Node::new(id, created_at, anchor_at_creation, extreme_at_creation);
        self.recent_events.push(NodeEvent::Created(node.clone()));
        self.open_nodes.push(node);
        id
    }

    /// Called on each bar.
    /// - current_anchor: anchor (pivot) at this bar
    /// - high, low: bar high/low
    /// - timestamp: bar timestamp
    ///
    /// For each open node:
    ///   - call node.update_with_bar(...)
    ///   - if node became CLOSED, move it to closed_nodes.
    pub(crate) fn on_bar(&mut self, current_anchor: f64, high: f64, low: f64, timestamp: i64) {
        let mut idx = 0;
        while idx < self.open_nodes.len() {
            let should_close = {
                let node = &mut self.open_nodes[idx];
                node.update_with_bar(current_anchor, high, low, timestamp)
            };

            if should_close {
                let mut node = self.open_nodes.swap_remove(idx);
                let projected_price = node.projected_price(current_anchor);
                node.state = NodeState::Closed;
                self.recent_events.push(NodeEvent::Closed {
                    node: node.clone(),
                    last_anchor_used: current_anchor,
                    projected_price,
                });
                self.closed_nodes.push(node);
            } else {
                idx += 1;
            }
        }
    }

    /// Snapshot of current projections for all open nodes at `current_anchor`.
    /// Returns (id, projected_price).
    pub(crate) fn current_projections(&self, current_anchor: f64) -> Vec<(u64, f64)> {
        self.open_nodes
            .iter()
            .map(|node| (node.id, node.projected_price(current_anchor)))
            .collect()
    }

    pub(crate) fn open_nodes(&self) -> &[Node] {
        &self.open_nodes
    }

    pub(crate) fn closed_nodes(&self) -> &[Node] {
        &self.closed_nodes
    }

    pub(crate) fn take_recent_events(&mut self) -> Vec<NodeEvent> {
        mem::take(&mut self.recent_events)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct Bar {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub anchor: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct NodeCreation {
    pub created_at: i64,
    pub anchor_at_creation: f64,
    pub extreme_at_creation: f64,
}

/// Given:
/// - a complete list of bars (sorted by timestamp ascending)
/// - a list of node-creation events
///
/// Behavior:
/// - sort node_creations by created_at ascending
/// - iterate through the bars in time order
/// - for each bar:
///     - create nodes whose created_at <= bar.timestamp
///     - update NodeManager with on_bar(...)
/// - return NodeManager with correct open/closed states and closed_at timestamps.
pub(crate) fn build_nodes_from_history(
    mut bars: Vec<Bar>,
    mut node_creations: Vec<NodeCreation>,
) -> NodeManager {
    bars.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    node_creations.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let mut manager = NodeManager::new();
    let mut creation_idx = 0usize;

    for bar in bars {
        while creation_idx < node_creations.len()
            && node_creations[creation_idx].created_at <= bar.timestamp
        {
            let creation = &node_creations[creation_idx];
            manager.create_node(
                creation.created_at,
                creation.anchor_at_creation,
                creation.extreme_at_creation,
            );
            creation_idx += 1;
        }

        manager.on_bar(bar.anchor, bar.high, bar.low, bar.timestamp);
    }

    while creation_idx < node_creations.len() {
        let creation = &node_creations[creation_idx];
        manager.create_node(
            creation.created_at,
            creation.anchor_at_creation,
            creation.extreme_at_creation,
        );
        creation_idx += 1;
    }

    manager
}
