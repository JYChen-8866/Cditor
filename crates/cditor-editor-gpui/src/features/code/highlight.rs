//! Code-block syntax highlighting and theme metadata.

#![cfg_attr(not(feature = "code-highlight"), allow(dead_code))]

use std::collections::HashMap;
#[cfg(feature = "code-highlight")]
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use cditor_core::ids::BlockId;
#[cfg(feature = "code-highlight")]
use cditor_core::rich_text::BlockPayloadView;
use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, InlineSpan};
#[cfg(any(feature = "code-highlight", test))]
use cditor_core::rich_text::{InlineMark, RichBlockKind};
use cditor_runtime::EditorViewProjection;
#[cfg(feature = "code-highlight")]
use cditor_runtime::{MainThreadWorkKind, WorkCost, WorkerTaskKind};
use cditor_text::requires_segmentation;
#[cfg(feature = "code-highlight")]
use gpui::AppContext;
use gpui::{Context, Task};
#[cfg(feature = "code-highlight")]
mod lumis_imports {
    pub(crate) use lumis::highlight::Highlighter;
    pub(crate) use lumis::themes::{self, UnderlineStyle};
}
#[cfg(feature = "code-highlight")]
use lumis_imports::*;

// Stub Language type for when code-highlight is disabled
#[cfg(not(feature = "code-highlight"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StubLanguage {
    Unknown,
}

#[cfg(not(feature = "code-highlight"))]
pub(crate) use StubLanguage as Language;
#[cfg(feature = "code-highlight")]
pub(crate) use lumis::languages::Language;

use crate::app::worker_admission::EditorWorkerAdmission;
#[cfg(feature = "code-highlight")]
use crate::app::worker_admission::WorkerPermit;
use crate::editor_view::CditorV2View;
use crate::text::input::RichTextLayoutSpans;

pub(crate) const DEFAULT_CODE_HIGHLIGHT_THEME_LIGHT: &str = "github_light";
pub(crate) const DEFAULT_CODE_HIGHLIGHT_THEME_DARK: &str = "github_dark";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CodeThemeItem {
    pub id: &'static str,
    pub label: &'static str,
    pub background: u32,
    pub foreground: u32,
    pub preview: [u32; 4],
}

pub(crate) const CODE_THEME_ITEMS: [CodeThemeItem; 2] = [
    CodeThemeItem {
        id: "github_light",
        label: "GitHub Light",
        background: 0xf6f8fa,
        foreground: 0x1f2328,
        preview: [0xcf222e, 0x0550ae, 0x0a3069, 0x57606a],
    },
    CodeThemeItem {
        id: "github_dark",
        label: "GitHub Dark",
        background: 0x0d1117,
        foreground: 0xe6edf3,
        preview: [0xff7b72, 0x79c0ff, 0xa5d6ff, 0x8b949e],
    },
];

pub(crate) fn code_theme_item(theme_name: &str) -> CodeThemeItem {
    CODE_THEME_ITEMS
        .iter()
        .copied()
        .find(|item| item.id == theme_name)
        .unwrap_or(CODE_THEME_ITEMS[0])
}

/// Get code theme based on global theme mode (light/dark)
pub(crate) fn code_theme_for_mode(is_dark: bool) -> &'static str {
    if is_dark {
        DEFAULT_CODE_HIGHLIGHT_THEME_DARK
    } else {
        DEFAULT_CODE_HIGHLIGHT_THEME_LIGHT
    }
}

type HighlightResult = Result<RichTextLayoutSpans, Arc<str>>;

struct CodeHighlightEntry {
    content_version: u64,
    language: Language,
    theme_name: &'static str,
    source: Arc<BlockPayloadRecord>,
    fallback: Option<RichTextLayoutSpans>,
    result: Arc<OnceLock<HighlightResult>>,
    _task: Task<()>,
}

struct CodeHighlightRequest {
    block_id: BlockId,
    source: Arc<BlockPayloadRecord>,
    language: Language,
    theme_name: &'static str,
    fallback: Option<RichTextLayoutSpans>,
}

