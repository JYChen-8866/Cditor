use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
    rc::Rc,
    sync::Arc,
};

use cditor_core::ids::SurfaceId;
use cditor_runtime::{MainThreadWorkKind, WorkCost};
use cditor_text::{SegmentedLayoutConfig, SegmentedTextLayout};
use gpui::{FontStyle, FontWeight};

use crate::{
    text::{RichTextLayoutInput, RichTextTypography},
    theme::GuiTheme,
};

const SEGMENT_CACHE_CAPACITY: usize = 16;

struct CachedSegmentedSurface {
    content_version: u64,
    layout_version: u64,
    theme_version: u64,
    font_version: u64,
    style_fingerprint: u64,
    text: Arc<str>,
    layout: Rc<RefCell<SegmentedTextLayout>>,
}

#[derive(Default)]
struct SegmentedSurfaceCache {
    entries: HashMap<SurfaceId, CachedSegmentedSurface>,
    lru: VecDeque<SurfaceId>,
}

thread_local! {
    static SEGMENTED_SURFACES: RefCell<SegmentedSurfaceCache> = RefCell::new(SegmentedSurfaceCache::default());
}

pub(super) fn cached_segmented_surface(
    input: &RichTextLayoutInput,
    style_fingerprint: u64,
) -> (Arc<str>, Rc<RefCell<SegmentedTextLayout>>) {
    SEGMENTED_SURFACES.with_borrow_mut(|cache| {
        let surface_id = input.surface_id;
        if cache
            .entries
            .get(&surface_id)
            .is_some_and(|entry| entry.content_version == input.content_version)
        {
            let entry = cache
                .entries
                .get_mut(&surface_id)
                .expect("entry was checked");
            let style_changed = entry.theme_version != input.theme_version
                || entry.font_version != input.font_version
                || entry.style_fingerprint != style_fingerprint;
            entry.layout_version = input.layout_version;
            entry.theme_version = input.theme_version;
            entry.font_version = input.font_version;
            entry.style_fingerprint = style_fingerprint;
            if style_changed {
                entry.layout.borrow_mut().invalidate_measurements();
            }
            let text = entry.text.clone();
            let layout = entry.layout.clone();
            touch_lru(cache, surface_id);
            return (text, layout);
        }

        let text = input.plain_text();
        let reusable = cache.entries.get(&surface_id).is_some_and(|entry| {
            entry.theme_version == input.theme_version && entry.font_version == input.font_version
        });
        if reusable {
            let entry = cache
                .entries
                .get_mut(&surface_id)
                .expect("entry was checked");
            if entry.text.as_ref() != text.as_ref() {
                let old = entry.text.as_ref();
                let (old_range, replacement) = changed_text_range(old, text.as_ref());
                entry
                    .layout
                    .borrow_mut()
                    .replace_range(old_range, replacement);
                entry.text = Arc::from(text.as_ref());
            }
            entry.content_version = input.content_version;
            entry.layout_version = input.layout_version;
            if entry.style_fingerprint != style_fingerprint {
                entry.layout.borrow_mut().invalidate_measurements();
                entry.style_fingerprint = style_fingerprint;
            }
        } else {
            let text = Arc::<str>::from(text.as_ref());
            cache.entries.insert(
                surface_id,
                CachedSegmentedSurface {
                    content_version: input.content_version,
                    layout_version: input.layout_version,
                    theme_version: input.theme_version,
                    font_version: input.font_version,
                    style_fingerprint,
                    layout: Rc::new(RefCell::new(SegmentedTextLayout::new(
                        text.as_ref(),
                        SegmentedLayoutConfig::default(),
                    ))),
                    text,
                },
            );
        }
        touch_lru(cache, surface_id);
        let entry = cache.entries.get(&surface_id).expect("entry was inserted");
        (entry.text.clone(), entry.layout.clone())
    })
}

