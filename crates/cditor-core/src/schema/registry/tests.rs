use super::*;
use crate::rich_text::CalloutVariant;

fn all_kinds() -> Vec<RichBlockKind> {
    use RichBlockKind as Kind;
    let mut kinds = vec![
        Kind::Paragraph,
        Kind::Quote,
        Kind::Callout {
            variant: CalloutVariant::Warning,
        },
        Kind::Todo { checked: true },
        Kind::BulletedList,
        Kind::NumberedList,
        Kind::Toggle,
        Kind::Code {
            language: Some("rust".to_owned()),
        },
        Kind::Math,
        Kind::Mermaid,
        Kind::Html,
        Kind::Table,
        Kind::ColumnsGroup,
        Kind::Column,
        Kind::Image,
        Kind::File,
        Kind::Attachment,
        Kind::Whiteboard,
        Kind::MindMap,
        Kind::Embed,
        Kind::Divider,
        Kind::Separator,
        Kind::Database,
        Kind::FootnoteDefinition,
        Kind::Comment,
        Kind::RawMarkdown,
    ];
    for level in 1..=6 {
        kinds.push(Kind::Heading { level });
    }
    kinds
}

#[test]
fn every_builtin_kind_has_a_registered_descriptor() {
    let registry = BlockRegistry::builtin();
    for kind in all_kinds() {
        let tag = kind_tag_for_rich_block_kind(&kind);
        assert!(
            registry.is_known(tag),
            "kind {kind:?} (tag {tag}) unregistered"
        );
        let descriptor = registry.descriptor_for_kind(&kind);
        assert_eq!(descriptor.kind_tag, tag);
        assert!(
            !descriptor.capabilities.lossless_unknown || matches!(kind, RichBlockKind::Custom(_))
        );
    }
}

#[test]
fn unknown_tag_falls_back_to_lossless_placeholder() {
    let registry = BlockRegistry::builtin();
    let descriptor = registry.descriptor_by_tag(31_337);
    assert_eq!(descriptor.name, "unknown");
    assert!(descriptor.capabilities.lossless_unknown);
    assert!(
        !descriptor.capabilities.text_surface,
        "unknown block must not be editable"
    );
    assert!(descriptor.capabilities.stable_box);
    assert!(!registry.is_known(31_337));
}

#[test]
fn capability_spot_checks_match_product_semantics() {
    let registry = BlockRegistry::builtin();
    let paragraph = registry.descriptor_for_kind(&RichBlockKind::Paragraph);
    assert!(paragraph.capabilities.text_surface);
    assert!(paragraph.capabilities.inline_marks);
    assert!(paragraph.capabilities.plain_text_conversion_target);
    assert!(
        registry
            .descriptor_for_kind(&RichBlockKind::Whiteboard)
            .capabilities
            .plain_text_conversion_target
    );

    let code = registry.descriptor_for_kind(&RichBlockKind::Code { language: None });
    assert!(code.capabilities.text_surface);
    assert!(!code.capabilities.inline_marks, "code has no rich marks");
    assert!(code.capabilities.internal_virtualization, "P10-002 target");

    let table = registry.descriptor_for_kind(&RichBlockKind::Table);
    assert!(
        !table.capabilities.text_surface,
        "cells own text, not the table shell"
    );
    assert!(table.capabilities.inner_selection);
    assert!(
        registry
            .descriptor_for_kind(&RichBlockKind::Html)
            .capabilities
            .text_surface
    );
    assert!(
        !registry
            .descriptor_for_kind(&RichBlockKind::Math)
            .capabilities
            .text_surface
    );
    assert!(
        registry
            .descriptor_for_kind(&RichBlockKind::Image)
            .capabilities
            .caption
    );
    assert!(
        !registry
            .descriptor_for_kind(&RichBlockKind::Image)
            .capabilities
            .plain_text_conversion_target
    );
    assert!(
        registry
            .descriptor_for_kind(&RichBlockKind::Toggle)
            .capabilities
            .container
    );
    for kind in [
        RichBlockKind::Quote,
        RichBlockKind::BulletedList,
        RichBlockKind::NumberedList,
    ] {
        assert!(registry.descriptor_for_kind(&kind).capabilities.container);
    }
}

#[test]
fn duplicate_registration_is_rejected() {
    let mut registry = BlockRegistry::builtin();
    let error = registry
        .register(BlockDescriptor {
            kind_tag: 1,
            name: "imposter-paragraph",
            default_kind: RichBlockKind::Paragraph,
            payload_version: CURRENT_BLOCK_PAYLOAD,
            capabilities: BlockCapabilities::empty(),
            migrator: None,
        })
        .unwrap_err();
    assert_eq!(
        error,
        RegistryError::DuplicateTag {
            tag: 1,
            existing: "paragraph"
        }
    );
}

#[test]
fn migrate_payload_uses_descriptor_migrator() {
    fn rename_field(
        mut body: serde_json::Value,
        _from: SchemaVersion,
    ) -> Result<serde_json::Value, String> {
        let object = body.as_object_mut().ok_or("expected object")?;
        let value = object.remove("old_name").ok_or("missing old_name")?;
        object.insert("new_name".to_owned(), value);
        Ok(body)
    }

    let mut registry = BlockRegistry::builtin();
    registry
        .register(BlockDescriptor {
            kind_tag: 40_000,
            name: "migratable",
            default_kind: RichBlockKind::Custom("migratable".to_owned()),
            payload_version: SchemaVersion::new(2, 0),
            capabilities: BlockCapabilities::empty(),
            migrator: Some(rename_field),
        })
        .unwrap();

    let migrated = registry
        .migrate_payload(
            40_000,
            serde_json::json!({"old_name": 7}),
            SchemaVersion::new(1, 0),
        )
        .unwrap();
    assert_eq!(migrated, serde_json::json!({"new_name": 7}));
    let same = registry
        .migrate_payload(
            40_000,
            serde_json::json!({"x": 1}),
            SchemaVersion::new(2, 0),
        )
        .unwrap();
    assert_eq!(same, serde_json::json!({"x": 1}));
    let error = registry
        .migrate_payload(1, serde_json::json!({}), SchemaVersion::new(0, 1))
        .unwrap_err();
    assert!(matches!(error, RegistryError::NoMigrator { tag: 1, .. }));
}
