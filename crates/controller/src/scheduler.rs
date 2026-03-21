use crate::db::NodeRow;

pub fn select_node(nodes: &[NodeRow]) -> Option<&NodeRow> {
    nodes.iter().find(|n| n.status == "ready")
}
