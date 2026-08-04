use std::ops::Range;

use cditor_core::edit::TextAffinity;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, InlineMark, InlineSpan};
use gpui::{App, Entity, MouseButton, MouseDownEvent, Pixels, Point, Window};
use url::Url;

use crate::editor_view::CditorV2View;

pub fn open_link_from_mouse(
    view: &Entity<CditorV2View>,
    block_id: BlockId,
    event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    if !is_link_activation(event.button, event.modifiers.secondary()) {
        return false;
    }

    let (href, host_opener) = {
        let view = view.read(cx);
        (
            view.link_at_block_position(block_id, event.position),
            view.features.link_opener.clone(),
        )
    };
    let Some(href) = href else {
        return false;
    };
    open_link_href(href, host_opener, window, cx)
}

fn open_link_href(
    href: String,
    host_opener: Option<std::sync::Arc<dyn Fn(&str, &mut Window, &mut App) -> bool>>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    if host_opener.is_some_and(|opener| opener(&href, window, cx)) {
        return true;
    }
    let Some(url) = openable_external_url(&href) else {
        return false;
    };

    cx.open_url(&url);
    true
}

impl CditorV2View {
    fn link_at_block_position(&self, block_id: BlockId, position: Point<Pixels>) -> Option<String> {
        let text_position = self.text_position_for_block_at_position(block_id, position)?;
        let payload = self
            .ready_session()?
            .loaded_payload_record(block_id)
            .ok()
            .flatten()?;
        let BlockPayload::RichText { spans } = &payload.payload else {
            return None;
        };
        let link = link_at_text_position(spans, text_position)?;
        if !self.text_range_contains_block_position(block_id, link.range, position) {
            return None;
        }
        safe_link_href(link.href)
    }
}

fn is_link_activation(button: MouseButton, secondary_modifier: bool) -> bool {
    button == MouseButton::Left && secondary_modifier
}

struct LinkSpanHit<'a> {
    href: &'a str,
    range: Range<usize>,
}

fn link_at_text_position(
    spans: &[InlineSpan],
    position: crate::text::TextLayoutPosition,
) -> Option<LinkSpanHit<'_>> {
    let offset = match position.affinity {
        TextAffinity::Downstream => position.offset,
        TextAffinity::Upstream => position.offset.checked_sub(1)?,
    };
    let mut span_start = 0usize;
    for span in spans {
        let span_end = span_start.checked_add(span.text.len())?;
        if (span_start..span_end).contains(&offset) {
            return span.marks.iter().find_map(|mark| {
                let href = match mark {
                    InlineMark::Link { href } | InlineMark::DocumentLink { href } => href,
                    _ => return None,
                };
                Some(LinkSpanHit {
                    href,
                    range: span_start..span_end,
                })
            });
        }
        span_start = span_end;
    }
    None
}

fn safe_link_href(href: &str) -> Option<String> {
    if href.is_empty() || href.trim() != href || href.chars().any(char::is_control) {
        return None;
    }
    Some(href.to_owned())
}

