use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};

use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayloadView, RichBlockKind};
use cditor_runtime::EditorViewProjection;
use gpui::{AppContext, Context, RenderImage, Task};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;

use super::theme::build_mermaid_theme;

const MAX_MERMAID_SOURCE_BYTES: usize = 256 * 1024;

type RenderResult = Result<Arc<RenderImage>, Arc<str>>;

#[derive(Clone)]
pub(crate) enum MermaidRenderStatus {
    Ready(Arc<RenderImage>),
    Rendering { fallback: Option<Arc<RenderImage>> },
    Failed { message: Arc<str> },
}

struct MermaidRenderEntry {
    content_version: u64,
    source_hash: u64,
    theme: GuiTheme,
    result: Arc<OnceLock<RenderResult>>,
    fallback: Option<Arc<RenderImage>>,
    _task: Option<Task<()>>,
}

impl MermaidRenderEntry {
    fn new(
        content_version: u64,
        source_hash: u64,
        source: String,
        theme: GuiTheme,
        fallback: Option<Arc<RenderImage>>,
        cx: &mut Context<CditorV2View>,
    ) -> Self {
        let result = Arc::new(OnceLock::new());
        if let Err(message) = validate_source(&source) {
            let _ = result.set(Err(message.into()));
            return Self {
                content_version,
                source_hash,
                theme,
                result,
                fallback,
                _task: None,
            };
        }

        let result_for_task = result.clone();
        let renderer = cx.svg_renderer();
        let render_theme = build_mermaid_theme(theme);
        let task = cx.spawn(async move |view, cx| {
            let rendered = cx
                .background_spawn(async move {
                    let svg = mermaid_render::render_to_svg(&source, &render_theme)
                        .map_err(|error| Arc::<str>::from(format!("{error:#}")))?;
                    renderer
                        .render_single_frame(svg.as_bytes(), 1.0)
                        .map_err(|error| Arc::<str>::from(error.to_string()))
                })
                .await;
            let _ = result_for_task.set(rendered);
            let _ = view.update(cx, |_view, cx| cx.notify());
        });

        Self {
            content_version,
            source_hash,
            theme,
            result,
            fallback,
            _task: Some(task),
        }
    }

    fn status(&self) -> MermaidRenderStatus {
        match self.result.get() {
            Some(Ok(image)) => MermaidRenderStatus::Ready(image.clone()),
            Some(Err(message)) => MermaidRenderStatus::Failed {
                message: message.clone(),
            },
            None => MermaidRenderStatus::Rendering {
                fallback: self.fallback.clone(),
            },
        }
    }

    pub(crate) fn best_image(&self) -> Option<Arc<RenderImage>> {
        match self.result.get() {
            Some(Ok(image)) => Some(image.clone()),
            _ => self.fallback.clone(),
        }
    }

    fn matches(&self, content_version: u64, source_hash: u64, theme: GuiTheme) -> bool {
        self.content_version == content_version
            && self.source_hash == source_hash
            && self.theme == theme
    }
}

#[derive(Default)]
pub(crate) struct MermaidRenderCache {
    entries: HashMap<BlockId, MermaidRenderEntry>,
}

impl MermaidRenderCache {
    pub(crate) fn sync_visible_window(
        &mut self,
        projection: &EditorViewProjection,
        theme: GuiTheme,
        cx: &mut Context<CditorV2View>,
    ) {
        let visible = projection
            .blocks
            .iter()
            .filter(|block| matches!(block.kind, RichBlockKind::Mermaid))
            .filter_map(|block| {
                let BlockPayloadView::Loaded(payload) = &block.payload else {
                    return None;
                };
                let source = payload.plain_text();
                Some((
                    block.block_id,
                    payload.content_version,
                    source_hash(&source),
                    source,
                ))
            })
            .collect::<Vec<_>>();
        let visible_ids = visible
            .iter()
            .map(|(block_id, _, _, _)| *block_id)
            .collect::<HashSet<_>>();
        self.entries
            .retain(|block_id, _| visible_ids.contains(block_id));

        for (block_id, content_version, hash, source) in visible {
            if self
                .entries
                .get(&block_id)
                .is_some_and(|entry| entry.matches(content_version, hash, theme))
            {
                continue;
            }
            let fallback = self
                .entries
                .remove(&block_id)
                .and_then(|entry| entry.best_image());
            self.entries.insert(
                block_id,
                MermaidRenderEntry::new(content_version, hash, source, theme, fallback, cx),
            );
        }
    }

    pub(crate) fn status(&self, block_id: BlockId) -> Option<MermaidRenderStatus> {
        self.entries.get(&block_id).map(MermaidRenderEntry::status)
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

fn source_hash(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn validate_source(source: &str) -> Result<(), String> {
    if source.trim().is_empty() {
        return Err("Mermaid 源码为空".to_owned());
    }
    if source.len() > MAX_MERMAID_SOURCE_BYTES {
        return Err(format!(
            "Mermaid 源码超过 {} KiB 安全上限",
            MAX_MERMAID_SOURCE_BYTES / 1024
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_validation_rejects_empty_and_oversized_input() {
        assert!(validate_source("  \n").is_err());
        assert!(validate_source("flowchart TD\n A --> B").is_ok());
        assert!(validate_source(&"x".repeat(MAX_MERMAID_SOURCE_BYTES + 1)).is_err());
    }

    #[test]
    fn source_hash_changes_with_content() {
        assert_eq!(source_hash("A --> B"), source_hash("A --> B"));
        assert_ne!(source_hash("A --> B"), source_hash("A --> C"));
    }
}
