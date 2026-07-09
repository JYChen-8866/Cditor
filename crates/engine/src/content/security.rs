#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyMode {
    Strict,
    Balanced,
    Permissive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgPolicy {
    Strip,
    Placeholder,
    AllowSafeStatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteResourcePolicy {
    Block,
    AskUser,
    MetadataOnly,
    Allow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedPolicy {
    Block,
    AskUser,
    AllowSandboxed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileUrlPolicy {
    Block,
    AskUser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataUrlPolicy {
    Block,
    AllowImagesOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalContentPolicy {
    pub privacy_mode: PrivacyMode,
    pub svg_policy: SvgPolicy,
    pub remote_image_policy: RemoteResourcePolicy,
    pub embed_policy: EmbedPolicy,
    pub file_url_policy: FileUrlPolicy,
    pub data_url_policy: DataUrlPolicy,
}

impl Default for ExternalContentPolicy {
    fn default() -> Self {
        Self {
            privacy_mode: PrivacyMode::Strict,
            svg_policy: SvgPolicy::Placeholder,
            remote_image_policy: RemoteResourcePolicy::AskUser,
            embed_policy: EmbedPolicy::AskUser,
            file_url_policy: FileUrlPolicy::AskUser,
            data_url_policy: DataUrlPolicy::Block,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalResourceKind {
    DangerousUrl,
    RemoteImage,
    Svg,
    IframeEmbed,
    FileUrl,
    DataUrl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalResourceAction {
    Allow,
    Block,
    AskUser,
    MetadataOnly,
    Sandbox,
    Placeholder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalResourceDecision {
    pub kind: ExternalResourceKind,
    pub url: Option<String>,
    pub action: ExternalResourceAction,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedHtml {
    pub html: String,
    pub removed_scripts: usize,
    pub removed_event_handlers: usize,
    pub decisions: Vec<ExternalResourceDecision>,
}

impl SanitizedHtml {
    pub fn blocked_count(&self) -> usize {
        self.decisions
            .iter()
            .filter(|decision| decision.action == ExternalResourceAction::Block)
            .count()
    }

    pub fn requires_user_confirmation(&self) -> bool {
        self.decisions
            .iter()
            .any(|decision| decision.action == ExternalResourceAction::AskUser)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HtmlTag {
    closing: bool,
    name: String,
    attrs: Vec<HtmlAttr>,
    self_closing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HtmlAttr {
    name: String,
    value: Option<String>,
    quote: Option<char>,
}

pub fn sanitize_external_html(html: &str, policy: ExternalContentPolicy) -> SanitizedHtml {
    let (without_scripts, _, removed_scripts) = strip_block(html, "script", None);
    let (without_svg, svg_decisions) = sanitize_svg_blocks(&without_scripts, policy.svg_policy);
    let mut decisions = svg_decisions;
    let mut removed_event_handlers = 0;
    let mut output = String::with_capacity(without_svg.len());
    let mut cursor = 0;

    while let Some(start_rel) = without_svg[cursor..].find('<') {
        let start = cursor + start_rel;
        output.push_str(&without_svg[cursor..start]);
        let Some(end_rel) = without_svg[start..].find('>') else {
            break;
        };
        let end = start + end_rel;
        let raw_tag = &without_svg[start + 1..end];
        if let Some(tag) = parse_tag(raw_tag) {
            let sanitized = sanitize_tag(tag, policy, &mut decisions, &mut removed_event_handlers);
            output.push_str(&sanitized);
        }
        cursor = end + 1;
    }
    output.push_str(&without_svg[cursor..]);

    SanitizedHtml {
        html: output,
        removed_scripts,
        removed_event_handlers,
        decisions,
    }
}

fn sanitize_svg_blocks(html: &str, policy: SvgPolicy) -> (String, Vec<ExternalResourceDecision>) {
    match policy {
        SvgPolicy::AllowSafeStatic => (html.to_owned(), Vec::new()),
        SvgPolicy::Strip => {
            let (html, decisions, _) = strip_block(
                html,
                "svg",
                Some(ExternalResourceDecision {
                    kind: ExternalResourceKind::Svg,
                    url: None,
                    action: ExternalResourceAction::Block,
                    reason: "svg stripped by policy".to_owned(),
                }),
            );
            (html, decisions)
        }
        SvgPolicy::Placeholder => {
            let (html, decisions, _) = strip_block(
                html,
                "svg",
                Some(ExternalResourceDecision {
                    kind: ExternalResourceKind::Svg,
                    url: None,
                    action: ExternalResourceAction::Placeholder,
                    reason: "svg replaced by stable placeholder".to_owned(),
                }),
            );
            (html, decisions)
        }
    }
}

fn strip_block(
    html: &str,
    tag: &str,
    decision: Option<ExternalResourceDecision>,
) -> (String, Vec<ExternalResourceDecision>, usize) {
    let lower = html.to_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut result = String::with_capacity(html.len());
    let mut cursor = 0;
    let mut decisions = Vec::new();
    let mut removed_count = 0;
    while let Some(start_rel) = lower[cursor..].find(&open) {
        let start = cursor + start_rel;
        result.push_str(&html[cursor..start]);
        removed_count += 1;
        if let Some(decision) = decision.clone() {
            match decision.action {
                ExternalResourceAction::Placeholder => {
                    result.push_str(&format!("<div data-cditor-placeholder=\"{}\"></div>", tag));
                }
                _ => {}
            }
            decisions.push(decision);
        }
        let Some(end_rel) = lower[start..].find(&close) else {
            cursor = html.len();
            break;
        };
        cursor = start + end_rel + close.len();
    }
    result.push_str(&html[cursor..]);
    (result, decisions, removed_count)
}

fn parse_tag(raw: &str) -> Option<HtmlTag> {
    let mut rest = raw.trim();
    if rest.is_empty() || rest.starts_with('!') {
        return None;
    }
    let closing = rest.starts_with('/');
    if closing {
        rest = rest[1..].trim_start();
    }
    let self_closing = rest.ends_with('/');
    if self_closing {
        rest = rest[..rest.len() - 1].trim_end();
    }

    let name_end = rest
        .find(|ch: char| ch.is_whitespace())
        .unwrap_or(rest.len());
    let name = rest[..name_end].to_ascii_lowercase();
    let attrs_src = rest[name_end..].trim();
    Some(HtmlTag {
        closing,
        name,
        attrs: parse_attrs(attrs_src),
        self_closing,
    })
}

fn parse_attrs(mut src: &str) -> Vec<HtmlAttr> {
    let mut attrs = Vec::new();
    while !src.trim_start().is_empty() {
        src = src.trim_start();
        let name_end = src
            .find(|ch: char| ch.is_whitespace() || ch == '=')
            .unwrap_or(src.len());
        let name = src[..name_end].to_ascii_lowercase();
        src = &src[name_end..];
        src = src.trim_start();
        if !src.starts_with('=') {
            attrs.push(HtmlAttr {
                name,
                value: None,
                quote: None,
            });
            continue;
        }
        src = src[1..].trim_start();
        let Some(first) = src.chars().next() else {
            attrs.push(HtmlAttr {
                name,
                value: Some(String::new()),
                quote: None,
            });
            break;
        };
        if first == '"' || first == '\'' {
            let rest = &src[first.len_utf8()..];
            let value_end = rest.find(first).unwrap_or(rest.len());
            attrs.push(HtmlAttr {
                name,
                value: Some(rest[..value_end].to_owned()),
                quote: Some(first),
            });
            src = if value_end < rest.len() {
                &rest[value_end + first.len_utf8()..]
            } else {
                ""
            };
        } else {
            let value_end = src.find(|ch: char| ch.is_whitespace()).unwrap_or(src.len());
            attrs.push(HtmlAttr {
                name,
                value: Some(src[..value_end].to_owned()),
                quote: None,
            });
            src = &src[value_end..];
        }
    }
    attrs
}

fn sanitize_tag(
    tag: HtmlTag,
    policy: ExternalContentPolicy,
    decisions: &mut Vec<ExternalResourceDecision>,
    removed_event_handlers: &mut usize,
) -> String {
    if tag.closing {
        return format!("</{}>", tag.name);
    }

    if matches!(tag.name.as_str(), "iframe" | "embed") {
        let src = tag
            .attrs
            .iter()
            .find(|attr| attr.name == "src")
            .and_then(|attr| attr.value.clone());
        let action = match policy.embed_policy {
            EmbedPolicy::Block => ExternalResourceAction::Block,
            EmbedPolicy::AskUser => ExternalResourceAction::AskUser,
            EmbedPolicy::AllowSandboxed => ExternalResourceAction::Sandbox,
        };
        decisions.push(ExternalResourceDecision {
            kind: ExternalResourceKind::IframeEmbed,
            url: src,
            action,
            reason: "iframe/embed requires explicit sandbox policy".to_owned(),
        });
        return match action {
            ExternalResourceAction::Sandbox => "<iframe sandbox></iframe>".to_owned(),
            _ => "<div data-cditor-placeholder=\"embed\"></div>".to_owned(),
        };
    }

    let mut attrs = Vec::new();
    for attr in tag.attrs {
        if attr.name.starts_with("on") {
            *removed_event_handlers = removed_event_handlers.saturating_add(1);
            continue;
        }
        if is_url_attr(&attr.name) {
            match sanitize_url_attr(&tag.name, attr, policy, decisions) {
                Some(attr) => attrs.push(attr),
                None => {}
            }
        } else {
            attrs.push(attr);
        }
    }

    render_tag(&tag.name, &attrs, tag.self_closing)
}

fn sanitize_url_attr(
    tag_name: &str,
    attr: HtmlAttr,
    policy: ExternalContentPolicy,
    decisions: &mut Vec<ExternalResourceDecision>,
) -> Option<HtmlAttr> {
    let value = attr.value.as_deref().unwrap_or("").trim();
    if is_dangerous_url(value) {
        decisions.push(ExternalResourceDecision {
            kind: ExternalResourceKind::DangerousUrl,
            url: Some(value.to_owned()),
            action: ExternalResourceAction::Block,
            reason: "dangerous executable URL".to_owned(),
        });
        return None;
    }

    if value.starts_with("file://") {
        let action = match policy.file_url_policy {
            FileUrlPolicy::Block => ExternalResourceAction::Block,
            FileUrlPolicy::AskUser => ExternalResourceAction::AskUser,
        };
        decisions.push(ExternalResourceDecision {
            kind: ExternalResourceKind::FileUrl,
            url: Some(value.to_owned()),
            action,
            reason: "file:// requires local user confirmation".to_owned(),
        });
        return match action {
            ExternalResourceAction::AskUser => Some(HtmlAttr {
                name: format!("data-cditor-{}", attr.name),
                value: attr.value,
                quote: attr.quote,
            }),
            _ => None,
        };
    }

    if value.starts_with("data:") {
        let allowed = policy.data_url_policy == DataUrlPolicy::AllowImagesOnly
            && value.to_ascii_lowercase().starts_with("data:image/")
            && !value.to_ascii_lowercase().starts_with("data:image/svg+xml");
        decisions.push(ExternalResourceDecision {
            kind: ExternalResourceKind::DataUrl,
            url: Some(value.chars().take(80).collect()),
            action: if allowed {
                ExternalResourceAction::Allow
            } else {
                ExternalResourceAction::Block
            },
            reason: "data URL policy".to_owned(),
        });
        return allowed.then_some(attr);
    }

    if tag_name == "img" && is_remote_url(value) {
        let action = match policy.remote_image_policy {
            RemoteResourcePolicy::Block => ExternalResourceAction::Block,
            RemoteResourcePolicy::AskUser => ExternalResourceAction::AskUser,
            RemoteResourcePolicy::MetadataOnly => ExternalResourceAction::MetadataOnly,
            RemoteResourcePolicy::Allow => ExternalResourceAction::Allow,
        };
        decisions.push(ExternalResourceDecision {
            kind: ExternalResourceKind::RemoteImage,
            url: Some(value.to_owned()),
            action,
            reason: "remote image privacy/decode policy".to_owned(),
        });
        return match action {
            ExternalResourceAction::Allow => Some(attr),
            ExternalResourceAction::AskUser | ExternalResourceAction::MetadataOnly => {
                Some(HtmlAttr {
                    name: "data-cditor-remote-src".to_owned(),
                    value: attr.value,
                    quote: attr.quote,
                })
            }
            _ => None,
        };
    }

    Some(attr)
}

fn render_tag(name: &str, attrs: &[HtmlAttr], self_closing: bool) -> String {
    let mut out = String::new();
    out.push('<');
    out.push_str(name);
    for attr in attrs {
        out.push(' ');
        out.push_str(&attr.name);
        if let Some(value) = &attr.value {
            out.push('=');
            let quote = attr.quote.unwrap_or('"');
            out.push(quote);
            out.push_str(&escape_attr(value));
            out.push(quote);
        }
    }
    if self_closing {
        out.push_str(" />");
    } else {
        out.push('>');
    }
    out
}

fn is_url_attr(name: &str) -> bool {
    matches!(name, "src" | "href" | "action" | "poster")
}

fn is_dangerous_url(url: &str) -> bool {
    let normalized = url.trim_start().to_ascii_lowercase();
    normalized.starts_with("javascript:") || normalized.starts_with("vbscript:")
}

fn is_remote_url(url: &str) -> bool {
    let normalized = url.trim_start().to_ascii_lowercase();
    normalized.starts_with("http://") || normalized.starts_with("https://")
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xss_payload_paste_removes_script_event_handlers_and_dangerous_urls() {
        let html = r#"
            <div onclick="steal()">safe</div>
            <a href="javascript:alert(1)">bad</a>
            <script>alert('x')</script>
        "#;

        let sanitized = sanitize_external_html(html, ExternalContentPolicy::default());

        assert_eq!(sanitized.removed_scripts, 1);
        assert_eq!(sanitized.removed_event_handlers, 1);
        assert!(!sanitized.html.to_ascii_lowercase().contains("<script"));
        assert!(!sanitized.html.to_ascii_lowercase().contains("onclick"));
        assert!(!sanitized.html.to_ascii_lowercase().contains("javascript:"));
        assert!(sanitized.decisions.iter().any(|decision| {
            decision.kind == ExternalResourceKind::DangerousUrl
                && decision.action == ExternalResourceAction::Block
        }));
    }

    #[test]
    fn svg_paste_uses_placeholder_policy_by_default() {
        let html = r#"<p>a</p><svg><script>x()</script><circle /></svg><p>b</p>"#;

        let sanitized = sanitize_external_html(html, ExternalContentPolicy::default());

        assert!(sanitized.html.contains("data-cditor-placeholder=\"svg\""));
        assert!(!sanitized.html.to_ascii_lowercase().contains("<svg"));
        assert!(sanitized.decisions.iter().any(|decision| {
            decision.kind == ExternalResourceKind::Svg
                && decision.action == ExternalResourceAction::Placeholder
        }));
    }

    #[test]
    fn remote_image_paste_does_not_auto_load_under_privacy_policy() {
        let html = r#"<img src="https://tracker.example/a.png" onload="x()">"#;

        let sanitized = sanitize_external_html(html, ExternalContentPolicy::default());

        assert!(!sanitized.html.contains(" src="));
        assert!(sanitized.html.contains("data-cditor-remote-src"));
        assert_eq!(sanitized.removed_event_handlers, 1);
        assert!(sanitized.requires_user_confirmation());
        assert!(sanitized.decisions.iter().any(|decision| {
            decision.kind == ExternalResourceKind::RemoteImage
                && decision.action == ExternalResourceAction::AskUser
        }));
    }

    #[test]
    fn file_url_paste_requires_confirmation_and_is_not_uploaded_or_loaded() {
        let html = r#"<img src="file:///Users/me/private.png">"#;

        let sanitized = sanitize_external_html(html, ExternalContentPolicy::default());

        assert!(!sanitized.html.contains(" src="));
        assert!(sanitized.html.contains("data-cditor-src"));
        assert!(sanitized.decisions.iter().any(|decision| {
            decision.kind == ExternalResourceKind::FileUrl
                && decision.action == ExternalResourceAction::AskUser
        }));
    }

    #[test]
    fn iframe_embed_uses_placeholder_unless_sandbox_allowed() {
        let html = r#"<iframe src="https://example.com/embed"></iframe>"#;
        let sanitized = sanitize_external_html(html, ExternalContentPolicy::default());
        assert!(sanitized.html.contains("data-cditor-placeholder=\"embed\""));
        assert!(sanitized.requires_user_confirmation());

        let sandboxed = sanitize_external_html(
            html,
            ExternalContentPolicy {
                embed_policy: EmbedPolicy::AllowSandboxed,
                ..ExternalContentPolicy::default()
            },
        );
        assert!(sandboxed.html.contains("<iframe sandbox></iframe>"));
    }

    #[test]
    fn data_url_policy_blocks_svg_and_allows_non_svg_images_when_configured() {
        let html = r#"
            <img src="data:image/svg+xml,<svg onload='x()'></svg>">
            <img src="data:image/png;base64,AAAA">
        "#;
        let sanitized = sanitize_external_html(
            html,
            ExternalContentPolicy {
                data_url_policy: DataUrlPolicy::AllowImagesOnly,
                ..ExternalContentPolicy::default()
            },
        );

        assert!(!sanitized.html.contains("image/svg+xml"));
        assert!(sanitized.html.contains("image/png"));
    }
}
