//! Code-block syntax highlighting and theme metadata.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};

use cditor_core::ids::BlockId;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadView, InlineMark, InlineSpan, RichBlockKind,
};
use cditor_runtime::EditorViewProjection;
use gpui::{AppContext, Context, Task};
use lumis::highlight::Highlighter;
use lumis::languages::Language;
use lumis::themes::{self, UnderlineStyle};

use crate::gui::app::CditorV2View;

pub(crate) const DEFAULT_CODE_HIGHLIGHT_THEME: &str = "catppuccin_latte";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CodeThemeItem {
    pub id: &'static str,
    pub label: &'static str,
    pub background: u32,
    pub foreground: u32,
    pub preview: [u32; 4],
}

pub(crate) const CODE_THEME_ITEMS: [CodeThemeItem; 8] = [
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
    CodeThemeItem {
        id: "dracula",
        label: "Dracula",
        background: 0x282a36,
        foreground: 0xf8f8f2,
        preview: [0xff79c6, 0xbd93f9, 0xf1fa8c, 0x6272a4],
    },
    CodeThemeItem {
        id: "catppuccin_latte",
        label: "Catppuccin Latte",
        background: 0xeff1f5,
        foreground: 0x4c4f69,
        preview: [0x8839ef, 0x1e66f5, 0x40a02b, 0x9ca0b0],
    },
    CodeThemeItem {
        id: "catppuccin_mocha",
        label: "Catppuccin Mocha",
        background: 0x1e1e2e,
        foreground: 0xcdd6f4,
        preview: [0xcba6f7, 0x89b4fa, 0xa6e3a1, 0x6c7086],
    },
    CodeThemeItem {
        id: "gruvbox_light",
        label: "Gruvbox Light",
        background: 0xfbf1c7,
        foreground: 0x3c3836,
        preview: [0x9d0006, 0x076678, 0x79740e, 0x928374],
    },
    CodeThemeItem {
        id: "gruvbox_dark",
        label: "Gruvbox Dark",
        background: 0x282828,
        foreground: 0xebdbb2,
        preview: [0xfb4934, 0x83a598, 0xb8bb26, 0x928374],
    },
    CodeThemeItem {
        id: "kanagawa_wave",
        label: "Kanagawa Wave",
        background: 0x1f1f28,
        foreground: 0xdcd7ba,
        preview: [0x957fb8, 0x7e9cd8, 0x98bb6c, 0x727169],
    },
];

pub(crate) fn code_theme_item(theme_name: &str) -> CodeThemeItem {
    CODE_THEME_ITEMS
        .iter()
        .copied()
        .find(|item| item.id == theme_name)
        .or_else(|| {
            CODE_THEME_ITEMS
                .iter()
                .copied()
                .find(|item| item.id == DEFAULT_CODE_HIGHLIGHT_THEME)
        })
        .expect("default code highlight theme is in the menu")
}

type HighlightResult = Result<Arc<Vec<InlineSpan>>, Arc<str>>;

struct CodeHighlightEntry {
    content_version: u64,
    source_hash: u64,
    language: Language,
    theme_name: &'static str,
    source: Arc<str>,
    fallback: Arc<Vec<InlineSpan>>,
    result: Arc<OnceLock<HighlightResult>>,
    _task: Task<()>,
}

impl CodeHighlightEntry {
    fn new(
        content_version: u64,
        source_hash: u64,
        source: String,
        language: Language,
        theme_name: &'static str,
        fallback: Vec<InlineSpan>,
        cx: &mut Context<CditorV2View>,
    ) -> Self {
        let stored_source = Arc::<str>::from(source.as_str());
        let result = Arc::new(OnceLock::new());
        let result_for_task = result.clone();
        let task = cx.spawn(async move |view, cx| {
            let highlighted = cx
                .background_spawn(async move {
                    highlight_source(&source, language, theme_name)
                        .map(Arc::new)
                        .map_err(Arc::<str>::from)
                })
                .await;
            let _ = result_for_task.set(highlighted);
            let _ = view.update(cx, |_view, cx| cx.notify());
        });

        Self {
            content_version,
            source_hash,
            language,
            theme_name,
            source: stored_source,
            fallback: Arc::new(fallback),
            result,
            _task: task,
        }
    }

    fn matches(
        &self,
        content_version: u64,
        source_hash: u64,
        language: Language,
        theme_name: &str,
    ) -> bool {
        self.content_version == content_version
            && self.source_hash == source_hash
            && self.language == language
            && self.theme_name == theme_name
    }

