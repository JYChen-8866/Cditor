use super::*;

#[test]
fn command_ids_require_stable_namespaces() {
    assert_eq!(
        CommandId::new("format.toggle_mark")
            .expect("valid id")
            .as_str(),
        "format.toggle_mark"
    );
    assert_eq!(
        CommandId::new("undo"),
        Err(CommandIdError::MissingNamespace)
    );
    assert_eq!(
        CommandId::new("Edit.undo"),
        Err(CommandIdError::InvalidSegmentStart)
    );
    assert_eq!(
        CommandId::new("edit..undo"),
        Err(CommandIdError::EmptySegment)
    );
    assert_eq!(
        CommandId::new("edit.undo!"),
        Err(CommandIdError::InvalidCharacter)
    );
}

#[test]
fn invocation_schema_roundtrips_without_losing_typed_arguments() {
    let invocation = CommandInvocation::new(
        CommandId::builtin(builtin::FORMAT_TOGGLE_MARK),
        CommandArgs::InlineMark(InlineMark::Bold),
        CommandSource::Toolbar,
    )
    .with_request_id(42);
    let encoded = serde_json::to_value(&invocation).expect("encode command");
    let decoded: CommandInvocation = serde_json::from_value(encoded).expect("decode command");

    assert_eq!(decoded, invocation);
    assert_eq!(decoded.args.kind(), CommandArgumentKind::InlineMark);
    assert_eq!(decoded.validate_schema(), Ok(()));
}

#[test]
fn unknown_command_schema_is_rejected_before_dispatch() {
    let mut invocation = CommandInvocation::new(
        CommandId::builtin(builtin::EDIT_UNDO),
        CommandArgs::None,
        CommandSource::Sdk,
    );
    invocation.schema_version += 1;

    let error = invocation.validate_schema().expect_err("schema must fail");
    assert_eq!(error.code, CommandErrorCode::UnsupportedSchema);
    assert_eq!(error.command_id.as_str(), builtin::EDIT_UNDO);
}

#[test]
fn query_state_distinguishes_checked_mixed_hidden_and_reason() {
    let checked = CommandQueryState::ENABLED.with_check(CommandCheckState::Checked);
    let mixed = CommandQueryState::ENABLED.with_check(CommandCheckState::Mixed);
    let disabled = CommandQueryState::disabled(CommandUnavailableReason::Readonly);
    let hidden = CommandQueryState::ENABLED.hidden();

    assert_eq!(checked.check, CommandCheckState::Checked);
    assert_eq!(mixed.check, CommandCheckState::Mixed);
    assert_eq!(disabled.reason, Some(CommandUnavailableReason::Readonly));
    assert_eq!(hidden.visibility, CommandVisibility::Hidden);
}

#[test]
fn command_outcome_preserves_transactions_and_affected_blocks() {
    let outcome = CommandOutcome::applied(vec![11, 12], vec![7, 8]);
    let encoded = serde_json::to_string(&outcome).expect("encode outcome");
    let decoded: CommandOutcome = serde_json::from_str(&encoded).expect("decode outcome");

    assert!(decoded.changed());
    assert_eq!(decoded.transaction_ids, vec![11, 12]);
    assert_eq!(decoded.affected_blocks, vec![7, 8]);
    assert!(!CommandOutcome::no_op().changed());
}

#[test]
fn extension_arguments_preserve_unknown_payloads() {
    let payload = serde_json::json!({
        "future_field": { "nested": [1, true, "value"] },
        "unknown_flag": 7
    });
    let invocation = CommandInvocation::new(
        CommandId::new("plugin.future_command").unwrap(),
        CommandArgs::Extension {
            schema: "plugin.example/v3".to_owned(),
            payload: payload.clone(),
        },
        CommandSource::Plugin,
    );
    let decoded: CommandInvocation =
        serde_json::from_value(serde_json::to_value(&invocation).unwrap()).unwrap();
    let CommandArgs::Extension {
        schema,
        payload: decoded_payload,
    } = decoded.args
    else {
        panic!("extension must stay opaque");
    };
    assert_eq!(schema, "plugin.example/v3");
    assert_eq!(decoded_payload, payload);
}

#[test]
fn unknown_closed_enum_variant_is_rejected() {
    let error = serde_json::from_str::<CommandSource>("\"future_source\"").unwrap_err();
    assert!(error.to_string().contains("unknown variant"));
}

#[test]
fn older_outcome_payload_uses_backward_compatible_defaults() {
    let outcome: CommandOutcome = serde_json::from_value(serde_json::json!({
        "status": "no_op"
    }))
    .unwrap();
    assert_eq!(outcome, CommandOutcome::no_op());
}