impl CodeHighlightEntry {
    #[cfg(feature = "code-highlight")]
    fn new(
        request: CodeHighlightRequest,
        permit: WorkerPermit,
        cx: &mut Context<CditorV2View>,
    ) -> Self {
        let CodeHighlightRequest {
            block_id,
            source,
            language,
            theme_name,
            fallback,
        } = request;
        let content_version = source.content_version;
        let result = Arc::new(OnceLock::new());
        let result_for_task = result.clone();
        let source_for_task = source.clone();
        let task = cx.spawn(async move |view, cx| {
            let highlighted = cx
                .background_spawn(async move {
                    let _permit = permit;
                    highlight_source(code_source(&source_for_task), language, theme_name)
                        .map(RichTextLayoutSpans::from)
                        .map_err(Arc::<str>::from)
                })
                .await;
            let _ = view.update(cx, |view, cx| {
                view.enqueue_main_thread_apply(
                    MainThreadWorkKind::AsyncMeasureApply,
                    content_version,
                    Some(block_id),
                    WorkCost {
                        sync_ms: 0.05,
                        async_results: 1,
                        ..WorkCost::ZERO
                    },
                    move |_view, cx| {
                        let _ = result_for_task.set(highlighted);
                        cx.notify();
                    },
                    cx,
                );
            });
        });

        Self {
            content_version,
            language,
            theme_name,
            source,
            fallback,
            result,
            _task: task,
        }
    }

    fn matches(&self, content_version: u64, language: Language, theme_name: &str) -> bool {
        self.content_version == content_version
            && self.language == language
            && self.theme_name == theme_name
    }

    fn spans(&self) -> Option<RichTextLayoutSpans> {
        self.result
            .get()
            .and_then(|result| result.as_ref().ok())
            .cloned()
            .or_else(|| self.fallback.clone())
    }

    fn source(&self) -> &str {
        code_source(&self.source)
    }
}

/// Viewport-scoped syntax colors for editable code blocks.
///
/// Highlighting runs off the GPUI thread. Until a result is ready the editor keeps
/// rendering the current plain source, so typing, selection, IME and caret geometry
/// never depend on Lumis or on HTML parsing.
#[derive(Default)]
pub(crate) struct CodeHighlightCache {
    entries: HashMap<BlockId, CodeHighlightEntry>,
}

impl CodeHighlightCache {
    #[cfg(feature = "code-highlight")]
    pub(crate) fn sync_visible_window(
        &mut self,
        projection: &EditorViewProjection,
        theme_name: &'static str,
        worker_admission: &EditorWorkerAdmission,
        cx: &mut Context<CditorV2View>,
    ) {
        let visible = projection
            .blocks
            .iter()
            .filter_map(|block| {
                let RichBlockKind::Code { language } = &block.kind else {
                    return None;
                };
                let BlockPayloadView::Loaded(payload) = &block.payload else {
                    return None;
                };
                let BlockPayload::Code {
                    language: payload_language,
                    ..
                } = &payload.payload
                else {
                    return None;
                };
                let language = code_language(payload_language.as_deref().or(language.as_deref()))?;
                Some((block.block_id, payload.clone(), language))
            })
            .collect::<Vec<_>>();
        let visible_ids = visible
            .iter()
            .map(|(block_id, _, _)| *block_id)
            .collect::<HashSet<_>>();
        if crate::diagnostics::flash::enabled() {
            let evicted = self
                .entries
                .keys()
                .filter(|block_id| !visible_ids.contains(block_id))
                .copied()
                .collect::<Vec<_>>();
            if !evicted.is_empty() {
                crate::diagnostics::flash::trace(
                    "code.highlight-evict",
                    format_args!(
                        "evicted={evicted:?} (code block left the projection or its payload \
                         is not loaded this frame; re-highlight needed when it returns)"
                    ),
                );
            }
        }
        self.entries
            .retain(|block_id, _| visible_ids.contains(block_id));

        for (block_id, source, language) in visible {
            if self
                .entries
                .get(&block_id)
                .is_some_and(|entry| entry.matches(source.content_version, language, theme_name))
            {
                continue;
            }
            let Some(permit) = worker_admission.try_acquire(WorkerTaskKind::SyntaxHighlight) else {
                crate::diagnostics::flash::trace(
                    "code.highlight-permit-denied",
                    format_args!(
                        "block={block_id} content_version={} stays unhighlighted this frame",
                        source.content_version
                    ),
                );
                continue;
            };
            let source_text = code_source(&source);
            let previous = self.entries.remove(&block_id);
            let fallback = if should_build_synchronous_fallback(source_text.len()) {
                Some(RichTextLayoutSpans::from(
                    previous
                        .and_then(|entry| {
                            let spans = entry.spans()?;
                            Some(rebase_spans(
                                entry.source(),
                                spans.as_inline_spans()?,
                                source_text,
                            ))
                        })
                        .unwrap_or_else(|| vec![InlineSpan::plain(source_text)]),
                ))
            } else {
                None
            };
            crate::diagnostics::flash::trace(
                "code.highlight-request",
                format_args!(
                    "block={block_id} content_version={} source_len={} synchronous_fallback={}",
                    source.content_version,
                    source_text.len(),
                    fallback.is_some()
                ),
            );
            self.entries.insert(
                block_id,
                CodeHighlightEntry::new(
                    CodeHighlightRequest {
                        block_id,
                        source,
                        language,
                        theme_name,
                        fallback,
                    },
                    permit,
                    cx,
                ),
            );
        }
    }

