use std::fmt;

pub type PostgresStorageResult<T> = Result<T, PostgresStorageError>;

#[derive(Debug)]
pub enum PostgresStorageError {
    Sqlx(sqlx::Error),
    Migration(String),
    SchemaVersionMismatch { expected: u32, found: u32 },
    NotFound { entity: &'static str, id: String },
    CorruptData { message: String },
    Serialization(serde_json::Error),
    Io(std::io::Error),
    Busy,
    Conflict { message: String },
    RetryExhausted { task_id: String },
}

impl fmt::Display for PostgresStorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlx(error) => write!(f, "postgres error: {error}"),
            Self::Migration(message) => write!(f, "migration error: {message}"),
            Self::SchemaVersionMismatch { expected, found } => {
                write!(
                    f,
                    "schema version mismatch: expected {expected}, found {found}"
                )
            }
            Self::NotFound { entity, id } => write!(f, "{entity} not found: {id}"),
            Self::CorruptData { message } => write!(f, "corrupt data: {message}"),
            Self::Serialization(error) => write!(f, "serialization error: {error}"),
            Self::Io(error) => write!(f, "io error: {error}"),
            Self::Busy => write!(f, "storage is busy"),
            Self::Conflict { message } => write!(f, "storage conflict: {message}"),
            Self::RetryExhausted { task_id } => write!(f, "retry exhausted for task {task_id}"),
        }
    }
}

impl std::error::Error for PostgresStorageError {}

impl From<sqlx::Error> for PostgresStorageError {
    fn from(error: sqlx::Error) -> Self {
        Self::Sqlx(error)
    }
}

impl From<serde_json::Error> for PostgresStorageError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}

impl From<std::io::Error> for PostgresStorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