    fn spans(&self) -> &[InlineSpan] {
        self.result
            .get()
            .and_then(|result| result.as_ref().ok())
            .map(|spans| spans.as_slice())
            .unwrap_or(self.fallback.as_slice())
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
    pub(crate) fn sync_visible_window(
        &mut self,
        projection: &EditorViewProjection,
        theme_name: &'static str,
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
                    text,
                } = &payload.payload
                else {
                    return None;
                };
                let language = code_language(payload_language.as_deref().or(language.as_deref()))?;
                Some((
                    block.block_id,
                    payload.content_version,
                    source_hash(text),
                    text.clone(),
                    language,
                ))
            })
            .collect::<Vec<_>>();
        let visible_ids = visible
            .iter()
            .map(|(block_id, _, _, _, _)| *block_id)
            .collect::<HashSet<_>>();
        self.entries
            .retain(|block_id, _| visible_ids.contains(block_id));

        for (block_id, content_version, hash, source, language) in visible {
            if self
                .entries
                .get(&block_id)
                .is_some_and(|entry| entry.matches(content_version, hash, language, theme_name))
            {
                continue;
            }
            let fallback = self
                .entries
                .remove(&block_id)
                .map(|entry| rebase_spans(&entry.source, entry.spans(), &source))
                .unwrap_or_else(|| vec![InlineSpan::plain(&source)]);
            self.entries.insert(
                block_id,
                CodeHighlightEntry::new(
                    content_version,
                    hash,
                    source,
                    language,
                    theme_name,
                    fallback,
                    cx,
                ),
            );
        }
    }

    pub(crate) fn spans(&self, block_id: BlockId) -> Option<&[InlineSpan]> {
        self.entries.get(&block_id).map(CodeHighlightEntry::spans)
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

fn code_language(language: Option<&str>) -> Option<Language> {
    let normalized = language?.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "text" | "plain" | "plaintext" | "plain text" => None,
        "rust" | "rs" => Some(Language::Rust),
        "typescript" | "ts" => Some(Language::TypeScript),
        "javascript" | "js" | "jsx" => Some(Language::JavaScript),
        "tsx" => Some(Language::Tsx),
        "python" | "py" => Some(Language::Python),
        "go" | "golang" => Some(Language::Go),
        "java" => Some(Language::Java),
        "kotlin" | "kt" => Some(Language::Kotlin),
        "swift" => Some(Language::Swift),
        "c" => Some(Language::C),
        "cpp" | "c++" => Some(Language::CPlusPlus),
        "csharp" | "c#" | "cs" => Some(Language::CSharp),
        "html" | "htm" => Some(Language::HTML),
        "css" => Some(Language::CSS),
        "scss" => Some(Language::SCSS),
        "json" => Some(Language::JSON),
        "yaml" | "yml" => Some(Language::YAML),
        "toml" => Some(Language::Toml),
        "markdown" | "md" => Some(Language::Markdown),
        "sql" => Some(Language::SQL),
        "shell" | "sh" | "bash" => Some(Language::Bash),
        "zsh" => Some(Language::Zsh),
        "dockerfile" | "docker" => Some(Language::Dockerfile),
        "diff" | "patch" => Some(Language::Diff),
        _ => None,
    }
}

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

fn source_hash(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::plain_text_from_spans;

    #[test]
    fn editor_language_labels_map_to_lumis_languages() {
        let supported = [
            "rust",
            "typescript",
            "javascript",
            "tsx",
            "jsx",
            "python",
            "go",
            "java",
            "kotlin",
            "swift",
            "c",
            "cpp",
            "csharp",
            "html",
            "css",
            "scss",
            "json",
            "yaml",
            "toml",
            "markdown",
            "sql",
            "shell",
            "bash",
            "zsh",
            "dockerfile",
            "diff",
        ];
        assert!(
            supported
                .into_iter()
                .all(|label| code_language(Some(label)).is_some())
        );
        assert_eq!(code_language(Some("plain text")), None);
        assert_eq!(code_language(Some("unknown-language")), None);
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
    fn bundled_theme_menu_items_resolve_in_lumis() {
        assert!(
            CODE_THEME_ITEMS
                .iter()
                .all(|item| themes::get(item.id).is_ok())
        );
        assert!(
            CODE_THEME_ITEMS
                .iter()
                .any(|item| item.id == DEFAULT_CODE_HIGHLIGHT_THEME)
        );
    }
}