    #[cfg(not(feature = "code-highlight"))]
    pub(crate) fn sync_visible_window(
        &mut self,
        _projection: &EditorViewProjection,
        _theme_name: &'static str,
        _worker_admission: &EditorWorkerAdmission,
        _cx: &mut Context<CditorV2View>,
    ) {
    }

    pub(crate) fn spans(
        &self,
        block_id: BlockId,
        content_version: u64,
    ) -> Option<RichTextLayoutSpans> {
        self.entries
            .get(&block_id)
            .filter(|entry| entry.content_version == content_version)
            .and_then(CodeHighlightEntry::spans)
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(feature = "code-highlight")]
fn code_language(language: Option<&str>) -> Option<Language> {
    let normalized = language?.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "text" | "plain" | "plaintext" | "plain text" => None,
        "rust" | "rs" => Some(Language::Rust),
        "bash" | "shell" | "zsh" => Some(Language::Bash),
        "typescript" | "ts" => Some(Language::TypeScript),
        "javascript" | "js" | "jsx" => Some(Language::JavaScript),
        "python" | "py" => Some(Language::Python),
        "go" | "golang" => Some(Language::Go),
        "swift" => Some(Language::Swift),
        "c" => Some(Language::C),
        "cpp" | "c++" => Some(Language::CPlusPlus),
        "csharp" | "c#" | "cs" => Some(Language::CSharp),
        "html" | "htm" => Some(Language::HTML),
        "java" => Some(Language::Java),
        "json" => Some(Language::JSON),
        "yaml" | "yml" => Some(Language::YAML),
        "sql" => Some(Language::SQL),
        "diff" | "patch" => Some(Language::Diff),
        "toml" => Some(Language::Toml),
        _ => None,
    }
}

#[cfg(feature = "code-highlight")]
pub(crate) fn code_language_supported(language: Option<&str>) -> bool {
    code_language(language).is_some()
}

#[cfg(not(feature = "code-highlight"))]
pub(crate) fn code_language_supported(_language: Option<&str>) -> bool {
    false
}

#[cfg(feature = "code-highlight")]
fn highlight_source(
    source: &str,
    language: Language,
    theme_name: &str,
) -> Result<Vec<InlineSpan>, String> {
    let theme = themes::get(theme_name).map_err(|error| error.to_string())?;
    let highlighter = Highlighter::new(language, Some(theme));
    let highlighted = highlighter
        .highlight(source)
        .map_err(|error| error.to_string())?;
    let mut spans = Vec::<InlineSpan>::with_capacity(highlighted.len());

    for (style, text) in highlighted {
        if text.is_empty() {
            continue;
        }
        let mut marks = Vec::with_capacity(4);
        if let Some(color) = &style.fg {
            marks.push(InlineMark::Color(color.clone()));
        }
        if style.bold {
            marks.push(InlineMark::Bold);
        }
        if style.italic {
            marks.push(InlineMark::Italic);
        }
        if style.text_decoration.underline != UnderlineStyle::None {
            marks.push(InlineMark::Underline);
        }
        if style.text_decoration.strikethrough {
            marks.push(InlineMark::Strike);
        }

        if let Some(previous) = spans.last_mut()
            && previous.marks == marks
        {
            previous.text.push_str(text);
        } else {
            spans.push(InlineSpan {
                text: text.to_owned(),
                marks,
            });
        }
    }

    if spans.is_empty() && !source.is_empty() {
        spans.push(InlineSpan::plain(source));
    }
    Ok(spans)
}

