//! Mermaid 块的默认内容。
//!
//! 新建的 Mermaid 块预置一段简短示例：插入后立刻能渲染出图，用户直接改这段
//! 源码即可，不用先猜 mermaid 语法。

use super::*;

/// 新建 Mermaid 块的示例源码。短到一眼看完，且是最常用的流程图语法。
pub(super) const MERMAID_EXAMPLE_SOURCE: &str = "flowchart LR\n  A[开始] --> B[处理] --> C[结束]";

/// 空的 Mermaid 块用示例填充；用户已经输入了内容就保留他的内容。
pub(super) fn mermaid_payload_from_text(text: &str) -> BlockPayload {
    let source = if text.trim().is_empty() {
        MERMAID_EXAMPLE_SOURCE
    } else {
        text
    };
    BlockPayload::RichText {
        spans: vec![InlineSpan::plain(source)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_mermaid_blocks_start_from_the_example() {
        assert_eq!(
            mermaid_payload_from_text("").plain_text(),
            MERMAID_EXAMPLE_SOURCE
        );
        assert_eq!(
            mermaid_payload_from_text("   ").plain_text(),
            MERMAID_EXAMPLE_SOURCE
        );
    }

    #[test]
    fn the_example_is_short_and_renders_a_flowchart() {
        assert!(MERMAID_EXAMPLE_SOURCE.starts_with("flowchart "));
        assert_eq!(MERMAID_EXAMPLE_SOURCE.lines().count(), 2);
    }

    #[test]
    fn existing_text_survives_the_conversion() {
        assert_eq!(
            mermaid_payload_from_text("sequenceDiagram").plain_text(),
            "sequenceDiagram"
        );
    }
}
