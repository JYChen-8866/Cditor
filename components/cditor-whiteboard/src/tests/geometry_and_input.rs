    #[test]
    fn pan_and_zoom_math() {
        let mut c = Camera::default();
        c.pan_by(50.0, -20.0);
        assert_eq!((c.x, c.y), (-50.0, 20.0));

        let mut c = Camera {
            x: 10.0,
            y: 5.0,
            zoom: 1.0,
        };
        let before = c.screen_to_world(300.0, 200.0);
        c.zoom_about(300.0, 200.0, 2.5);
        let after = c.screen_to_world(300.0, 200.0);
        assert!((before.0 - after.0).abs() < 1e-3);
        assert!((before.1 - after.1).abs() < 1e-3);
        assert_eq!(c.zoom, 2.5);
    }

    #[test]
    fn bbox_translate_and_hit_test() {
        let mut k = ElementKind::Line(SegGeom {
            x1: 0.0,
            y1: 0.0,
            x2: 10.0,
            y2: 4.0,
            width: 1.0,
            style: SegmentStyle::Solid,
            start_anchor: None,
            end_anchor: None,
        });
        assert_eq!(bbox(&k), (0.0, 0.0, 10.0, 4.0));
        translate(&mut k, 5.0, -2.0);
        assert_eq!(bbox(&k), (5.0, -2.0, 15.0, 2.0));
        // Within the padded bounds hits; far away misses.
        assert!(hit_test(&k, 5.0, -2.0, 1.0));
        assert!(hit_test(&k, 4.5, -2.5, 1.0)); // inside pad
        assert!(!hit_test(&k, 100.0, 100.0, 1.0));
    }

    #[test]
    fn diagonal_scale_projects_the_cursor_onto_the_diagonal() {
        // On the diagonal: cursor twice as far from the anchor → 2×.
        let s = diagonal_scale([0.0, 0.0], [10.0, 10.0], [20.0, 20.0]);
        assert!((s - 2.0).abs() < 1e-4, "{s}");
        // Off-diagonal projects onto it: (20,0) onto the (10,10) line → 1×.
        let s = diagonal_scale([0.0, 0.0], [10.0, 10.0], [20.0, 0.0]);
        assert!((s - 1.0).abs() < 1e-4, "{s}");
    }

    #[test]
    fn snap_45_locks_angle_and_keeps_length() {
        // Near 45° snaps onto the exact diagonal (x == y).
        let (x, y) = snap_45(0.0, 0.0, 10.0, 9.0);
        assert!((x - y).abs() < 1e-3, "{x} vs {y}");
        // Near-horizontal snaps flat, preserving the distance.
        let (x, y) = snap_45(0.0, 0.0, 10.0, 1.0);
        assert!(y.abs() < 1e-3);
        assert!((x - 101.0f32.sqrt()).abs() < 1e-2);
    }

    #[test]
    fn snap_grid_rounds_to_nearest_line() {
        // GRID is 24: values round to the nearest multiple, halves away from zero.
        assert_eq!(snap_grid(0.0), 0.0);
        assert_eq!(snap_grid(11.0), 0.0);
        assert_eq!(snap_grid(13.0), GRID);
        assert_eq!(snap_grid(GRID), GRID);
        assert_eq!(snap_grid(-13.0), -GRID);
        assert_eq!(snap_grid(1.5 * GRID), 2.0 * GRID);
    }

    #[test]
    fn move_target_drives_an_absolute_snapped_target() {
        // Origin off-grid (100 % 24 == 4); grab anchor at the cursor's start.
        let origin = [100.0, 100.0];
        let anchor = [0.0, 0.0];

        // Free move tracks the cursor exactly on both axes.
        assert_eq!(
            move_target(origin, anchor, [37.0, -11.0], false),
            [137.0, 89.0]
        );

        // Snapped: the target is `snap(origin + total)`, computed fresh each
        // frame — never the running position, so it can't stick. A 50,50 total
        // lands on snap(150) = 144 (150/24 = 6.25 → 6).
        assert_eq!(
            move_target(origin, anchor, [50.0, 50.0], true),
            [144.0, 144.0]
        );

        // Regression: twelve sub-threshold 4px steps (each < half a grid cell)
        // must still accumulate across grid lines on BOTH axes — the old logic
        // snapped each tiny step from the already-snapped spot and stuck.
        let mut cursor = [0.0, 0.0];
        for _ in 0..12 {
            cursor = [cursor[0] + 4.0, cursor[1] + 4.0];
        }
        // 48px total → snap(148) = 144 on each axis.
        assert_eq!(move_target(origin, anchor, cursor, true), [144.0, 144.0]);
    }

    #[test]
    fn resize_scales_geometry_about_the_anchor() {
        // Drag the bottom-right corner of a 20×20 rect to double it, anchored
        // at the top-left — origin stays put, size doubles.
        let mut k = ElementKind::Rect(BoxGeom {
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 20.0,
            width: 1.0,
            rotation: 0.0,
        });
        resize_about(&mut k, 10.0, 10.0, 2.0, 2.0);
        match k {
            ElementKind::Rect(b) => {
                assert_eq!((b.x, b.y), (10.0, 10.0));
                assert_eq!((b.w, b.h), (40.0, 40.0));
            }
            other => panic!("expected rect, got {other:?}"),
        }
    }

    #[test]
    fn axis_scale_measures_one_axis_about_the_anchor() {
        // `target` twice as far from the anchor as `from` → 2×.
        assert!((axis_scale(0.0, 10.0, 20.0) - 2.0).abs() < 1e-4);
        // Halfway back toward the anchor → 0.5×.
        assert!((axis_scale(0.0, 10.0, 5.0) - 0.5).abs() < 1e-4);
        // Degenerate (anchor == from) → 1.0, no divide-by-zero.
        assert!((axis_scale(7.0, 7.0, 99.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn per_axis_resize_stretches_one_axis_and_keeps_text_uniform() {
        // A rect stretched on x only (sx=2, sy=1): width doubles, height holds.
        let mut k = ElementKind::Rect(BoxGeom {
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 20.0,
            width: 1.0,
            rotation: 0.0,
        });
        resize_about(&mut k, 10.0, 10.0, 2.0, 1.0);
        match k {
            ElementKind::Rect(b) => {
                assert_eq!((b.x, b.y), (10.0, 10.0));
                assert_eq!((b.w, b.h), (40.0, 20.0));
            }
            other => panic!("expected rect, got {other:?}"),
        }
        // Text under a per-axis (4×, 1×) stretch keeps a single size: the geometric
        // mean (sqrt(4) = 2×), never distorted to the raw 4× horizontal factor.
        let mut t = ElementKind::Text(TextGeom {
            x: 0.0,
            y: 0.0,
            content: "hi".into(),
            size: 10.0,
            rotation: 0.0,
            measured_w: 0.0,
            measured_h: 0.0,
        });
        resize_about(&mut t, 0.0, 0.0, 4.0, 1.0);
        match t {
            ElementKind::Text(t) => assert!((t.size - 20.0).abs() < 1e-3, "{}", t.size),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn color_round_trips_through_hsv_and_packed_ints() {
        // Pure primaries survive HSV → packed → HSV.
        for c in [0xff0000ff, 0x00ff00ff, 0x0000ffff, 0x808080ff, 0xffffffff] {
            let (h, s, v) = u32_to_hsv(c);
            assert_eq!(hsv_to_u32(h, s, v), c, "{c:#010x}");
        }
        // Hue endpoints both land on red.
        assert_eq!(hsv_to_u32(0.0, 1.0, 1.0), 0xff0000ff);
        assert_eq!(hsv_to_u32(1.0, 1.0, 1.0), 0xff0000ff);
        // A 2/3 hue is pure blue.
        assert_eq!(hsv_to_u32(2.0 / 3.0, 1.0, 1.0), 0x0000ffff);
        // pack clamps out-of-range and rounds to 0..255.
        assert_eq!(pack_rgba(1.5, -0.2, 0.5, 1.0), 0xff0080ff);
    }

    #[test]
    fn rotation_accumulates_on_boxes_and_bakes_into_segments() {
        use std::f32::consts::FRAC_PI_2;
        // A box stores the angle and its center-anchored bounds don't move.
        let mut k = ElementKind::Rect(BoxGeom {
            x: -10.0,
            y: -10.0,
            w: 20.0,
            h: 20.0,
            width: 1.0,
            rotation: 0.0,
        });
        rotate_element(&mut k, 0.0, 0.0, FRAC_PI_2);
        match &k {
            ElementKind::Rect(b) => assert!((b.rotation - FRAC_PI_2).abs() < 1e-5),
            other => panic!("expected rect, got {other:?}"),
        }
        // A square's bounds are unchanged by a 90° turn about its center.
        let bb = bbox(&k);
        assert!(
            (bb.0 + 10.0).abs() < 1e-3 && (bb.2 - 10.0).abs() < 1e-3,
            "{bb:?}"
        );

        // A line bakes the rotation into its endpoints: +90° about the origin
        // sends (10,0) → (0,10).
        let mut seg = ElementKind::Line(SegGeom {
            x1: 0.0,
            y1: 0.0,
            x2: 10.0,
            y2: 0.0,
            width: 1.0,
            style: SegmentStyle::Solid,
            start_anchor: None,
            end_anchor: None,
        });
        rotate_element(&mut seg, 0.0, 0.0, FRAC_PI_2);
        match seg {
            ElementKind::Line(s) => {
                assert!(s.x2.abs() < 1e-3 && (s.y2 - 10.0).abs() < 1e-3, "{s:?}");
            }
            other => panic!("expected line, got {other:?}"),
        }

        // Text rotates like a box: spun about its own center, it accumulates an
        // angle and stays put (centered on the pivot here, so no orbit).
        let mut txt = ElementKind::Text(TextGeom {
            x: -20.0,
            y: -8.0,
            content: "hi".into(),
            size: 16.0,
            rotation: 0.0,
            measured_w: 40.0,
            measured_h: 16.0,
        });
        rotate_element(&mut txt, 0.0, 0.0, FRAC_PI_2);
        match txt {
            ElementKind::Text(t) => {
                assert!((t.rotation - FRAC_PI_2).abs() < 1e-5);
                assert!(
                    (t.x + 20.0).abs() < 1e-3 && (t.y + 8.0).abs() < 1e-3,
                    "{t:?}"
                );
            }
            other => panic!("expected text, got {other:?}"),
        }

        // Orbiting: rotating a box about a *different* pivot moves its center
        // along the arc. A unit box at (1,0) turned 90° about the origin → (0,1).
        let mut orb = ElementKind::Rect(BoxGeom {
            x: 0.5,
            y: -0.5,
            w: 1.0,
            h: 1.0,
            width: 1.0,
            rotation: 0.0,
        });
        rotate_element(&mut orb, 0.0, 0.0, FRAC_PI_2);
        match orb {
            ElementKind::Rect(b) => {
                let (ccx, ccy) = (b.x + 0.5, b.y + 0.5);
                assert!(ccx.abs() < 1e-3 && (ccy - 1.0).abs() < 1e-3, "{b:?}");
            }
            other => panic!("expected rect, got {other:?}"),
        }
    }

    #[test]
    fn rotation_snaps_to_horizontal_and_vertical() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
        let step = std::f32::consts::PI / 12.0;
        // Within the snap zone of a cardinal snaps onto it...
        assert!((snap_angle(FRAC_PI_2 - 0.05, false) - FRAC_PI_2).abs() < 1e-6);
        assert!(snap_angle(0.04, false).abs() < 1e-6);
        assert!((snap_angle(-FRAC_PI_2 + 0.03, false) + FRAC_PI_2).abs() < 1e-6);
        // ...but a hair outside it, and at 45°, the angle is left free.
        assert!((snap_angle(FRAC_PI_2 - 0.2, false) - (FRAC_PI_2 - 0.2)).abs() < 1e-6);
        assert!((snap_angle(FRAC_PI_4, false) - FRAC_PI_4).abs() < 1e-6);
        // Shift snaps to the nearest 15° everywhere.
        assert!((snap_angle(0.30, true) - step).abs() < 1e-4);
    }

    #[test]
    fn caret_navigation_walks_chars_and_lines() {
        // Multi-byte: "é" is 2 bytes, so caret steps by whole chars, never panics.
        let s = "aébc";
        assert_eq!(caret_right(s, 0), 1); // past 'a'
        assert_eq!(caret_right(s, 1), 3); // past 'é' (2 bytes)
        assert_eq!(caret_left(s, 3), 1); // back over 'é'
        assert_eq!(caret_left(s, 0), 0); // clamps at start
        assert_eq!(caret_right(s, s.len()), s.len()); // clamps at end
        // Line edges around a newline.
        let m = "ab\ncde";
        assert_eq!(line_start(m, 5), 3); // within "cde" → after the '\n'
        assert_eq!(line_end(m, 0), 2); // end of "ab" (before '\n')
        assert_eq!(line_start(m, 1), 0);
        assert_eq!(line_end(m, 4), m.len());
        // floor_boundary never splits a char.
        assert_eq!(floor_boundary(s, 2), 1); // mid-'é' → its start
        assert_eq!(floor_boundary(s, 99), s.len());
    }

    #[test]
    fn word_range_selects_the_word_under_the_caret() {
        let s = "foo bar_baz qux";
        assert_eq!(word_range(s, 1), (0, 3)); // inside "foo"
        assert_eq!(word_range(s, 7), (4, 11)); // "bar_baz" (underscore is a word char)
        // At a word/space boundary the adjacent word wins (caret just after "foo").
        assert_eq!(word_range(s, 3), (0, 3));
        // Between two spaces → empty (no word under the caret).
        assert_eq!(word_range("a  b", 2), (2, 2));
    }

    #[test]
    fn text_bbox_anchors_at_origin_and_grows() {
        let t = TextGeom {
            x: 5.0,
            y: 6.0,
            content: "ab\ncde".into(),
            size: 10.0,
            rotation: 0.0,
            measured_w: 0.0,
            measured_h: 0.0,
        };
        let bb = bbox(&ElementKind::Text(t));
        assert_eq!((bb.0, bb.1), (5.0, 6.0));
        assert!(bb.2 > bb.0 && bb.3 > bb.1);
    }

    #[test]
    fn tiny_drags_are_not_committed() {
        assert!(!committable(&ElementKind::Draw(Stroke {
            points: vec![[0.0, 0.0]],
            width: 1.0,
        })));
        assert!(committable(&ElementKind::Rect(BoxGeom {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 5.0,
            width: 1.0,
            rotation: 0.0,
        })));
    }

    #[test]
    fn image_round_trips_and_behaves_like_a_box() {
        let kind = ElementKind::Image(ImageGeom {
            src: "images/x.png".into(),
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 60.0,
            rotation: 0.0,
        });
        // Bounds = the box; not a fillable closed shape.
        assert_eq!(bbox(&kind), (10.0, 20.0, 110.0, 80.0));
        assert!(!is_closed_shape(&kind));
        // Round-trips through JSON under the "image" tag, keeping its src.
        let elem = Element {
            id: 1,
            kind,
            stroke: None,
            fill: None,
            label: None,
            label_color: None,
            styles: Vec::new(),
            mindmap: None,
        };
        let json = serde_json::to_string(&elem).unwrap();
        assert!(json.contains("\"image\""), "{json}");
        assert!(json.contains("images/x.png"));
        let mut back = serde_json::from_str::<Element>(&json).unwrap().kind;
        assert_eq!(bbox(&back), (10.0, 20.0, 110.0, 80.0));
        // Translates like the other box kinds.
        translate(&mut back, 5.0, -3.0);
        assert_eq!(bbox(&back), (15.0, 17.0, 115.0, 77.0));
    }

    #[test]
    fn new_box_shapes_share_box_behavior_and_round_trip() {
        let b = BoxGeom {
            x: 1.0,
            y: 2.0,
            w: 30.0,
            h: 40.0,
            width: 2.0,
            rotation: 0.5,
        };
        // (serde tag, kind) — the tag is what gets persisted in JSON.
        let cases = [
            ("diamond", ElementKind::Diamond(b)),
            ("triangle", ElementKind::Triangle(b)),
            ("round_rect", ElementKind::RoundRect(b)),
            ("star", ElementKind::Star(b)),
            ("hexagon", ElementKind::Hexagon(b)),
        ];
        for (tag, kind) in cases {
            // Every new shape is a fillable closed shape, commits like a box, and
            // flows through the shared `box_like` path (bounds / select / resize /
            // rotate) just like rect/ellipse.
            assert!(is_closed_shape(&kind), "{tag} should be fillable");
            assert!(committable(&kind), "{tag} should commit");
            assert_eq!(
                box_like(&kind),
                Some((1.0, 2.0, 30.0, 40.0, 0.5)),
                "{tag} box_like"
            );
            // Round-trips through JSON under its snake_case tag.
            let elem = Element {
                id: 7,
                kind,
                stroke: None,
                fill: None,
                label: None,
                label_color: None,
                styles: Vec::new(),
                mindmap: None,
            };
            let json = serde_json::to_string(&elem).unwrap();
            assert!(json.contains(tag), "{tag} not in json: {json}");
            let back: Element = serde_json::from_str(&json).unwrap();
            assert_eq!(box_like(&back.kind), Some((1.0, 2.0, 30.0, 40.0, 0.5)));
        }
    }
