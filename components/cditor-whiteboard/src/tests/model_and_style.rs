    #[test]
    fn text_layout_cache_reuses_outlines_until_content_changes() {
        let font = Font::default();
        let mut cache = HashMap::new();
        let first = cached_text_layout(&mut cache, &font, 9, "hello", 16.0, None, &[]);
        let reused = cached_text_layout(&mut cache, &font, 9, "hello", 16.0, None, &[]);
        let changed = cached_text_layout(&mut cache, &font, 9, "hello!", 16.0, None, &[]);

        assert!(Arc::ptr_eq(&first.segs, &reused.segs));
        assert!(!Arc::ptr_eq(&first.segs, &changed.segs));
    }
    #[test]
    fn shape_label_cache_survives_movement_and_invalidates_on_resize() {
        let font = Font::default();
        let mut cache = HashMap::new();
        let kind = ElementKind::RoundRect(BoxGeom {
            x: 10.0,
            y: 20.0,
            w: 180.0,
            h: 60.0,
            width: 2.0,
            rotation: 0.0,
        });
        let first = cached_label_layout(
            &mut cache,
            &font,
            3,
            &kind,
            LabelBox {
                x: 10.0,
                y: 20.0,
                w: 180.0,
                h: 60.0,
            },
            "Node",
            &[],
        );
        let moved = cached_label_layout(
            &mut cache,
            &font,
            3,
            &kind,
            LabelBox {
                x: 80.0,
                y: 90.0,
                w: 180.0,
                h: 60.0,
            },
            "Node",
            &[],
        );
        let resized = cached_label_layout(
            &mut cache,
            &font,
            3,
            &kind,
            LabelBox {
                x: 80.0,
                y: 90.0,
                w: 240.0,
                h: 80.0,
            },
            "Node",
            &[],
        );

        assert!(Arc::ptr_eq(&first.text.segs, &moved.text.segs));
        assert!(!Arc::ptr_eq(&first.text.segs, &resized.text.segs));
    }

    #[test]
    fn read_only_embed_does_not_accept_wheel_input() {
        assert!(!accepts_wheel_input(true));
        assert!(accepts_wheel_input(false));
    }

    #[test]
    fn empty_or_garbage_loads_a_blank_board() {
        for s in ["", "   ", "not json", "{}", r#"{"camera":{"zoom":0}}"#] {
            let scene = Scene::from_json(s);
            assert_eq!(scene.camera.zoom, 1.0, "input {s:?}");
            assert!(scene.elements.is_empty(), "input {s:?}");
        }
    }

    #[test]
    fn ime_offsets_bridge_utf16_and_utf8() {
        // Chinese uses one UTF-16 code unit but three UTF-8 bytes; emoji uses a
        // surrogate pair. These are the offsets GPUI's native IME APIs exchange.
        let text = "A中😀B";
        let utf8_boundaries = [0, 1, 4, 8, 9];
        let utf16_boundaries = [0, 1, 2, 4, 5];
        for (utf8, utf16) in utf8_boundaries.into_iter().zip(utf16_boundaries) {
            assert_eq!(WhiteboardView::utf8_to_utf16_in(text, utf8), utf16);
            assert_eq!(WhiteboardView::utf16_to_utf8_in(text, utf16), utf8);
        }

        // Like the editor bridge, offsets inside a code point advance to the next
        // valid boundary, so slicing the scene's UTF-8 string remains safe.
        assert_eq!(WhiteboardView::utf8_to_utf16_in(text, 3), 2);
        assert_eq!(WhiteboardView::utf16_to_utf8_in(text, 3), 8);
    }

    #[test]
    fn camera_round_trips_through_json() {
        let scene = Scene {
            camera: Camera {
                x: 12.5,
                y: -4.0,
                zoom: 2.0,
            },
            ..Default::default()
        };
        let restored = Scene::from_json(&scene.to_json());
        assert_eq!(restored.camera.x, 12.5);
        assert_eq!(restored.camera.zoom, 2.0);
    }

    #[test]
    fn all_content_thumbnail_snapshot_uses_scene_bounds_without_mounting_view() {
        let scene = Scene {
            elements: vec![Element {
                id: 1,
                kind: ElementKind::Rect(BoxGeom {
                    x: 10.0,
                    y: 20.0,
                    w: 100.0,
                    h: 60.0,
                    width: 2.0,
                    rotation: 0.0,
                }),
                stroke: None,
                fill: None,
                label: None,
                label_color: None,
                styles: Vec::new(),
                mindmap: None,
            }],
            ..Scene::default()
        };

        let snapshot = LocalThumbnailSnapshot::for_scene_all_content(scene, 320.0, 180.0);

        assert_eq!(snapshot.spec.scene_bounds, Some([10.0, 20.0, 110.0, 80.0]));
        assert_eq!(snapshot.spec.focus_bounds, [10.0, 20.0, 110.0, 80.0]);
        assert!(snapshot.spec.camera.zoom > 0.0);
    }

    #[test]
    fn empty_scene_still_builds_a_renderable_thumbnail_snapshot() {
        let snapshot =
            LocalThumbnailSnapshot::for_scene_all_content(Scene::default(), 320.0, 180.0);

        assert_eq!(snapshot.spec.scene_bounds, None);
        assert_eq!(snapshot.spec.focus_bounds, [0.0, 0.0, 320.0, 180.0]);
        assert_eq!(snapshot.spec.camera.zoom, 1.0);
    }

    #[test]
    fn every_element_kind_round_trips_through_json() {
        let scene = Scene {
            camera: Camera::default(),
            elements: vec![
                Element {
                    id: 1,
                    kind: ElementKind::Draw(Stroke {
                        points: vec![[0.0, 0.0], [10.0, 5.0]],
                        width: 3.0,
                    }),
                    stroke: None,
                    fill: None,
                    label: None,
                    label_color: None,
                    styles: Vec::new(),
                    mindmap: None,
                },
                Element {
                    id: 2,
                    kind: ElementKind::Rect(BoxGeom {
                        x: 1.0,
                        y: 2.0,
                        w: 30.0,
                        h: 40.0,
                        width: 2.0,
                        rotation: 0.0,
                    }),
                    stroke: Some(0xff0000ff),
                    fill: Some(0x00ff0080),
                    label: Some("hi".into()),
                    label_color: Some(0x112233ff),
                    styles: Vec::new(),
                    mindmap: None,
                },
                Element {
                    id: 3,
                    kind: ElementKind::Arrow(SegGeom {
                        x1: 1.0,
                        y1: 1.0,
                        x2: 2.0,
                        y2: 8.0,
                        width: 2.5,
                        style: SegmentStyle::Solid,
                        start_anchor: None,
                        end_anchor: None,
                    }),
                    stroke: None,
                    fill: None,
                    label: None,
                    label_color: None,
                    styles: Vec::new(),
                    mindmap: None,
                },
            ],
        };
        let restored = Scene::from_json(&scene.to_json());
        assert_eq!(restored.elements.len(), 3);
        match &restored.elements[2].kind {
            ElementKind::Arrow(s) => assert_eq!(s.y2, 8.0),
            other => panic!("expected arrow, got {other:?}"),
        }
        // Per-element color round-trips; an uncolored element stays `None`.
        assert_eq!(restored.elements[1].stroke, Some(0xff0000ff));
        assert_eq!(restored.elements[1].fill, Some(0x00ff0080));
        // The shape label + its color round-trip too.
        assert_eq!(restored.elements[1].label.as_deref(), Some("hi"));
        assert_eq!(restored.elements[1].label_color, Some(0x112233ff));
        assert_eq!(restored.elements[0].stroke, None);
        assert_eq!(restored.elements[0].fill, None);
    }

    #[test]
    fn label_defaults_to_none_for_older_boards() {
        // A board saved before labels existed has no `label` key → `None`, and an
        // unlabeled element never writes the key back.
        let old = r#"{"id":2,"kind":{"rect":{"x":0.0,"y":0.0,"w":1.0,"h":1.0,"width":1.0}}}"#;
        let back: Element = serde_json::from_str(old).unwrap();
        assert_eq!(back.label, None);
        assert!(!serde_json::to_string(&back).unwrap().contains("label"));
    }

    #[test]
    fn shape_label_block_fits_inscribed_region() {
        let font = Font::default();
        let bg = BoxGeom {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            width: 0.0,
            rotation: 0.0,
        };
        let (bx, by, bw, bh) = (10.0, 20.0, 200.0, 120.0);

        // Rect: default size for a roomy box; wraps to ~the full padded width.
        let rect = shape_label_block(&font, &ElementKind::Rect(bg), bx, by, bw, bh, "hello world");
        assert!(rect.size <= TEXT_SIZE + 0.01, "size {}", rect.size);
        assert!(
            (rect.wrap - (bw - 2.0 * LABEL_PAD)).abs() < 0.5,
            "rect wraps full width: {}",
            rect.wrap
        );

        // Diamond: wraps to the central inscribed rectangle (½ width), so it wraps
        // narrower and shrinks at least as much as the rect.
        let dia = shape_label_block(
            &font,
            &ElementKind::Diamond(bg),
            bx,
            by,
            bw,
            bh,
            "hello world",
        );
        assert!(
            (dia.wrap - (bw * 0.5 - 2.0 * LABEL_PAD)).abs() < 0.5,
            "diamond half width: {}",
            dia.wrap
        );
        assert!(
            dia.size <= rect.size,
            "diamond shrinks ≥ rect: {} vs {}",
            dia.size,
            rect.size
        );

        // Triangle: its band sits in the lower half (anchored near the base).
        let tri = shape_label_block(
            &font,
            &ElementKind::Triangle(bg),
            bx,
            by,
            bw,
            bh,
            "hello world",
        );
        assert!(
            tri.y >= by + bh / 2.0 - 0.5,
            "triangle label low: y={}",
            tri.y
        );

        // A long label in a small box shrinks below the default to avoid overflow.
        let tiny = shape_label_block(
            &font,
            &ElementKind::Rect(bg),
            0.0,
            0.0,
            44.0,
            28.0,
            "a long label that must shrink",
        );
        assert!(tiny.size < TEXT_SIZE, "shrinks: {}", tiny.size);
    }

    #[test]
    fn style_span_toggle_and_layer() {
        let bold = RunStyle {
            bold: true,
            ..Default::default()
        };
        let s = toggle_format(&[], 0, 4, Format::Bold);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].start, s[0].end, s[0].style), (0, 4, bold));
        assert!(style_at(&s, 2).bold && !style_at(&s, 4).bold);
        // Toggling the same range off clears it.
        assert!(toggle_format(&s, 0, 4, Format::Bold).is_empty());
        // Extending past the run (partly unstyled) adds, merging into one run.
        let s2 = toggle_format(&s, 2, 6, Format::Bold);
        assert_eq!((s2.len(), s2[0].start, s2[0].end), (1, 0, 6));
        // Layering italic over part of the bold run yields three runs.
        let s3 = toggle_format(&s, 1, 3, Format::Italic);
        assert_eq!(s3.len(), 3, "{s3:?}");
        assert!(style_at(&s3, 1).bold && style_at(&s3, 1).italic);
        assert!(style_at(&s3, 0).bold && !style_at(&s3, 0).italic);
        // Highlight toggles its color independently.
        let h = toggle_highlight(&[], 0, 3, 0xffff00ff);
        assert_eq!(style_at(&h, 1).highlight, Some(0xffff00ff));
        assert!(toggle_highlight(&h, 0, 3, 0xffff00ff).is_empty());
    }

    #[test]
    fn active_style_reports_common_formatting() {
        let s = toggle_format(&[], 2, 5, Format::Bold); // bytes 2..5 bold
        assert!(active_style(&s, 2, 5).bold, "whole selection bold");
        assert!(
            !active_style(&s, 0, 5).bold,
            "selection spills onto plain text"
        );
        // Collapsed caret inherits the char to its left.
        assert!(
            active_style(&s, 5, 5).bold,
            "just after the run inherits bold"
        );
        assert!(!active_style(&s, 2, 2).bold, "just before it is plain");
    }

    #[test]
    fn splice_keeps_runs_aligned() {
        let plain = RunStyle::default();
        let s = toggle_format(&[], 2, 5, Format::Bold);
        // Insert two chars at the start → the run shifts right.
        let a = splice_styles(&s, 0, 0, 2, plain);
        assert_eq!((a[0].start, a[0].end), (4, 7));
        // Delete a char inside the run → it shrinks by one.
        let b = splice_styles(&s, 3, 4, 0, plain);
        assert_eq!((b[0].start, b[0].end), (2, 4), "{b:?}");
        // Replacing a middle slice of a bold run with plain text splits it.
        let full = toggle_format(&[], 0, 6, Format::Bold);
        let c = splice_styles(&full, 2, 4, 2, plain);
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(
            ((c[0].start, c[0].end), (c[1].start, c[1].end)),
            ((0, 2), (4, 6))
        );
    }

    #[test]
    fn styles_round_trip_and_back_compat() {
        let el = Element {
            id: 1,
            kind: ElementKind::Text(TextGeom {
                x: 0.0,
                y: 0.0,
                content: "hello world".into(),
                size: 12.0,
                rotation: 0.0,
                measured_w: 0.0,
                measured_h: 0.0,
            }),
            stroke: None,
            fill: None,
            label: None,
            label_color: None,
            styles: vec![StyleSpan {
                start: 0,
                end: 5,
                style: RunStyle {
                    bold: true,
                    highlight: Some(0xffff00ff),
                    ..Default::default()
                },
            }],
            mindmap: None,
        };
        let back: Element = serde_json::from_str(&serde_json::to_string(&el).unwrap()).unwrap();
        assert_eq!(back.styles.len(), 1);
        assert!(back.styles[0].style.bold);
        assert_eq!(back.styles[0].style.highlight, Some(0xffff00ff));
        // A board saved before rich text loads with no styles.
        let old = r#"{"id":2,"kind":{"rect":{"x":0.0,"y":0.0,"w":1.0,"h":1.0,"width":1.0}}}"#;
        assert!(
            serde_json::from_str::<Element>(old)
                .unwrap()
                .styles
                .is_empty()
        );
    }
