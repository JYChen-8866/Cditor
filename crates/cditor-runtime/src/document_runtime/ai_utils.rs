use super::*;

pub(super) fn bounded_prefix(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

pub(super) fn bounded_suffix(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut start = value.len() - max_bytes;
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    value[start..].to_owned()
}

pub(super) fn ai_selection_fingerprint(
    target: &RuntimeAiTarget,
    versions: &[(BlockId, u64)],
) -> u64 {
    let mut value = 0xcbf29ce484222325u64;
    let mut mix = |part: u64| {
        value ^= part;
        value = value.wrapping_mul(0x100000001b3);
    };
    match target {
        RuntimeAiTarget::InlineCaret(position) => {
            mix(1);
            mix(position.block_id);
            mix(position.offset as u64);
        }
        RuntimeAiTarget::TextSelection(selection) => {
            mix(2);
            mix(selection.anchor.block_id);
            mix(selection.anchor.offset as u64);
            mix(selection.focus.block_id);
            mix(selection.focus.offset as u64);
        }
    }
    for (block_id, version) in versions {
        mix(*block_id);
        mix(*version);
    }
    value
}
