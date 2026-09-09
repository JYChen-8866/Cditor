mod parser;
mod generator;
mod storage;
mod source_sync;

pub use storage::MarkdownStorage;
pub use parser::parse_markdown_to_blocks;
pub use generator::blocks_to_markdown;

use cditor_storage::StorageError;

pub type Result<T> = std::result::Result<T, StorageError>;