pub(super) fn segmented_style_fingerprint(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    base_text_color: Option<u32>,
    typography: RichTextTypography,
    font_family: &str,
    font_weight: FontWeight,
    font_style: FontStyle,
    scale: f32,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    input.spans.cache_identity().hash(&mut hasher);
    input.kind.hash(&mut hasher);
    match input.text_align {
        cditor_core::rich_text::TextAlign::Start => 0u8,
        cditor_core::rich_text::TextAlign::Center => 1,
        cditor_core::rich_text::TextAlign::End => 2,
    }
    .hash(&mut hasher);
    theme.text.hash(&mut hasher);
    theme.quote_text.hash(&mut hasher);
    theme.muted.hash(&mut hasher);
    theme.code_text.hash(&mut hasher);
    theme.focused.hash(&mut hasher);
    theme.inline_code_text.hash(&mut hasher);
    theme.inline_code_background.hash(&mut hasher);
    base_text_color.hash(&mut hasher);
    font_family.hash(&mut hasher);
    font_weight.0.to_bits().hash(&mut hasher);
    match font_style {
        FontStyle::Normal => 0u8,
        FontStyle::Italic => 1,
        FontStyle::Oblique => 2,
    }
    .hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    typography.font_size_px.map(f32::to_bits).hash(&mut hasher);
    typography
        .line_height_px
        .map(f32::to_bits)
        .hash(&mut hasher);
    typography
        .font_weight
        .map(|weight| weight.0.to_bits())
        .hash(&mut hasher);
    hasher.finish()
}

pub(super) fn admitted_segments(
    layout: &SegmentedTextLayout,
    desired: &[usize],
    visible: Range<usize>,
    interaction_offsets: &[usize],
    composing: bool,
    mut admit: impl FnMut(MainThreadWorkKind, WorkCost) -> bool,
) -> Vec<usize> {
    let interaction_segments = interaction_offsets
        .iter()
        .filter_map(|offset| layout.segment_index_at_byte(*offset))
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for index in desired.iter().copied() {
        if layout.segment_snapshot(index).is_some() {
            continue;
        }
        let (kind, rank) = if interaction_segments.contains(&index) {
            if composing {
                (MainThreadWorkKind::CompositionCaret, 0)
            } else {
                (MainThreadWorkKind::EditingTextShape, 0)
            }
        } else if visible.contains(&index) {
            (MainThreadWorkKind::CurrentWindowMeasure, 1)
        } else {
            (MainThreadWorkKind::Prefetch, 2)
        };
        let bytes = layout
            .segment_layout_byte_range(index)
            .map_or(0, |range| range.len());
        let sync_ms = ((bytes as f64 / (64.0 * 1024.0)) * 4.0).clamp(0.2, 5.5);
        candidates.push((
            rank,
            index,
            kind,
            WorkCost {
                sync_ms,
                measure_applies: 1,
                ..WorkCost::ZERO
            },
        ));
    }
    candidates.sort_by_key(|(rank, index, _, _)| (*rank, *index));
    candidates
        .into_iter()
        .filter_map(|(_, index, kind, cost)| admit(kind, cost).then_some(index))
        .collect()
}

fn touch_lru(cache: &mut SegmentedSurfaceCache, surface_id: SurfaceId) {
    if let Some(index) = cache
        .lru
        .iter()
        .position(|candidate| *candidate == surface_id)
    {
        cache.lru.remove(index);
    }
    cache.lru.push_back(surface_id);
    while cache.entries.len() > SEGMENT_CACHE_CAPACITY {
        let Some(candidate) = cache.lru.pop_front() else {
            break;
        };
        cache.entries.remove(&candidate);
    }
}

pub(super) fn changed_text_range<'a>(old: &str, new: &'a str) -> (Range<usize>, &'a str) {
    let mut prefix = old
        .as_bytes()
        .iter()
        .zip(new.as_bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let max_suffix = old.len().min(new.len()).saturating_sub(prefix);
    let mut suffix = old
        .as_bytes()
        .iter()
        .rev()
        .zip(new.as_bytes().iter().rev())
        .take(max_suffix)
        .take_while(|(left, right)| left == right)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    (prefix..old.len() - suffix, &new[prefix..new.len() - suffix])
}

#[cfg(test)]
pub(super) fn remove_cached_segmented_surface_for_tests(surface_id: SurfaceId) {
    SEGMENTED_SURFACES.with_borrow_mut(|cache| {
        cache.entries.remove(&surface_id);
        cache.lru.retain(|candidate| *candidate != surface_id);
    });
}