fn rebase_spans(old_source: &str, old_spans: &[InlineSpan], new_source: &str) -> Vec<InlineSpan> {
    let prefix = common_prefix_bytes(old_source, new_source);
    let suffix = common_suffix_bytes(&old_source[prefix..], &new_source[prefix..]);
    let old_suffix_start = old_source.len() - suffix;
    let new_suffix_start = new_source.len() - suffix;
    let mut rebased = Vec::new();
    append_span_slice(&mut rebased, old_spans, 0, prefix);
    push_span(
        &mut rebased,
        InlineSpan::plain(&new_source[prefix..new_suffix_start]),
    );
    append_span_slice(&mut rebased, old_spans, old_suffix_start, old_source.len());
    rebased
}

fn common_prefix_bytes(left: &str, right: &str) -> usize {
    left.char_indices()
        .zip(right.chars())
        .take_while(|((_, left), right)| *left == *right)
        .map(|((offset, character), _)| offset + character.len_utf8())
        .last()
        .unwrap_or(0)
}

fn common_suffix_bytes(left: &str, right: &str) -> usize {
    left.char_indices()
        .rev()
        .zip(right.chars().rev())
        .take_while(|((_, left), right)| *left == *right)
        .map(|((offset, _), _)| left.len() - offset)
        .last()
        .unwrap_or(0)
}

fn append_span_slice(
    target: &mut Vec<InlineSpan>,
    spans: &[InlineSpan],
    range_start: usize,
    range_end: usize,
) {
    if range_start >= range_end {
        return;
    }
    let mut offset = 0;
    for span in spans {
        let span_start = offset;
        let span_end = span_start + span.text.len();
        offset = span_end;
        let start = span_start.max(range_start);
        let end = span_end.min(range_end);
        if start < end {
            push_span(
                target,
                InlineSpan {
                    text: span.text[start - span_start..end - span_start].to_owned(),
                    marks: span.marks.clone(),
                },
            );
        }
    }
}

fn push_span(target: &mut Vec<InlineSpan>, span: InlineSpan) {
    if span.text.is_empty() {
        return;
    }
    if let Some(previous) = target.last_mut()
        && previous.marks == span.marks
    {
        previous.text.push_str(&span.text);
    } else {
        target.push(span);
    }
}

fn code_source(record: &BlockPayloadRecord) -> &str {
    match &record.payload {
        BlockPayload::Code { text, .. } => text,
        _ => "",
    }
}

