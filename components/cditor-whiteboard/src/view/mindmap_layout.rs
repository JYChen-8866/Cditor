impl WhiteboardView {
    fn relayout_mindmap_tree(&mut self, root_id: u64) {
        let zoom = self.scene.camera.zoom.max(MIN_ZOOM);
        let mut moved = vec![root_id];
        self.relayout_mindmap_subtree(root_id, &mut moved, zoom);
        self.sync_mindmap_links_for_root(root_id);
        self.sync_segment_anchors_for(&moved);
    }

    fn bump_mindmap_sibling_orders(&mut self, parent: u64, side: MindMapSide, from_order: usize) {
        for element in &mut self.scene.elements {
            if let Some(meta) = &mut element.mindmap
                && meta.parent == Some(parent)
                && meta.side == side
                && meta.order >= from_order
            {
                meta.order += 1;
            }
        }
    }

    fn create_mindmap_node(
        &mut self,
        parent: u64,
        side: MindMapSide,
        order: usize,
        label: &str,
    ) -> u64 {
        self.bump_mindmap_sibling_orders(parent, side, order);
        let zoom = self.scene.camera.zoom.max(MIN_ZOOM);
        let id = self.next_id;
        self.next_id += 1;
        let w = MINDMAP_NODE_W / zoom;
        let h = MINDMAP_NODE_H / zoom;
        let (x, y) = self
            .scene
            .elements
            .iter()
            .find(|element| element.id == parent)
            .and_then(|element| box_like(&element.kind))
            .map(|(px, py, pw, ph, _)| match side {
                MindMapSide::Right => (
                    px + pw + MINDMAP_BRANCH_GAP_X / zoom,
                    py + ph / 2.0 - h / 2.0,
                ),
                MindMapSide::Left => (
                    px - MINDMAP_BRANCH_GAP_X / zoom - w,
                    py + ph / 2.0 - h / 2.0,
                ),
            })
            .unwrap_or((0.0, 0.0));
        self.scene.elements.push(Element {
            id,
            kind: ElementKind::RoundRect(BoxGeom {
                x,
                y,
                w,
                h,
                width: NIB / zoom,
                rotation: 0.0,
            }),
            stroke: Some(0x2563ebff),
            fill: Some(0xffffffff),
            label: Some(label.to_string()),
            label_color: Some(0x0f172aff),
            styles: Vec::new(),
            mindmap: Some(MindMapNodeMeta {
                parent: Some(parent),
                side,
                order,
                root_direction: MindMapRootDirection::Both,
                connector_style: MindMapConnectorStyle::Bezier,
            }),
        });
        let start_anchor = SegmentAnchor {
            element_id: parent,
            connector: match side {
                MindMapSide::Right => 1,
                MindMapSide::Left => 3,
            },
        };
        let end_anchor = SegmentAnchor {
            element_id: id,
            connector: match side {
                MindMapSide::Right => 3,
                MindMapSide::Left => 1,
            },
        };
        let [x1, y1] = connector_world_pos_in(&self.scene.elements, start_anchor).unwrap_or([x, y]);
        let [x2, y2] = connector_world_pos_in(&self.scene.elements, end_anchor).unwrap_or([x, y]);
        let line_id = self.next_id;
        self.next_id += 1;
        self.scene.elements.push(Element {
            id: line_id,
            kind: ElementKind::Arrow(SegGeom {
                x1,
                y1,
                x2,
                y2,
                width: NIB / zoom,
                style: SegmentStyle::Solid,
                start_anchor: Some(start_anchor),
                end_anchor: Some(end_anchor),
            }),
            stroke: Some(0x2563ebff),
            fill: None,
            label: None,
            label_color: None,
            styles: Vec::new(),
            mindmap: None,
        });
        if let Some(root_id) = self.mindmap_root_of(parent) {
            self.relayout_mindmap_tree(root_id);
        }
        id
    }

    fn add_mindmap_relative(
        &mut self,
        source_id: u64,
        sibling: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(meta) = self.mindmap_meta(source_id) else {
            return false;
        };
        let (parent, side, order) = if sibling {
            match meta.parent {
                Some(parent) => (parent, meta.side, meta.order + 1),
                None => (
                    source_id,
                    MindMapSide::Right,
                    self.mindmap_children(source_id, MindMapSide::Right).len(),
                ),
            }
        } else {
            (
                source_id,
                meta.side,
                self.mindmap_children(source_id, meta.side).len(),
            )
        };
        self.push_undo();
        let new_id = self.create_mindmap_node(parent, side, order, "");
        self.selected = vec![new_id];
        self.begin_text_edit(new_id, 0, window, cx);
        self.dirty = true;
        cx.notify();
        true
    }

    fn mindmap_connector_style_for_element(
        &self,
        kind: &ElementKind,
    ) -> Option<MindMapConnectorStyle> {
        let seg = match kind {
            ElementKind::Line(seg) | ElementKind::Arrow(seg) => seg,
            _ => return None,
        };
        let start_root = seg
            .start_anchor
            .and_then(|anchor| self.mindmap_root_of(anchor.element_id));
        let end_root = seg
            .end_anchor
            .and_then(|anchor| self.mindmap_root_of(anchor.element_id));
        match (start_root, end_root) {
            (Some(a), Some(b)) if a == b => Some(self.mindmap_connector_style_for_root(a)),
            _ => None,
        }
    }

    /// The world point at the center of the current viewport — where paste drops
    /// an image (the host has no access to the camera otherwise).
    pub fn viewport_center(&self) -> [f32; 2] {
        let b = self.bounds.get();
        let cam = self.scene.camera;
        let z = cam.zoom.max(MIN_ZOOM);
        [
            cam.x + f32::from(b.size.width) / 2.0 / z,
            cam.y + f32::from(b.size.height) / 2.0 / z,
        ]
    }

    /// The current board document (for the host to persist).
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Build a local-thumbnail focus spec for the board. The board default has
    /// no natural root, so `Auto` means:
    ///
    /// - selected content, if any
    /// - otherwise the current viewport
    /// - otherwise all content
    pub fn local_thumbnail_spec(
        &self,
        width_px: f32,
        height_px: f32,
    ) -> Option<LocalThumbnailSpec> {
        self.local_thumbnail_spec_for_mode(LocalThumbnailMode::Auto, width_px, height_px)
    }

    pub fn local_thumbnail_snapshot(
        &self,
        width_px: f32,
        height_px: f32,
    ) -> Option<LocalThumbnailSnapshot> {
        self.local_thumbnail_snapshot_for_mode(LocalThumbnailMode::Auto, width_px, height_px)
    }

    pub fn local_thumbnail_spec_for_mode(
        &self,
        mode: LocalThumbnailMode,
        width_px: f32,
        height_px: f32,
    ) -> Option<LocalThumbnailSpec> {
        let scene_bounds = self.scene_bbox();
        let (anchor_element_id, focus) = match mode {
            LocalThumbnailMode::Auto => self
                .selection_bbox()
                .map(|bb| (self.selected_single(), bb))
                .or_else(|| self.viewport_world_bbox().map(|bb| (None, bb)))
                .or_else(|| scene_bounds.map(|bb| (None, bb)))?,
            LocalThumbnailMode::Selection => {
                let bb = self.selection_bbox()?;
                (self.selected_single(), bb)
            }
            LocalThumbnailMode::Viewport => (None, self.viewport_world_bbox()?),
            LocalThumbnailMode::AllContent => (None, scene_bounds?),
            LocalThumbnailMode::Element(id) => (Some(id), self.element_bbox(id)?),
        };
        Some(self.thumbnail_spec_from_bbox(
            anchor_element_id,
            focus,
            scene_bounds,
            width_px,
            height_px,
        ))
    }

    pub fn local_thumbnail_snapshot_for_mode(
        &self,
        mode: LocalThumbnailMode,
        width_px: f32,
        height_px: f32,
    ) -> Option<LocalThumbnailSnapshot> {
        Some(LocalThumbnailSnapshot {
            scene: self.scene.clone(),
            spec: self.local_thumbnail_spec_for_mode(mode, width_px, height_px)?,
        })
    }

    /// The lone selected id, if exactly one element is selected. Single-element
    /// manipulation (resize, endpoints, edit) only applies then.
    fn selected_single(&self) -> Option<u64> {
        match self.selected.as_slice() {
            [id] => Some(*id),
            _ => None,
        }
    }

    fn is_selected(&self, id: u64) -> bool {
        self.selected.contains(&id)
    }

    fn element_bbox(&self, id: u64) -> Option<(f32, f32, f32, f32)> {
        self.scene
            .elements
            .iter()
            .find(|e| e.id == id)
            .map(|e| bbox(&e.kind))
    }

    fn scene_bbox(&self) -> Option<(f32, f32, f32, f32)> {
        scene_bbox_for_local_thumbnail(&self.scene)
    }

    fn viewport_world_bbox(&self) -> Option<(f32, f32, f32, f32)> {
        let b = self.bounds.get();
        let vw = f32::from(b.size.width);
        let vh = f32::from(b.size.height);
        if vw <= 1.0 || vh <= 1.0 {
            return None;
        }
        let cam = self.scene.camera;
        let z = cam.zoom.max(MIN_ZOOM);
        Some((cam.x, cam.y, cam.x + vw / z, cam.y + vh / z))
    }

    fn render_viewport(&self, fallback_size: Option<gpui::Size<Pixels>>) -> Option<WorldViewport> {
        let bounds = self.bounds.get();
        let size = if f32::from(bounds.size.width) > 1.0 && f32::from(bounds.size.height) > 1.0 {
            bounds.size
        } else {
            fallback_size?
        };
        let camera = self.scene.camera;
        WorldViewport::from_canvas(
            f32::from(size.width),
            f32::from(size.height),
            camera.x,
            camera.y,
            camera.zoom.max(MIN_ZOOM),
            VIEWPORT_CULL_MARGIN_PX,
        )
    }

    fn thumbnail_spec_from_bbox(
        &self,
        anchor_element_id: Option<u64>,
        focus: (f32, f32, f32, f32),
        scene_bounds: Option<(f32, f32, f32, f32)>,
        width_px: f32,
        height_px: f32,
    ) -> LocalThumbnailSpec {
        local_thumbnail_spec_from_bbox(anchor_element_id, focus, scene_bounds, width_px, height_px)
    }

    /// World-space bounds enclosing the whole selection, or `None` if empty.
    fn selection_bbox(&self) -> Option<(f32, f32, f32, f32)> {
        let mut it = self
            .scene
            .elements
            .iter()
            .filter(|e| self.selected.contains(&e.id))
            .map(|e| bbox(&e.kind));
        let first = it.next()?;
        Some(it.fold(first, |a, b| {
            (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
        }))
    }

    fn aligned_move_delta(&self, dx: f32, dy: f32) -> (f32, f32, AlignmentGuides) {
        const SNAP_PX: f32 = 6.0;
        let Some(selection) = self.selection_bbox() else {
            return (dx, dy, AlignmentGuides::default());
        };
        let threshold = SNAP_PX / self.scene.camera.zoom.max(MIN_ZOOM);
        let moving_x = [
            selection.0 + dx,
            (selection.0 + selection.2) / 2.0 + dx,
            selection.2 + dx,
        ];
        let moving_y = [
            selection.1 + dy,
            (selection.1 + selection.3) / 2.0 + dy,
            selection.3 + dy,
        ];
        let mut best_x: Option<(f32, f32)> = None;
        let mut best_y: Option<(f32, f32)> = None;

        for element in self
            .scene
            .elements
            .iter()
            .filter(|element| !self.selected.contains(&element.id))
        {
            let bb = bbox(&element.kind);
            for moving in moving_x {
                for target in [bb.0, (bb.0 + bb.2) / 2.0, bb.2] {
                    let correction = target - moving;
                    if correction.abs() <= threshold
                        && best_x.is_none_or(|(best, _)| correction.abs() < best.abs())
                    {
                        best_x = Some((correction, target));
                    }
                }
            }
            for moving in moving_y {
                for target in [bb.1, (bb.1 + bb.3) / 2.0, bb.3] {
                    let correction = target - moving;
                    if correction.abs() <= threshold
                        && best_y.is_none_or(|(best, _)| correction.abs() < best.abs())
                    {
                        best_y = Some((correction, target));
                    }
                }
            }
        }

        (
            dx + best_x.map_or(0.0, |(correction, _)| correction),
            dy + best_y.map_or(0.0, |(correction, _)| correction),
            AlignmentGuides {
                vertical: best_x.map(|(_, target)| target),
                horizontal: best_y.map(|(_, target)| target),
            },
        )
    }

    fn sync_segment_anchors_for(&mut self, changed_ids: &[u64]) {
        if changed_ids.is_empty() {
            return;
        }
        let elements = self.scene.elements.clone();
        for element in &mut self.scene.elements {
            let segment = match &mut element.kind {
                ElementKind::Line(segment) | ElementKind::Arrow(segment) => segment,
                _ => continue,
            };
            if let Some(anchor) = segment.start_anchor
                && changed_ids.contains(&anchor.element_id)
                && let Some(pos) = connector_world_pos_in(&elements, anchor)
            {
                segment.x1 = pos[0];
                segment.y1 = pos[1];
            }
            if let Some(anchor) = segment.end_anchor
                && changed_ids.contains(&anchor.element_id)
                && let Some(pos) = connector_world_pos_in(&elements, anchor)
            {
                segment.x2 = pos[0];
                segment.y2 = pos[1];
            }
        }
    }

    fn detach_segment_bindings_for_move(&mut self, ids: &[u64]) {
        for element in &mut self.scene.elements {
            if !ids.contains(&element.id) {
                continue;
            }
            if let ElementKind::Line(segment) | ElementKind::Arrow(segment) = &mut element.kind {
                if segment
                    .start_anchor
                    .is_some_and(|anchor| !ids.contains(&anchor.element_id))
                {
                    segment.start_anchor = None;
                }
                if segment
                    .end_anchor
                    .is_some_and(|anchor| !ids.contains(&anchor.element_id))
                {
                    segment.end_anchor = None;
                }
            }
        }
    }

    fn set_segment_endpoint_anchor(
        &mut self,
        segment_id: u64,
        endpoint: usize,
        anchor: Option<SegmentAnchor>,
    ) {
        let pos = anchor.and_then(|anchor| {
            connector_world_pos_in(&self.scene.elements, anchor).map(|pos| (anchor, pos))
        });
        let Some(element) = self
            .scene
            .elements
            .iter_mut()
            .find(|element| element.id == segment_id)
        else {
            return;
        };
        let segment = match &mut element.kind {
            ElementKind::Line(segment) | ElementKind::Arrow(segment) => segment,
            _ => return,
        };
        if endpoint == 0 {
            segment.start_anchor = anchor;
            if let Some((_, pos)) = pos {
                segment.x1 = pos[0];
                segment.y1 = pos[1];
            }
        } else {
            segment.end_anchor = anchor;
            if let Some((_, pos)) = pos {
                segment.x2 = pos[0];
                segment.y2 = pos[1];
            }
        }
    }

}
