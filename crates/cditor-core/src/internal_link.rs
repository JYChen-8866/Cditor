use crate::ids::{BlockId, DocumentId};

const DOCUMENT_LINK_PREFIX: &str = "cditor://document/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalDocumentLink {
    pub document_id: DocumentId,
    pub block_id: Option<BlockId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLinkPresentation {
    pub href: String,
    pub label: String,
}

impl BlockLinkPresentation {
    pub fn new(href: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            href: href.into(),
            label: label.into(),
        }
    }

    pub fn plain(href: impl Into<String>) -> Self {
        let href = href.into();
        Self {
            label: href.clone(),
            href,
        }
    }
}

pub fn document_link(document_id: DocumentId) -> String {
    format!("{DOCUMENT_LINK_PREFIX}{document_id}")
}

pub fn block_link(document_id: DocumentId, block_id: BlockId) -> String {
    format!("{DOCUMENT_LINK_PREFIX}{document_id}/block/{block_id}")
}

pub fn parse_document_link(href: &str) -> Option<InternalDocumentLink> {
    let path = href.strip_prefix(DOCUMENT_LINK_PREFIX)?;
    let mut segments = path.split('/');
    let document_id = segments.next()?.parse().ok()?;
    let block_id = match (segments.next(), segments.next(), segments.next()) {
        (None, None, None) => None,
        (Some("block"), Some(block_id), None) => Some(block_id.parse().ok()?),
        _ => return None,
    };
    Some(InternalDocumentLink {
        document_id,
        block_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_and_block_links_round_trip() {
        assert_eq!(document_link(7), "cditor://document/7");
        assert_eq!(block_link(7, 9), "cditor://document/7/block/9");
        assert_eq!(
            parse_document_link(&block_link(7, 9)),
            Some(InternalDocumentLink {
                document_id: 7,
                block_id: Some(9),
            })
        );
    }

    #[test]
    fn parser_rejects_noncanonical_or_incomplete_links() {
        for href in [
            "https://example.com",
            "Cditor://document/7/block/9",
            "cditor://document/title",
            "cditor://document/7/block",
            "cditor://document/7/block/9/extra",
            "cditor://document/7/other/9",
            "cditor://document/7?block=9",
            " cditor://document/7/block/9",
        ] {
            assert_eq!(parse_document_link(href), None, "accepted {href:?}");
        }
    }
}
