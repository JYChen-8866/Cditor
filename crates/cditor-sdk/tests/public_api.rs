use std::sync::Arc;

use cditor_sdk::command::{CditorCommand, CommandSource};
use cditor_sdk::document::{DocumentInfo, SaveStatus};
use cditor_sdk::event::CditorEvent;
use cditor_sdk::providers::{AiProvider, AssetInput, CditorExtension};
use cditor_sdk::{Cditor, CditorError, CditorOptions};

fn accepts_builder(builder: Cditor) -> CditorOptions {
    builder.into_options()
}

fn accepts_ai_provider(_: Arc<dyn AiProvider>) {}

fn accepts_extension(_: &dyn CditorExtension) {}

#[test]
fn framework_free_sdk_surface_compiles_for_external_consumers() {
    let options = accepts_builder(
        Cditor::new()
            .memory()
            .with_document_id(42)
            .with_readonly(true)
            .without_autosave(),
    );
    assert_eq!(options.document_id, Some(42));
    assert!(options.readonly);
    assert_eq!(options.autosave_interval, None);

    let command = CditorCommand::Undo;
    assert_eq!(command.stable_id(), "edit.undo");
    let source = CommandSource::Sdk;
    assert_eq!(source, CommandSource::Sdk);

    let info = DocumentInfo {
        document_id: 42,
        title: Some("SDK".to_owned()),
        revision: 7,
        block_count: 3,
        readonly: false,
    };
    let event = CditorEvent::Ready {
        document: info.clone(),
    };
    assert!(matches!(event, CditorEvent::Ready { document } if document == info));
    assert_eq!(SaveStatus::LocallySaved, SaveStatus::LocallySaved);

    let asset = AssetInput {
        name: "image.png".to_owned(),
        media_type: Some("image/png".to_owned()),
        bytes: vec![1, 2, 3],
    };
    assert_eq!(asset.bytes.len(), 3);
    let error = CditorError::InvalidInput("bad input".to_owned());
    assert!(error.to_string().contains("bad input"));

    let provider_slot: Option<Arc<dyn AiProvider>> = None;
    if let Some(provider) = provider_slot {
        accepts_ai_provider(provider);
    }
    let extension_slot: Option<&dyn CditorExtension> = None;
    if let Some(extension) = extension_slot {
        accepts_extension(extension);
    }
}

#[test]
fn public_sdk_does_not_export_gpui_component_types() {
    let source = include_str!("../src/lib.rs");
    for forbidden in [
        "CditorComponent",
        "CditorHandle",
        "CditorViewContract",
        "CditorViewFactory",
        "gpui",
    ] {
        assert!(!source.contains(forbidden), "SDK leaked {forbidden}");
    }
}
