use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacingToken {
    None,
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}
