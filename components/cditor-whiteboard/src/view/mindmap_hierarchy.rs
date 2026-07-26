impl WhiteboardView {
    fn mindmap_meta(&self, id: u64) -> Option<MindMapNodeMeta> {
        self.scene
            .elements
            .iter()
            .find(|element| element.id == id)
            .and_then(|element| element.mindmap)
    }

    fn is_mindmap_node(&self, id: u64) -> bool {
        self.mindmap_meta(id).is_some()
    }

    fn is_mindmap_root(&self, id: u64) -> bool {
        self.mindmap_meta(id)
            .is_some_and(|meta| meta.parent.is_none())
    }

    fn selected_mindmap_root(&self) -> Option<u64> {
        self.selected_single()
            .filter(|id| self.is_mindmap_root(*id))
    }

    fn mindmap_root_direction(&self, root_id: u64) -> MindMapRootDirection {
        self.mindmap_meta(root_id)
            .map(|meta| meta.root_direction)
            .unwrap_or_default()
    }

    fn mindmap_connector_style_for_root(&self, root_id: u64) -> MindMapConnectorStyle {
        self.mindmap_meta(root_id)
            .map(|meta| meta.connector_style)
            .unwrap_or_default()
    }

    fn mindmap_root_of(&self, id: u64) -> Option<u64> {
        let mut current = id;
        loop {
            let meta = self.mindmap_meta(current)?;
            match meta.parent {
                Some(parent) => current = parent,
                None => return Some(current),
            }
        }
    }

    fn mindmap_children(&self, parent: u64, side: MindMapSide) -> Vec<u64> {
        let mut children: Vec<(usize, u64)> = self
            .scene
            .elements
            .iter()
            .filter_map(|element| {
                let meta = element.mindmap?;
                (meta.parent == Some(parent) && meta.side == side)
                    .then_some((meta.order, element.id))
            })
            .collect();
        children.sort_by_key(|(order, id)| (*order, *id));
        children.into_iter().map(|(_, id)| id).collect()
    }

    fn set_mindmap_node_side(&mut self, id: u64, side: MindMapSide) {
        if let Some(element) = self
            .scene
            .elements
            .iter_mut()
            .find(|element| element.id == id)
            && let Some(meta) = &mut element.mindmap
        {
            meta.side = side;
        }
        self.sync_mindmap_parent_link(id);
    }

    fn sync_mindmap_parent_link(&mut self, child_id: u64) {
        let Some(meta) = self.mindmap_meta(child_id) else {
            return;
        };
        let Some(parent_id) = meta.parent else {
            return;
        };
        let parent_connector = match meta.side {
            MindMapSide::Right => 1,
            MindMapSide::Left => 3,
        };
        let child_connector = match meta.side {
            MindMapSide::Right => 3,
            MindMapSide::Left => 1,
        };
        for element in &mut self.scene.elements {
            let segment = match &mut element.kind {
                ElementKind::Line(segment) | ElementKind::Arrow(segment) => segment,
                _ => continue,
            };
            let start_id = segment.start_anchor.map(|anchor| anchor.element_id);
            let end_id = segment.end_anchor.map(|anchor| anchor.element_id);
            let links_parent_child = matches!((start_id, end_id), (Some(a), Some(b)) if (a == parent_id && b == child_id) || (a == child_id && b == parent_id));
            if !links_parent_child {
                continue;
            }
            if let Some(anchor) = &mut segment.start_anchor {
                if anchor.element_id == parent_id {
                    anchor.connector = parent_connector;
                } else if anchor.element_id == child_id {
                    anchor.connector = child_connector;
                }
            }
            if let Some(anchor) = &mut segment.end_anchor {
                if anchor.element_id == parent_id {
                    anchor.connector = parent_connector;
                } else if anchor.element_id == child_id {
                    anchor.connector = child_connector;
                }
            }
        }
        self.sync_segment_anchors_for(&[parent_id, child_id]);
    }

    fn sync_mindmap_links_for_root(&mut self, root_id: u64) {
        let child_ids: Vec<u64> = self
            .scene
            .elements
            .iter()
            .filter_map(|element| {
                let meta = element.mindmap?;
                meta.parent?;
                (self.mindmap_root_of(element.id) == Some(root_id)).then_some(element.id)
            })
            .collect();
        for child_id in &child_ids {
            let Some(meta) = self.mindmap_meta(*child_id) else {
                continue;
            };
            let Some(parent_id) = meta.parent else {
                continue;
            };
            let parent_connector = match meta.side {
                MindMapSide::Right => 1,
                MindMapSide::Left => 3,
            };
            let child_connector = match meta.side {
                MindMapSide::Right => 3,
                MindMapSide::Left => 1,
            };
            for element in &mut self.scene.elements {
                let segment = match &mut element.kind {
                    ElementKind::Line(segment) | ElementKind::Arrow(segment) => segment,
                    _ => continue,
                };
                let start_id = segment.start_anchor.map(|anchor| anchor.element_id);
                let end_id = segment.end_anchor.map(|anchor| anchor.element_id);
                let links_parent_child = matches!((start_id, end_id), (Some(a), Some(b)) if (a == parent_id && b == *child_id) || (a == *child_id && b == parent_id));
                if !links_parent_child {
                    continue;
                }
                if let Some(anchor) = &mut segment.start_anchor {
                    if anchor.element_id == parent_id {
                        anchor.connector = parent_connector;
                    } else if anchor.element_id == *child_id {
                        anchor.connector = child_connector;
                    }
                }
                if let Some(anchor) = &mut segment.end_anchor {
                    if anchor.element_id == parent_id {
                        anchor.connector = parent_connector;
                    } else if anchor.element_id == *child_id {
                        anchor.connector = child_connector;
                    }
                }
            }
        }
        self.sync_segment_anchors_for(&child_ids);
    }

    fn ordered_mindmap_children(&self, parent: u64) -> Vec<u64> {
        let mut children: Vec<(f32, f32, usize, u64)> = self
            .scene
            .elements
            .iter()
            .filter_map(|element| {
                let meta = element.mindmap?;
                (meta.parent == Some(parent)).then(|| {
                    let (x, y, _, h, _) =
                        box_like(&element.kind).unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0));
                    (y + h / 2.0, x, meta.order, element.id)
                })
            })
            .collect();
        children.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| a.1.total_cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.3.cmp(&b.3))
        });
        children.into_iter().map(|(_, _, _, id)| id).collect()
    }

    fn set_mindmap_children_side(&mut self, parent: u64, side: MindMapSide) {
        for child_id in self.ordered_mindmap_children(parent) {
            self.set_mindmap_node_side(child_id, side);
        }
    }

    fn set_mindmap_children_side_alternating(&mut self, parent: u64) {
        for (index, child_id) in self
            .ordered_mindmap_children(parent)
            .into_iter()
            .enumerate()
        {
            let side = if index % 2 == 0 {
                MindMapSide::Right
            } else {
                MindMapSide::Left
            };
            self.set_mindmap_node_side(child_id, side);
        }
    }

    fn reindex_mindmap_children(&mut self, parent: u64) {
        for side in [MindMapSide::Left, MindMapSide::Right] {
            let children = self.mindmap_children(parent, side);
            for (order, child_id) in children.into_iter().enumerate() {
                if let Some(element) = self
                    .scene
                    .elements
                    .iter_mut()
                    .find(|element| element.id == child_id)
                    && let Some(meta) = &mut element.mindmap
                {
                    meta.order = order;
                }
                self.reindex_mindmap_children(child_id);
            }
        }
    }

    fn set_mindmap_root_direction(
        &mut self,
        root_id: u64,
        direction: MindMapRootDirection,
        cx: &mut Context<Self>,
    ) {
        if let Some(element) = self
            .scene
            .elements
            .iter_mut()
            .find(|element| element.id == root_id)
            && let Some(meta) = &mut element.mindmap
        {
            meta.root_direction = direction;
        }
        match direction {
            MindMapRootDirection::Left => {
                self.set_mindmap_children_side(root_id, MindMapSide::Left)
            }
            MindMapRootDirection::Right => {
                self.set_mindmap_children_side(root_id, MindMapSide::Right)
            }
            MindMapRootDirection::Both => self.set_mindmap_children_side_alternating(root_id),
        }
        self.reindex_mindmap_children(root_id);
        self.relayout_mindmap_tree(root_id);
        cx.notify();
    }

    fn set_mindmap_connector_style(
        &mut self,
        root_id: u64,
        style: MindMapConnectorStyle,
        cx: &mut Context<Self>,
    ) {
        if let Some(element) = self
            .scene
            .elements
            .iter_mut()
            .find(|element| element.id == root_id)
            && let Some(meta) = &mut element.mindmap
        {
            meta.connector_style = style;
        }
        cx.notify();
    }

    fn set_mindmap_node_position(&mut self, id: u64, x: f32, y: f32) {
        if let Some(element) = self
            .scene
            .elements
            .iter_mut()
            .find(|element| element.id == id)
            && let ElementKind::RoundRect(geom) = &mut element.kind
        {
            geom.x = x;
            geom.y = y;
        }
    }

    fn mindmap_node_size(&self, id: u64, zoom: f32) -> (f32, f32) {
        self.scene
            .elements
            .iter()
            .find(|element| element.id == id)
            .and_then(|element| box_like(&element.kind).map(|(_, _, w, h, _)| (w, h)))
            .unwrap_or((MINDMAP_NODE_W / zoom, MINDMAP_NODE_H / zoom))
    }

    fn side_stack_height(&self, parent: u64, side: MindMapSide, zoom: f32) -> f32 {
        let children = self.mindmap_children(parent, side);
        if children.is_empty() {
            return 0.0;
        }
        let gap_y = MINDMAP_BRANCH_GAP_Y / zoom;
        children
            .into_iter()
            .enumerate()
            .fold(0.0, |acc, (index, child_id)| {
                acc + if index > 0 { gap_y } else { 0.0 }
                    + self.mindmap_subtree_height(child_id, zoom)
            })
    }

    fn mindmap_subtree_height(&self, id: u64, zoom: f32) -> f32 {
        let (_, node_h) = self.mindmap_node_size(id, zoom);
        node_h.max(
            self.side_stack_height(id, MindMapSide::Left, zoom)
                .max(self.side_stack_height(id, MindMapSide::Right, zoom)),
        )
    }

    fn relayout_mindmap_subtree(&mut self, node_id: u64, moved: &mut Vec<u64>, zoom: f32) {
        self.relayout_mindmap_children(node_id, MindMapSide::Left, moved, zoom);
        self.relayout_mindmap_children(node_id, MindMapSide::Right, moved, zoom);
    }

    fn relayout_mindmap_children(
        &mut self,
        parent_id: u64,
        side: MindMapSide,
        moved: &mut Vec<u64>,
        zoom: f32,
    ) {
        let children = self.mindmap_children(parent_id, side);
        if children.is_empty() {
            return;
        }
        let Some((px, py, pw, ph, _)) = self
            .scene
            .elements
            .iter()
            .find(|element| element.id == parent_id)
            .and_then(|element| box_like(&element.kind))
        else {
            return;
        };
        let gap_x = MINDMAP_BRANCH_GAP_X / zoom;
        let gap_y = MINDMAP_BRANCH_GAP_Y / zoom;
        let total_h = children
            .iter()
            .enumerate()
            .fold(0.0, |acc, (index, child_id)| {
                acc + if index > 0 { gap_y } else { 0.0 }
                    + self.mindmap_subtree_height(*child_id, zoom)
            });
        let mut cursor_y = py + ph / 2.0 - total_h / 2.0;
        for child_id in children {
            let subtree_h = self.mindmap_subtree_height(child_id, zoom);
            let (cw, ch) = self.mindmap_node_size(child_id, zoom);
            let cy = cursor_y + subtree_h / 2.0;
            let x = match side {
                MindMapSide::Right => px + pw + gap_x,
                MindMapSide::Left => px - gap_x - cw,
            };
            self.set_mindmap_node_position(child_id, x, cy - ch / 2.0);
            moved.push(child_id);
            self.relayout_mindmap_subtree(child_id, moved, zoom);
            cursor_y += subtree_h + gap_y;
        }
    }

}