fn openable_external_url(href: &str) -> Option<String> {
    let href = safe_link_href(href)?;
    let parsed = Url::parse(&href).ok()?;
    match parsed.scheme() {
        "http" | "https" if parsed.host_str().is_some() => Some(parsed.into()),
        "mailto" | "tel" if !parsed.path().is_empty() => Some(parsed.into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::ids::SurfaceId;
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, TextAlign};
    use cditor_runtime::DocumentRuntime;
    use gpui::Modifiers;

    use super::*;
    use crate::interaction::geometry::{DocumentViewportOrigin, ProjectedBlockRect};

    fn span(text: &str, marks: Vec<InlineMark>) -> InlineSpan {
        InlineSpan {
            text: text.to_owned(),
            marks,
        }
    }

    fn link(text: &str, href: &str) -> InlineSpan {
        span(
            text,
            vec![InlineMark::Link {
                href: href.to_owned(),
            }],
        )
    }

    #[test]
    fn link_activation_requires_left_button_and_platform_secondary_modifier() {
        assert!(is_link_activation(
            MouseButton::Left,
            Modifiers::secondary_key().secondary()
        ));
        assert!(!is_link_activation(MouseButton::Left, false));
        assert!(!is_link_activation(MouseButton::Right, true));
        assert!(!is_link_activation(MouseButton::Middle, true));
    }

    #[test]
    fn link_lookup_uses_utf8_byte_offsets_and_respects_span_boundaries() {
        let spans = vec![
            span("prefix ", vec![]),
            link("\u{94fe}\u{63a5}", "https://example.com"),
            span(" suffix", vec![]),
        ];
        let start = "prefix ".len();
        let end = start + "\u{94fe}\u{63a5}".len();

        assert_eq!(
            link_at_text_position(&spans, crate::text::TextLayoutPosition::downstream(start))
                .map(|hit| (hit.href, hit.range)),
            Some(("https://example.com", start..end))
        );
        assert_eq!(
            link_at_text_position(
                &spans,
                crate::text::TextLayoutPosition::downstream(start + "\u{94fe}".len())
            )
            .map(|hit| hit.href),
            Some("https://example.com")
        );
        assert!(
            link_at_text_position(&spans, crate::text::TextLayoutPosition::downstream(end))
                .is_none()
        );
        assert_eq!(
            link_at_text_position(
                &spans,
                crate::text::TextLayoutPosition {
                    offset: end,
                    affinity: TextAffinity::Upstream,
                }
            )
            .map(|hit| hit.href),
            Some("https://example.com")
        );
        assert!(
            link_at_text_position(&spans, crate::text::TextLayoutPosition::downstream(0)).is_none()
        );
    }

    #[test]
    fn link_lookup_extracts_link_among_other_marks() {
        let spans = vec![span(
            "Aurin",
            vec![
                InlineMark::Bold,
                InlineMark::Link {
                    href: "https://aurin.example/docs".to_owned(),
                },
                InlineMark::Underline,
            ],
        )];

        assert_eq!(
            link_at_text_position(&spans, crate::text::TextLayoutPosition::downstream(2))
                .map(|hit| hit.href),
            Some("https://aurin.example/docs")
        );
        assert!(
            link_at_text_position(
                &spans,
                crate::text::TextLayoutPosition::downstream("Aurin".len())
            )
            .is_none()
        );
    }

    #[test]
    fn openable_external_url_accepts_only_explicit_external_protocols() {
        for href in [
            "https://example.com/path?q=1#part",
            "http://localhost:8080/docs",
            "mailto:hello@example.com",
            "tel:+8613800138000",
        ] {
            assert_eq!(openable_external_url(href).as_deref(), Some(href));
        }

        for href in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            " javascript:alert(1)",
            "vbscript:msgbox(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "/relative/path",
            "https://",
            "https://example.com\0.test",
            "https://example.com\njavascript:alert(1)",
            "",
        ] {
            assert_eq!(
                openable_external_url(href),
                None,
                "unexpectedly accepted {href:?}"
            );
        }
    }

    #[gpui::test]
    fn host_opener_handles_internal_links_before_the_system_browser(cx: &mut gpui::TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_window, cx| {
            CditorV2View::from_runtime(DocumentRuntime::empty(), false, cx)
        });
        let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = opened.clone();
        let opener = std::sync::Arc::new(move |href: &str, _window: &mut Window, _cx: &mut App| {
            captured.lock().unwrap().push(href.to_owned());
            true
        });

        assert!(cx.update(|window, cx| open_link_href(
            "aurin://doc/node/content/block/1".to_owned(),
            Some(opener),
            window,
            cx,
        )));
        assert_eq!(
            opened.lock().unwrap().as_slice(),
            ["aurin://doc/node/content/block/1"]
        );
        assert_eq!(cx.opened_url(), None);
    }

    #[gpui::test]
    fn command_click_opens_link_but_not_adjacent_blank_space(cx: &mut gpui::TestAppContext) {
        let text = "prefix Aurin";
        let link_start = "prefix ".len();
        let payload = BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![
                    InlineSpan::plain("prefix "),
                    link("Aurin", "https://aurin.example/docs"),
                ],
            },
        };
        let runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);
        let (view, cx) =
            cx.add_window_view(|_window, cx| CditorV2View::from_runtime(runtime, false, cx));

        let (link_point, blank_point) = view.update(cx, |view, _cx| {
            view.interaction.document_viewport_origin =
                Some(DocumentViewportOrigin { x: 100.0, y: 40.0 });
            view.interaction.projected_block_rects = vec![ProjectedBlockRect {
                block_id: 1,
                document_top: 120.0,
                document_bottom: 180.0,
                text_origin_x_in_block_px: 32.0,
                text_origin_y_in_block_px: 12.0,
                text_width_px: 300.0,
                text_align: Some(TextAlign::Start),
                ..ProjectedBlockRect::default()
            }];
            let current = view
                .ready_session()
                .unwrap()
                .surface_version(SurfaceId::Block(1))
                .unwrap()
                .unwrap();
            let mut cache = crate::text::test_platform_layout(
                1,
                current.content_version,
                text,
                gpui::Bounds::new(
                    gpui::point(gpui::px(0.0), gpui::px(0.0)),
                    gpui::size(gpui::px(300.0), gpui::px(120.0)),
                ),
                None,
            );
            cache.layout_version = current.layout_version;
            let link_rect = cache
                .snapshot
                .range_rects(link_start..text.len())
                .into_iter()
                .next()
                .unwrap();
            view.cache.text_layouts.insert(1, cache, None);
            let placement = view.projected_text_placement_for_block(1).unwrap();
            let at = |x: f32| {
                gpui::point(
                    gpui::px((placement.window_origin_x_px + f64::from(x)) as f32),
                    gpui::px(
                        (placement.window_origin_y_px
                            + f64::from(link_rect.y + link_rect.height / 2.0))
                            as f32,
                    ),
                )
            };
            (
                at(link_rect.x + link_rect.width / 2.0),
                at(link_rect.x + link_rect.width + 20.0),
            )
        });

        let event = |position, modifiers| MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count: 1,
            first_mouse: false,
        };
        assert!(!cx.update(|window, cx| open_link_from_mouse(
            &view,
            1,
            &event(link_point, Modifiers::none()),
            window,
            cx,
        )));
        assert!(!cx.update(|window, cx| open_link_from_mouse(
            &view,
            1,
            &event(blank_point, Modifiers::secondary_key()),
            window,
            cx,
        )));
        assert_eq!(cx.opened_url(), None);

        assert!(cx.update(|window, cx| open_link_from_mouse(
            &view,
            1,
            &event(link_point, Modifiers::secondary_key()),
            window,
            cx,
        )));
        assert_eq!(
            cx.opened_url(),
            Some("https://aurin.example/docs".to_owned())
        );
    }
}
