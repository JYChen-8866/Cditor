use accesskit::{Action, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate};
use cditor_core::edit::TextAffinity;
use parley::{Affinity, Cursor, LayoutAccessibility, Selection};

use super::{ParleyLayoutSnapshot, ParleySelection, ParleyTextPosition};

#[derive(Clone, Debug)]
pub struct ParleyAccessibilityProjection {
    pub parent_id: NodeId,
    pub update: TreeUpdate,
}

impl ParleyAccessibilityProjection {
    pub fn parent_node(&self) -> Option<&Node> {
        self.update
            .nodes
            .iter()
            .find_map(|(id, node)| (*id == self.parent_id).then_some(node))
    }
}

pub fn build_parley_accessibility_projection(
    snapshot: &ParleyLayoutSnapshot,
    parent_id: NodeId,
    first_child_id: NodeId,
    origin_x: f64,
    origin_y: f64,
    selection: Option<ParleySelection>,
) -> ParleyAccessibilityProjection {
    let mut layout_access = LayoutAccessibility::default();
    let mut update = TreeUpdate {
        nodes: Vec::new(),
        tree: Some(Tree::new(parent_id)),
        tree_id: TreeId::ROOT,
        focus: parent_id,
    };
    let mut parent = Node::new(Role::MultilineTextInput);
    parent.set_bounds(Rect {
        x0: origin_x,
        y0: origin_y,
        x1: origin_x + snapshot.width() as f64,
        y1: origin_y + snapshot.height() as f64,
    });
    let mut next_id = first_child_id.0;
    layout_access.build_nodes(
        snapshot.text(),
        snapshot.layout(),
        &mut update,
        &mut parent,
        || {
            let id = NodeId(next_id);
            next_id = next_id.saturating_add(1);
            id
        },
        origin_x,
        origin_y,
        |_node, _style| {},
    );
    if let Some(selection) = selection
        && let Some(selection) = parley_selection(selection, snapshot)
        && let Some(selection) = selection.to_access_selection(snapshot.layout(), &layout_access)
    {
        parent.set_text_selection(selection);
    }
    parent.add_action(Action::SetTextSelection);
    update.nodes.push((parent_id, parent));
    ParleyAccessibilityProjection { parent_id, update }
}

fn parley_selection(
    selection: ParleySelection,
    snapshot: &ParleyLayoutSnapshot,
) -> Option<Selection> {
    Some(Selection::new(
        parley_cursor(selection.anchor, snapshot)?,
        parley_cursor(selection.focus, snapshot)?,
    ))
}

fn parley_cursor(position: ParleyTextPosition, snapshot: &ParleyLayoutSnapshot) -> Option<Cursor> {
    (position.offset <= snapshot.text().len()).then(|| {
        Cursor::from_byte_index(
            snapshot.layout(),
            position.offset,
            match position.affinity {
                TextAffinity::Upstream => Affinity::Upstream,
                TextAffinity::Downstream => Affinity::Downstream,
            },
        )
    })
}

pub fn accessibility_node_ids(
    block_id: u64,
    table_cell: Option<(usize, usize)>,
) -> (NodeId, NodeId) {
    let cell_component = table_cell
        .map(|(row, col)| ((row as u64) << 20) ^ col as u64)
        .unwrap_or(0);
    let base = block_id
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(cell_component)
        & 0x3FFF_FFFF_FFFF_FFFF;
    (NodeId(base << 1), NodeId((base << 1).saturating_add(1)))
}
