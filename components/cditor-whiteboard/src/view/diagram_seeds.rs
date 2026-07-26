impl WhiteboardView {
    /// Insert a native mind-map seed built from whiteboard round-rect nodes and
    /// anchored arrows. This stays entirely inside the whiteboard scene model, so
    /// selection, movement, text editing, IME, and connector-follow all reuse the
    /// existing whiteboard machinery.
    pub fn add_mindmap_seed(&mut self, center_x: f32, center_y: f32, cx: &mut Context<Self>) {
        self.push_undo();
        let zoom = self.scene.camera.zoom.max(MIN_ZOOM);
        let root_w = MINDMAP_ROOT_W / zoom;
        let root_h = MINDMAP_ROOT_H / zoom;
        let node_w = MINDMAP_NODE_W / zoom;
        let node_h = MINDMAP_NODE_H / zoom;
        let gap_x = MINDMAP_BRANCH_GAP_X / zoom;
        let gap_y = MINDMAP_BRANCH_GAP_Y / zoom;
        let stroke = Some(0x2563ebff);
        let root_fill = Some(0xdbeafeff);
        let node_fill = Some(0xffffffff);

        let add_node = |scene: &mut Scene,
                        next_id: &mut u64,
                        x: f32,
                        y: f32,
                        w: f32,
                        h: f32,
                        label: &str,
                        fill: Option<u32>,
                        mindmap: Option<MindMapNodeMeta>| {
            let id = *next_id;
            *next_id += 1;
            scene.elements.push(Element {
                id,
                kind: ElementKind::RoundRect(BoxGeom {
                    x,
                    y,
                    w,
                    h,
                    width: NIB / zoom,
                    rotation: 0.0,
                }),
                stroke,
                fill,
                label: Some(label.to_string()),
                label_color: Some(0x0f172aff),
                styles: Vec::new(),
                mindmap,
            });
            id
        };

        let root_id = add_node(
            &mut self.scene,
            &mut self.next_id,
            center_x - root_w / 2.0,
            center_y - root_h / 2.0,
            root_w,
            root_h,
            "中心主题",
            root_fill,
            Some(MindMapNodeMeta {
                parent: None,
                side: MindMapSide::Right,
                order: 0,
                root_direction: MindMapRootDirection::Both,
                connector_style: MindMapConnectorStyle::Bezier,
            }),
        );
        let right_top_id = add_node(
            &mut self.scene,
            &mut self.next_id,
            center_x + root_w / 2.0 + gap_x,
            center_y - gap_y - node_h / 2.0,
            node_w,
            node_h,
            "分支 1",
            node_fill,
            Some(MindMapNodeMeta {
                parent: Some(root_id),
                side: MindMapSide::Right,
                order: 0,
                root_direction: MindMapRootDirection::Both,
                connector_style: MindMapConnectorStyle::Bezier,
            }),
        );
        let right_bottom_id = add_node(
            &mut self.scene,
            &mut self.next_id,
            center_x + root_w / 2.0 + gap_x,
            center_y + gap_y - node_h / 2.0,
            node_w,
            node_h,
            "分支 2",
            node_fill,
            Some(MindMapNodeMeta {
                parent: Some(root_id),
                side: MindMapSide::Right,
                order: 1,
                root_direction: MindMapRootDirection::Both,
                connector_style: MindMapConnectorStyle::Bezier,
            }),
        );
        let left_top_id = add_node(
            &mut self.scene,
            &mut self.next_id,
            center_x - root_w / 2.0 - gap_x - node_w,
            center_y - gap_y - node_h / 2.0,
            node_w,
            node_h,
            "分支 3",
            node_fill,
            Some(MindMapNodeMeta {
                parent: Some(root_id),
                side: MindMapSide::Left,
                order: 0,
                root_direction: MindMapRootDirection::Both,
                connector_style: MindMapConnectorStyle::Bezier,
            }),
        );
        let left_bottom_id = add_node(
            &mut self.scene,
            &mut self.next_id,
            center_x - root_w / 2.0 - gap_x - node_w,
            center_y + gap_y - node_h / 2.0,
            node_w,
            node_h,
            "分支 4",
            node_fill,
            Some(MindMapNodeMeta {
                parent: Some(root_id),
                side: MindMapSide::Left,
                order: 1,
                root_direction: MindMapRootDirection::Both,
                connector_style: MindMapConnectorStyle::Bezier,
            }),
        );

        let add_branch = |scene: &mut Scene,
                          next_id: &mut u64,
                          from_id: u64,
                          from_connector: usize,
                          to_id: u64,
                          to_connector: usize| {
            let start_anchor = SegmentAnchor {
                element_id: from_id,
                connector: from_connector,
            };
            let end_anchor = SegmentAnchor {
                element_id: to_id,
                connector: to_connector,
            };
            let [x1, y1] =
                connector_world_pos_in(&scene.elements, start_anchor).unwrap_or([0.0, 0.0]);
            let [x2, y2] =
                connector_world_pos_in(&scene.elements, end_anchor).unwrap_or([0.0, 0.0]);
            let id = *next_id;
            *next_id += 1;
            scene.elements.push(Element {
                id,
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
                stroke,
                fill: None,
                label: None,
                label_color: None,
                styles: Vec::new(),
                mindmap: None,
            });
        };

        add_branch(
            &mut self.scene,
            &mut self.next_id,
            root_id,
            1,
            right_top_id,
            3,
        );
        add_branch(
            &mut self.scene,
            &mut self.next_id,
            root_id,
            1,
            right_bottom_id,
            3,
        );
        add_branch(
            &mut self.scene,
            &mut self.next_id,
            root_id,
            3,
            left_top_id,
            1,
        );
        add_branch(
            &mut self.scene,
            &mut self.next_id,
            root_id,
            3,
            left_bottom_id,
            1,
        );

        self.selected = vec![root_id];
        self.tool = Tool::Select;
        cx.notify();
    }

    pub fn add_mindmap_seed_at_viewport_center(&mut self, cx: &mut Context<Self>) {
        let center = self.viewport_center();
        self.add_mindmap_seed(center[0], center[1], cx);
    }

    /// Insert a native flowchart seed made from regular whiteboard nodes and
    /// anchored arrows. This is the first structured flowchart primitive inside
    /// the board and can later grow auto-layout / auto-branch behavior.
    pub fn add_flowchart_seed(&mut self, center_x: f32, center_y: f32, cx: &mut Context<Self>) {
        self.push_undo();
        let zoom = self.scene.camera.zoom.max(MIN_ZOOM);
        let node_w = FLOWCHART_NODE_W / zoom;
        let node_h = FLOWCHART_NODE_H / zoom;
        let gap_y = FLOWCHART_GAP_Y / zoom;
        let branch_gap_x = FLOWCHART_BRANCH_GAP_X / zoom;
        let stroke = Some(0x0f172aff);
        let fill = Some(0xffffffff);

        let add_box = |scene: &mut Scene, next_id: &mut u64, kind: ElementKind, label: &str| {
            let id = *next_id;
            *next_id += 1;
            scene.elements.push(Element {
                id,
                kind,
                stroke,
                fill,
                label: Some(label.to_string()),
                label_color: Some(0x0f172aff),
                styles: Vec::new(),
                mindmap: None,
            });
            id
        };
        let add_arrow = |scene: &mut Scene,
                         next_id: &mut u64,
                         from_id: u64,
                         from_connector: usize,
                         to_id: u64,
                         to_connector: usize| {
            let start_anchor = SegmentAnchor {
                element_id: from_id,
                connector: from_connector,
            };
            let end_anchor = SegmentAnchor {
                element_id: to_id,
                connector: to_connector,
            };
            let [x1, y1] =
                connector_world_pos_in(&scene.elements, start_anchor).unwrap_or([0.0, 0.0]);
            let [x2, y2] =
                connector_world_pos_in(&scene.elements, end_anchor).unwrap_or([0.0, 0.0]);
            let id = *next_id;
            *next_id += 1;
            scene.elements.push(Element {
                id,
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
                stroke,
                fill: None,
                label: None,
                label_color: None,
                styles: Vec::new(),
                mindmap: None,
            });
        };

        let start_id = add_box(
            &mut self.scene,
            &mut self.next_id,
            ElementKind::Ellipse(BoxGeom {
                x: center_x - node_w / 2.0,
                y: center_y - gap_y - node_h * 1.5,
                w: node_w,
                h: node_h,
                width: NIB / zoom,
                rotation: 0.0,
            }),
            "开始",
        );
        let process_id = add_box(
            &mut self.scene,
            &mut self.next_id,
            ElementKind::RoundRect(BoxGeom {
                x: center_x - node_w / 2.0,
                y: center_y - node_h / 2.0,
                w: node_w,
                h: node_h,
                width: NIB / zoom,
                rotation: 0.0,
            }),
            "处理",
        );
        let decision_id = add_box(
            &mut self.scene,
            &mut self.next_id,
            ElementKind::Diamond(BoxGeom {
                x: center_x - node_w / 2.0,
                y: center_y + gap_y - node_h / 2.0,
                w: node_w,
                h: node_h,
                width: NIB / zoom,
                rotation: 0.0,
            }),
            "判断",
        );
        let branch_yes_id = add_box(
            &mut self.scene,
            &mut self.next_id,
            ElementKind::RoundRect(BoxGeom {
                x: center_x + branch_gap_x - node_w / 2.0,
                y: center_y + gap_y - node_h / 2.0,
                w: node_w,
                h: node_h,
                width: NIB / zoom,
                rotation: 0.0,
            }),
            "是",
        );
        let branch_no_id = add_box(
            &mut self.scene,
            &mut self.next_id,
            ElementKind::RoundRect(BoxGeom {
                x: center_x - branch_gap_x - node_w / 2.0,
                y: center_y + gap_y - node_h / 2.0,
                w: node_w,
                h: node_h,
                width: NIB / zoom,
                rotation: 0.0,
            }),
            "否",
        );
        let end_id = add_box(
            &mut self.scene,
            &mut self.next_id,
            ElementKind::Ellipse(BoxGeom {
                x: center_x - node_w / 2.0,
                y: center_y + gap_y * 2.0 + node_h * 0.5,
                w: node_w,
                h: node_h,
                width: NIB / zoom,
                rotation: 0.0,
            }),
            "结束",
        );

        add_arrow(
            &mut self.scene,
            &mut self.next_id,
            start_id,
            2,
            process_id,
            0,
        );
        add_arrow(
            &mut self.scene,
            &mut self.next_id,
            process_id,
            2,
            decision_id,
            0,
        );
        add_arrow(
            &mut self.scene,
            &mut self.next_id,
            decision_id,
            1,
            branch_yes_id,
            3,
        );
        add_arrow(
            &mut self.scene,
            &mut self.next_id,
            decision_id,
            3,
            branch_no_id,
            1,
        );
        add_arrow(
            &mut self.scene,
            &mut self.next_id,
            branch_yes_id,
            2,
            end_id,
            1,
        );
        add_arrow(
            &mut self.scene,
            &mut self.next_id,
            branch_no_id,
            2,
            end_id,
            3,
        );

        self.selected = vec![process_id];
        self.tool = Tool::Select;
        cx.notify();
    }

    pub fn add_flowchart_seed_at_viewport_center(&mut self, cx: &mut Context<Self>) {
        let center = self.viewport_center();
        self.add_flowchart_seed(center[0], center[1], cx);
    }

}