fn should_build_synchronous_fallback(text_len: usize) -> bool {
    !requires_segmentation(text_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::plain_text_from_spans;

    #[test]
    fn segmented_code_skips_the_full_source_main_thread_fallback() {
        assert!(should_build_synchronous_fallback(1024));
        assert!(!should_build_synchronous_fallback(10 * 1024 * 1024));
    }

    #[test]
    fn cache_never_returns_spans_for_a_stale_content_version() {
        let block_id = 17;
        let source = Arc::new(BlockPayloadRecord {
            block_id,
            content_version: 8,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn current() {}".to_owned(),
            },
        });
        let highlighted = Arc::new(vec![InlineSpan::plain("fn current() {}")]);
        let layout_spans = RichTextLayoutSpans::from(highlighted.clone());
        let result = Arc::new(OnceLock::new());
        result
            .set(Ok(layout_spans))
            .expect("test result is initialized once");
        let mut cache = CodeHighlightCache::default();
        cache.entries.insert(
            block_id,
            CodeHighlightEntry {
                content_version: source.content_version,
                language: Language::Rust,
                theme_name: DEFAULT_CODE_HIGHLIGHT_THEME_LIGHT,
                source,
                fallback: None,
                result,
                _task: Task::ready(()),
            },
        );

        assert!(
            cache
                .spans(block_id, 8)
                .expect("matching content version should use highlighted spans")
                .shares_materialized_spans(&highlighted)
        );
        assert!(cache.spans(block_id, 9).is_none());
    }

    #[test]
    fn editor_language_labels_map_to_lumis_languages() {
        let supported = [
            "rust",
            "bash",
            "shell",
            "zsh",
            "typescript",
            "javascript",
            "jsx",
            "python",
            "go",
            "swift",
            "c",
            "cpp",
            "csharp",
            "html",
            "java",
            "json",
            "yaml",
            "sql",
            "diff",
            "toml",
        ];
        assert!(
            supported
                .into_iter()
                .all(|label| code_language(Some(label)).is_some())
        );
        for unsupported in ["tsx", "kotlin", "css", "scss", "markdown", "dockerfile"] {
            assert_eq!(code_language(Some(unsupported)), None);
        }
        assert_eq!(code_language(Some("plain text")), None);
        assert_eq!(code_language(Some("unknown-language")), None);
    }

    #[test]
    fn bash_and_toml_highlighting_preserve_source_and_color_tokens() {
        for (language, source) in [
            (Language::Bash, "#!/usr/bin/env bash\necho \"hello\""),
            (Language::Toml, "[package]\nname = \"cditor\""),
        ] {
            let spans = highlight_source(source, language, DEFAULT_CODE_HIGHLIGHT_THEME_LIGHT)
                .expect("language highlighting succeeds");
            assert_eq!(plain_text_from_spans(&spans), source);
            assert!(spans.iter().any(|span| {
                span.marks
                    .iter()
                    .any(|mark| matches!(mark, InlineMark::Color(_)))
            }));
        }
    }

    #[test]
    fn javascript_highlight_preserves_source_and_adds_colors() {
        let source = "const x = 1; // 你好 👋";
        let spans =
            highlight_source(source, Language::JavaScript, "dracula").expect("highlight succeeds");

        assert_eq!(plain_text_from_spans(&spans), source);
        assert!(
            spans
                .iter()
                .flat_map(|span| &span.marks)
                .any(|mark| matches!(mark, InlineMark::Color(_)))
        );
    }

    #[test]
    fn adjacent_equal_styles_are_coalesced_without_losing_bytes() {
        let source = "fn main() {\n    println!(\"hi\");\n}\n";
        let spans =
            highlight_source(source, Language::Rust, "dracula").expect("highlight succeeds");

        assert_eq!(plain_text_from_spans(&spans), source);
        assert!(spans.iter().all(|span| !span.text.is_empty()));
        assert!(spans.windows(2).all(|pair| pair[0].marks != pair[1].marks));
    }

    #[test]
    fn default_rust_theme_produces_distinct_text_layout_foreground_runs() {
        let source = "fn main() {\n    let answer = 42;\n}";
        let spans = highlight_source(source, Language::Rust, DEFAULT_CODE_HIGHLIGHT_THEME_LIGHT)
            .expect("default Rust highlighting succeeds");
        let style_runs = cditor_text::text_style_runs(
            &spans,
            &RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            cditor_text::TextTheme::default(),
            code_theme_item(DEFAULT_CODE_HIGHLIGHT_THEME_LIGHT).foreground,
            &cditor_text::TextStyleConfig::default(),
            crate::platform::EDITOR_MONO_FONT_FAMILY,
        );
        let colors = style_runs
            .iter()
            .map(|run| run.style.brush.foreground)
            .collect::<HashSet<_>>();

        assert_eq!(plain_text_from_spans(&spans), source);
        assert!(spans.iter().any(|span| !span.marks.is_empty()));
        assert!(
            colors.len() > 1,
            "syntax colors must reach TextLayout style runs"
        );
    }

    #[test]
    fn rebase_keeps_existing_colors_around_unicode_insertions() {
        let old_source = "const 名 = 1;";
        let old_spans = highlight_source(old_source, Language::JavaScript, "dracula")
            .expect("highlight succeeds");
        let new_source = "const 新名 = 1;";

        let rebased = rebase_spans(old_source, &old_spans, new_source);

        assert_eq!(plain_text_from_spans(&rebased), new_source);
        assert!(rebased.iter().any(|span| !span.marks.is_empty()));
    }

    #[test]
    fn rebase_preserves_exact_text_for_delete_replace_and_empty_edits() {
        let old_source = "const answer = 42;";
        let old_spans = highlight_source(old_source, Language::JavaScript, "dracula")
            .expect("highlight succeeds");

        for new_source in ["const answer = 4;", "let answer = 42;", "", "界"] {
            let rebased = rebase_spans(old_source, &old_spans, new_source);
            assert_eq!(plain_text_from_spans(&rebased), new_source);
        }
    }

    #[test]
    fn bundled_theme_menu_items_have_valid_ids() {
        assert!(
            CODE_THEME_ITEMS
                .iter()
                .any(|item| item.id == DEFAULT_CODE_HIGHLIGHT_THEME_LIGHT)
        );
        assert!(
            CODE_THEME_ITEMS
                .iter()
                .any(|item| item.id == DEFAULT_CODE_HIGHLIGHT_THEME_DARK)
        );
    }

    #[test]
    fn bundled_themes_resolve_in_lumis() {
        assert!(
            CODE_THEME_ITEMS
                .iter()
                .all(|item| themes::get(item.id).is_ok())
        );
    }
}
