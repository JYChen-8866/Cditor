use cditor_core::ids::BlockId;
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};

pub(super) fn apply_error(message: String) -> ProtocolError {
    ProtocolError::new(ProtocolErrorCode::ApplyFailed, message)
}

pub(super) fn missing_table_error(block_id: BlockId) -> ProtocolError {
    ProtocolError::new(
        ProtocolErrorCode::ApplyFailed,
        format!("block {block_id} is not a loaded table"),
    )
}

pub(super) fn validate_expected_revision(
    expected: Option<u64>,
    current: u64,
) -> Result<(), ProtocolError> {
    if let Some(expected) = expected
        && expected != current
    {
        return Err(ProtocolError::new(
            ProtocolErrorCode::StalePrecondition,
            format!("command expected revision {expected}, current revision is {current}"),
        ));
    }
    Ok(())
}
