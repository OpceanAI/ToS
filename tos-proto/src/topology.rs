use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSpec {
    pub id: String,
    pub conn: String,
    pub role: Role,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Source,
    Destination,
    Relay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeSpec {
    pub from: String,
    pub to: Vec<String>,
    pub mode: EdgeMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeMode {
    Push,
    Sync,
}

#[derive(Debug, Clone, Default)]
pub struct Topology {
    pub nodes: HashMap<String, NodeSpec>,
    pub edges: Vec<EdgeSpec>,
}

impl Topology {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: NodeSpec) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_edge(&mut self, edge: EdgeSpec) {
        self.edges.push(edge);
    }

    pub fn destinations_for(&self, source: &str) -> Vec<&NodeSpec> {
        self.edges
            .iter()
            .filter(|e| e.from == source)
            .flat_map(|e| e.to.iter())
            .filter_map(|id| self.nodes.get(id))
            .collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.from) {
                return Err(format!("edge references unknown source node '{}'", edge.from));
            }
            for to in &edge.to {
                if !self.nodes.contains_key(to) {
                    return Err(format!("edge references unknown destination node '{to}'"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_fanout() {
        let mut t = Topology::new();
        t.add_node(NodeSpec { id: "pg".into(), conn: "postgres://".into(), role: Role::Source });
        t.add_node(NodeSpec { id: "redis".into(), conn: "redis://".into(), role: Role::Destination });
        t.add_node(NodeSpec { id: "json".into(), conn: "json://".into(), role: Role::Destination });
        t.add_edge(EdgeSpec {
            from: "pg".into(),
            to: vec!["redis".into(), "json".into()],
            mode: EdgeMode::Sync,
        });
        assert_eq!(t.destinations_for("pg").len(), 2);
    }

    #[test]
    fn topology_validation_unknown_source() {
        let mut t = Topology::new();
        t.add_node(NodeSpec { id: "a".into(), conn: "x".into(), role: Role::Destination });
        t.add_edge(EdgeSpec { from: "missing".into(), to: vec!["a".into()], mode: EdgeMode::Push });
        assert!(t.validate().is_err());
    }

    #[test]
    fn topology_validation_unknown_dest() {
        let mut t = Topology::new();
        t.add_node(NodeSpec { id: "a".into(), conn: "x".into(), role: Role::Source });
        t.add_edge(EdgeSpec { from: "a".into(), to: vec!["missing".into()], mode: EdgeMode::Push });
        assert!(t.validate().is_err());
    }
}
