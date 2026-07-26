use std::collections::{HashMap, HashSet, VecDeque, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use cditor_core::rich_text::InlineSpan;

use super::{TextAlignment, TextLayoutOptions, TextLayoutSnapshot, build_text_layout};
use crate::{TextLayoutInput, TextLayoutSurfaceId, TextTheme};

const DEFAULT_LAYOUT_CACHE_MAX_ENTRIES: usize = 512;
const DEFAULT_LAYOUT_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;

thread_local! {
    static TEXT_LAYOUT_CACHE: std::cell::RefCell<TextLayoutCache> =
        std::cell::RefCell::new(TextLayoutCache::new(TextLayoutCachePolicy::default()));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextLayoutCachePolicy {
    pub max_entries: usize,
    pub max_estimated_bytes: usize,
}

impl TextLayoutCachePolicy {
    pub const fn new(max_entries: usize, max_estimated_bytes: usize) -> Self {
        Self {
            max_entries,
            max_estimated_bytes,
        }
    }

    fn normalized(self) -> Self {
        Self {
            max_entries: self.max_entries.max(1),
            max_estimated_bytes: self.max_estimated_bytes.max(1),
        }
    }
}

impl Default for TextLayoutCachePolicy {
    fn default() -> Self {
        Self::new(
            DEFAULT_LAYOUT_CACHE_MAX_ENTRIES,
            DEFAULT_LAYOUT_CACHE_MAX_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextLayoutCachePriority {
    Offscreen,
    Overscan,
    #[default]
    Visible,
    Editing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextLayoutCacheRequest {
    pub priority: TextLayoutCachePriority,
    pub pin_surface: bool,
}

impl TextLayoutCacheRequest {
    pub const fn visible() -> Self {
        Self {
            priority: TextLayoutCachePriority::Visible,
            pin_surface: false,
        }
    }

    pub const fn editing() -> Self {
        Self {
            priority: TextLayoutCachePriority::Editing,
            pin_surface: true,
        }
    }
}

impl Default for TextLayoutCacheRequest {
    fn default() -> Self {
        Self::visible()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextLayoutMemoryPressure {
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextLayoutCacheStats {
    pub entries: usize,
    pub estimated_bytes: usize,
    pub pinned_entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub reflows: u64,
    pub evictions: u64,
    pub over_budget_due_to_pins: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextLayoutCacheTrimReport {
    pub evicted_entries: usize,
    pub evicted_estimated_bytes: usize,
    pub remaining_entries: usize,
    pub remaining_estimated_bytes: usize,
    pub over_budget_due_to_pins: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextShapeKey {
    pub surface_id: TextLayoutSurfaceId,
    pub content_version: u64,
    pub theme_version: u64,
    pub font_version: u64,
    pub display_scale_bits: u32,
    pub text_fingerprint: u64,
    pub marks_fingerprint: u64,
    pub typography_fingerprint: u64,
    pub inline_objects_fingerprint: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextLayoutKey {
    pub shape: TextShapeKey,
    pub layout_version: u64,
    pub width_bits: Option<u32>,
    pub alignment: TextAlignment,
}

impl TextLayoutKey {
    pub fn from_input(input: &TextLayoutInput, options: &TextLayoutOptions) -> Self {
        Self {
            shape: TextShapeKey {
                surface_id: input.surface_id,
                content_version: input.content_version,
                theme_version: input.theme_version,
                font_version: input.font_version,
                display_scale_bits: options.display_scale.to_bits(),
                text_fingerprint: text_fingerprint(&input.spans),
                marks_fingerprint: marks_fingerprint(&input.spans),
                typography_fingerprint: typography_fingerprint(input, options),
                inline_objects_fingerprint: inline_objects_fingerprint(options),
            },
            layout_version: input.layout_version,
            width_bits: options.width.map(f32::to_bits),
            alignment: options.alignment,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRelayoutFallbackReason {
    NoPreviousSnapshot,
    ContentChanged,
    StyleChanged,
    InlineObjectsChanged,
    FontChanged,
    ScaleChanged,
    ShapeConfigurationChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRelayoutStrategy {
    CacheHit,
    Reflow,
    FullBuild(TextRelayoutFallbackReason),
}

#[derive(Debug, Clone)]
pub struct CachedTextLayout {
    pub key: TextLayoutKey,
    pub layout: TextLayoutSnapshot,
    pub cache_hit: bool,
    pub reflowed: bool,
    pub estimated_bytes: usize,
    pub strategy: TextRelayoutStrategy,
}

#[derive(Debug)]
struct TextLayoutCacheEntry {
    layout: TextLayoutSnapshot,
    estimated_bytes: usize,
    priority: TextLayoutCachePriority,
}

#[derive(Debug)]
struct TextLayoutCache {
    entries: HashMap<TextLayoutKey, TextLayoutCacheEntry>,
    order: VecDeque<TextLayoutKey>,
    policy: TextLayoutCachePolicy,
    automatic_pins: HashSet<TextLayoutSurfaceId>,
    explicit_pins: HashSet<TextLayoutSurfaceId>,
    estimated_bytes: usize,
    hits: u64,
    misses: u64,
    reflows: u64,
    evictions: u64,
}

impl TextLayoutCache {
    fn new(policy: TextLayoutCachePolicy) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            policy: policy.normalized(),
            automatic_pins: HashSet::new(),
            explicit_pins: HashSet::new(),
            estimated_bytes: 0,
            hits: 0,
            misses: 0,
            reflows: 0,
            evictions: 0,
        }
    }

    fn prepare_request(
        &mut self,
        surface_id: TextLayoutSurfaceId,
        request: TextLayoutCacheRequest,
    ) {
        if request.pin_surface {
            self.automatic_pins.insert(surface_id);
        } else {
            self.automatic_pins.remove(&surface_id);
        }
    }

    fn get(
        &mut self,
        key: &TextLayoutKey,
        priority: TextLayoutCachePriority,
    ) -> Option<TextLayoutSnapshot> {
        let entry = self.entries.get_mut(key)?;
        entry.priority = priority;
        let layout = entry.layout.clone();
        self.touch(key);
        Some(layout)
    }

    fn compatible_shape(&self, key: &TextLayoutKey) -> Option<TextLayoutSnapshot> {
        self.order.iter().rev().find_map(|candidate| {
            (candidate.shape == key.shape)
                .then(|| {
                    self.entries
                        .get(candidate)
                        .map(|entry| entry.layout.clone())
                })
                .flatten()
        })
    }

    fn full_build_reason(&self, key: &TextLayoutKey) -> TextRelayoutFallbackReason {
        let Some(previous) = self
            .order
            .iter()
            .rev()
            .find(|candidate| candidate.shape.surface_id == key.shape.surface_id)
        else {
            return TextRelayoutFallbackReason::NoPreviousSnapshot;
        };
        let previous = &previous.shape;
        let current = &key.shape;
        if previous.font_version != current.font_version {
            TextRelayoutFallbackReason::FontChanged
        } else if previous.display_scale_bits != current.display_scale_bits {
            TextRelayoutFallbackReason::ScaleChanged
        } else if previous.content_version != current.content_version
            || previous.text_fingerprint != current.text_fingerprint
        {
            TextRelayoutFallbackReason::ContentChanged
        } else if previous.inline_objects_fingerprint != current.inline_objects_fingerprint {
            TextRelayoutFallbackReason::InlineObjectsChanged
        } else if previous.theme_version != current.theme_version
            || previous.marks_fingerprint != current.marks_fingerprint
            || previous.typography_fingerprint != current.typography_fingerprint
        {
            TextRelayoutFallbackReason::StyleChanged
        } else {
            TextRelayoutFallbackReason::ShapeConfigurationChanged
        }
    }

    fn insert(
        &mut self,
        key: TextLayoutKey,
        layout: TextLayoutSnapshot,
        priority: TextLayoutCachePriority,
    ) {
        let estimated_bytes = layout.estimated_bytes().max(1);
        if let Some(previous) = self.entries.insert(
            key.clone(),
            TextLayoutCacheEntry {
                layout,
                estimated_bytes,
                priority,
            },
        ) {
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_sub(previous.estimated_bytes);
        }
        self.estimated_bytes = self.estimated_bytes.saturating_add(estimated_bytes);
        self.touch(&key);
        self.trim_to(self.policy.max_entries, self.policy.max_estimated_bytes);
    }

    fn touch(&mut self, key: &TextLayoutKey) {
        if let Some(index) = self.order.iter().position(|candidate| candidate == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.clone());
    }

    fn set_explicit_pin(&mut self, surface_id: TextLayoutSurfaceId, pinned: bool) {
        if pinned {
            self.explicit_pins.insert(surface_id);
        } else {
            self.explicit_pins.remove(&surface_id);
        }
    }

    fn replace_automatic_pins(&mut self, surface_ids: &[TextLayoutSurfaceId]) {
        self.automatic_pins.clear();
        self.automatic_pins.extend(surface_ids.iter().copied());
    }

    fn is_surface_pinned(&self, surface_id: TextLayoutSurfaceId) -> bool {
        self.automatic_pins.contains(&surface_id) || self.explicit_pins.contains(&surface_id)
    }

    fn apply_memory_pressure(
        &mut self,
        pressure: TextLayoutMemoryPressure,
    ) -> TextLayoutCacheTrimReport {
        match pressure {
            TextLayoutMemoryPressure::Normal => {
                self.trim_to(self.policy.max_entries, self.policy.max_estimated_bytes)
            }
            TextLayoutMemoryPressure::Warning => self.trim_to(
                self.policy.max_entries / 2,
                self.policy.max_estimated_bytes / 2,
            ),
            TextLayoutMemoryPressure::Critical => self.trim_to(0, 0),
        }
    }

    fn trim_to(
        &mut self,
        target_entries: usize,
        target_estimated_bytes: usize,
    ) -> TextLayoutCacheTrimReport {
        let mut report = TextLayoutCacheTrimReport::default();
        while self.entries.len() > target_entries || self.estimated_bytes > target_estimated_bytes {
            let Some(victim) = self.eviction_victim() else {
                break;
            };
            let Some(entry) = self.entries.remove(&victim) else {
                self.order.retain(|candidate| candidate != &victim);
                continue;
            };
            self.order.retain(|candidate| candidate != &victim);
            self.estimated_bytes = self.estimated_bytes.saturating_sub(entry.estimated_bytes);
            self.evictions = self.evictions.saturating_add(1);
            report.evicted_entries = report.evicted_entries.saturating_add(1);
            report.evicted_estimated_bytes = report
                .evicted_estimated_bytes
                .saturating_add(entry.estimated_bytes);
        }
        report.remaining_entries = self.entries.len();
        report.remaining_estimated_bytes = self.estimated_bytes;
        report.over_budget_due_to_pins =
            self.entries.len() > target_entries || self.estimated_bytes > target_estimated_bytes;
        report
    }

    fn eviction_victim(&self) -> Option<TextLayoutKey> {
        self.order
            .iter()
            .enumerate()
            .filter_map(|(age, key)| {
                let entry = self.entries.get(key)?;
                (!self.is_surface_pinned(key.shape.surface_id)).then_some((
                    entry.priority,
                    age,
                    key,
                ))
            })
            .min_by_key(|(priority, age, _)| (*priority, *age))
            .map(|(_, _, key)| key.clone())
    }

    fn stats(&self) -> TextLayoutCacheStats {
        let pinned_entries = self
            .entries
            .keys()
            .filter(|key| self.is_surface_pinned(key.shape.surface_id))
            .count();
        TextLayoutCacheStats {
            entries: self.entries.len(),
            estimated_bytes: self.estimated_bytes,
            pinned_entries,
            hits: self.hits,
            misses: self.misses,
            reflows: self.reflows,
            evictions: self.evictions,
            over_budget_due_to_pins: (self.entries.len() > self.policy.max_entries
                || self.estimated_bytes > self.policy.max_estimated_bytes)
                && pinned_entries == self.entries.len(),
        }
    }
}

pub fn cached_text_layout(
    input: &TextLayoutInput,
    theme: TextTheme,
    options: &TextLayoutOptions,
) -> CachedTextLayout {
    cached_text_layout_with_request(input, theme, options, TextLayoutCacheRequest::visible())
}

pub fn try_cached_text_layout_with_request(
    input: &TextLayoutInput,
    options: &TextLayoutOptions,
    request: TextLayoutCacheRequest,
) -> Option<CachedTextLayout> {
    let key = TextLayoutKey::from_input(input, options);
    TEXT_LAYOUT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.prepare_request(input.surface_id, request);
        let layout = cache.get(&key, request.priority)?;
        cache.hits = cache.hits.saturating_add(1);
        let estimated_bytes = layout.estimated_bytes();
        Some(CachedTextLayout {
            key,
            layout,
            cache_hit: true,
            reflowed: false,
            estimated_bytes,
            strategy: TextRelayoutStrategy::CacheHit,
        })
    })
}

/// Returns the newest snapshot with the same shaped text/style identity without
/// reflowing it to the requested width. This is a paintable one-frame fallback
/// while a scheduler-admitted reflow is pending.
pub fn try_compatible_text_layout_with_request(
    input: &TextLayoutInput,
    options: &TextLayoutOptions,
    request: TextLayoutCacheRequest,
) -> Option<CachedTextLayout> {
    let key = TextLayoutKey::from_input(input, options);
    TEXT_LAYOUT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.prepare_request(input.surface_id, request);
        let layout = cache.compatible_shape(&key)?;
        let estimated_bytes = layout.estimated_bytes();
        Some(CachedTextLayout {
            key,
            layout,
            cache_hit: true,
            reflowed: false,
            estimated_bytes,
            strategy: TextRelayoutStrategy::CacheHit,
        })
    })
}

pub fn cached_text_layout_with_request(
    input: &TextLayoutInput,
    theme: TextTheme,
    options: &TextLayoutOptions,
    request: TextLayoutCacheRequest,
) -> CachedTextLayout {
    let key = TextLayoutKey::from_input(input, options);
    TEXT_LAYOUT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.prepare_request(input.surface_id, request);
        if let Some(layout) = cache.get(&key, request.priority) {
            cache.hits = cache.hits.saturating_add(1);
            let estimated_bytes = layout.estimated_bytes();
            return CachedTextLayout {
                key,
                layout,
                cache_hit: true,
                reflowed: false,
                estimated_bytes,
                strategy: TextRelayoutStrategy::CacheHit,
            };
        }

        cache.misses = cache.misses.saturating_add(1);
        let (layout, strategy) = if let Some(layout) = cache.compatible_shape(&key) {
            cache.reflows = cache.reflows.saturating_add(1);
            (
                layout.reflow(options.width, options.alignment),
                TextRelayoutStrategy::Reflow,
            )
        } else {
            let reason = cache.full_build_reason(&key);
            (
                build_text_layout(input, theme, options),
                TextRelayoutStrategy::FullBuild(reason),
            )
        };
        let estimated_bytes = layout.estimated_bytes();
        cache.insert(key.clone(), layout.clone(), request.priority);
        CachedTextLayout {
            key,
            layout,
            cache_hit: false,
            reflowed: strategy == TextRelayoutStrategy::Reflow,
            estimated_bytes,
            strategy,
        }
    })
}

pub fn set_text_layout_cache_policy(policy: TextLayoutCachePolicy) -> TextLayoutCacheTrimReport {
    TEXT_LAYOUT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.policy = policy.normalized();
        let max_entries = cache.policy.max_entries;
        let max_estimated_bytes = cache.policy.max_estimated_bytes;
        cache.trim_to(max_entries, max_estimated_bytes)
    })
}

pub fn set_text_layout_surface_pin(surface_id: TextLayoutSurfaceId, pinned: bool) {
    TEXT_LAYOUT_CACHE.with(|cache| cache.borrow_mut().set_explicit_pin(surface_id, pinned));
}

pub fn sync_automatic_text_layout_pins(surface_ids: &[TextLayoutSurfaceId]) {
    TEXT_LAYOUT_CACHE.with(|cache| {
        cache.borrow_mut().replace_automatic_pins(surface_ids);
    });
}

pub fn apply_text_layout_memory_pressure(
    pressure: TextLayoutMemoryPressure,
) -> TextLayoutCacheTrimReport {
    TEXT_LAYOUT_CACHE.with(|cache| cache.borrow_mut().apply_memory_pressure(pressure))
}

pub fn text_layout_cache_stats() -> TextLayoutCacheStats {
    TEXT_LAYOUT_CACHE.with(|cache| cache.borrow().stats())
}

fn text_fingerprint(spans: &[InlineSpan]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for span in spans {
        span.text.hash(&mut hasher);
    }
    hasher.finish()
}

fn marks_fingerprint(spans: &[InlineSpan]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for span in spans {
        span.marks.hash(&mut hasher);
    }
    hasher.finish()
}

fn typography_fingerprint(input: &TextLayoutInput, options: &TextLayoutOptions) -> u64 {
    let mut hasher = DefaultHasher::new();
    format!("{:?}", input.kind).hash(&mut hasher);
    options.quantize.hash(&mut hasher);
    options.base_text_color.hash(&mut hasher);
    options.mono_font_family.hash(&mut hasher);
    format!("{:?}", options.base_style).hash(&mut hasher);
    options.text_indent.amount.to_bits().hash(&mut hasher);
    options.text_indent.each_line.hash(&mut hasher);
    options.text_indent.hanging.hash(&mut hasher);
    hasher.finish()
}

fn inline_objects_fingerprint(options: &TextLayoutOptions) -> u64 {
    let mut hasher = DefaultHasher::new();
    for inline_box in &options.inline_boxes {
        inline_box.id.hash(&mut hasher);
        format!("{:?}", inline_box.kind).hash(&mut hasher);
        inline_box.index.hash(&mut hasher);
        inline_box.width.to_bits().hash(&mut hasher);
        inline_box.height.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

pub(crate) fn clear_text_layout_cache() {
    TEXT_LAYOUT_CACHE.with(|cache| {
        *cache.borrow_mut() = TextLayoutCache::new(TextLayoutCachePolicy::default());
    });
}

#[cfg(test)]
pub(super) fn reset_text_layout_cache_for_tests() {
    clear_text_layout_cache();
}

#[cfg(test)]
mod tests;
