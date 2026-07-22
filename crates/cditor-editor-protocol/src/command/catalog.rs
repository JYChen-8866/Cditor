//! Registry metadata and validation for command producers.

use std::collections::BTreeMap;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandMutability {
    ReadOnly,
    Document,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDefinition {
    pub id: CommandId,
    pub argument_kind: CommandArgumentKind,
    pub mutability: CommandMutability,
    pub creates_undo_boundary: bool,
}

impl CommandDefinition {
    pub fn new(
        id: &'static str,
        argument_kind: CommandArgumentKind,
        mutability: CommandMutability,
        creates_undo_boundary: bool,
    ) -> Self {
        Self {
            id: CommandId::builtin(id),
            argument_kind,
            mutability,
            creates_undo_boundary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandCatalogRegistrationError {
    pub id: CommandId,
}

impl fmt::Display for CommandCatalogRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "command {} is already registered", self.id)
    }
}

impl std::error::Error for CommandCatalogRegistrationError {}

#[derive(Debug, Clone, Default)]
pub struct CommandCatalog {
    definitions: BTreeMap<CommandId, CommandDefinition>,
}

impl CommandCatalog {
    pub fn builtin() -> Self {
        let mut catalog = Self::default();
        for definition in builtin_definitions() {
            catalog
                .register(definition)
                .expect("builtin command ids are unique");
        }
        catalog
    }

    pub fn register(
        &mut self,
        definition: CommandDefinition,
    ) -> Result<(), CommandCatalogRegistrationError> {
        if self.definitions.contains_key(&definition.id) {
            return Err(CommandCatalogRegistrationError { id: definition.id });
        }
        self.definitions.insert(definition.id.clone(), definition);
        Ok(())
    }

    pub fn definition(&self, id: &CommandId) -> Option<&CommandDefinition> {
        self.definitions.get(id)
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    pub fn validate_invocation(&self, invocation: &CommandInvocation) -> Result<(), CommandError> {
        invocation.validate_schema()?;
        let Some(definition) = self.definition(&invocation.id) else {
            return Err(CommandError::new(
                invocation.id.clone(),
                CommandErrorCode::UnknownCommand,
                "command is not registered",
            ));
        };
        let actual = invocation.args.kind();
        if actual != definition.argument_kind {
            return Err(CommandError::new(
                invocation.id.clone(),
                CommandErrorCode::InvalidArguments,
                format!(
                    "expected {:?} arguments, got {actual:?}",
                    definition.argument_kind
                ),
            ));
        }
        Ok(())
    }
}

fn builtin_definitions() -> Vec<CommandDefinition> {
    use CommandArgumentKind as Args;
    use CommandMutability::{Document, ReadOnly};
    use builtin::*;

    vec![
        definition(EDIT_UNDO, Args::None, Document),
        definition(EDIT_REDO, Args::None, Document),
        definition(EDIT_SELECT_ALL, Args::None, ReadOnly),
        definition(EDIT_COPY, Args::None, ReadOnly),
        definition(EDIT_CUT, Args::None, Document),
        definition(EDIT_PASTE, Args::None, Document),
        definition(EDIT_DELETE_SELECTION, Args::None, Document),
        definition(TEXT_INSERT, Args::Text, Document),
        definition(TEXT_REPLACE, Args::ReplaceText, Document),
        definition(TEXT_DELETE_BACKWARD, Args::None, Document),
        definition(TEXT_DELETE_FORWARD, Args::None, Document),
        definition(TEXT_MOVE_CARET, Args::MoveCaret, ReadOnly),
        definition(TEXT_INSERT_SOFT_BREAK, Args::None, Document),
        definition(FORMAT_TOGGLE_MARK, Args::InlineMark, Document),
        definition(FORMAT_TOGGLE_BOLD, Args::InlineMark, Document),
        definition(FORMAT_TOGGLE_ITALIC, Args::InlineMark, Document),
        definition(FORMAT_TOGGLE_UNDERLINE, Args::InlineMark, Document),
        definition(FORMAT_TOGGLE_STRIKE, Args::InlineMark, Document),
        definition(FORMAT_TOGGLE_INLINE_CODE, Args::InlineMark, Document),
        definition(FORMAT_SET_COLOR, Args::InlineColor, Document),
        definition(BLOCK_SET_COLOR, Args::BlockColor, Document),
        definition(BLOCK_INSERT, Args::InsertBlock, Document),
        definition(BLOCK_INSERT_AFTER, Args::BlockTarget, Document),
        definition(BLOCK_INSERT_AFTER_FOCUSED, Args::None, Document),
        definition(BLOCK_TRANSFORM, Args::BlockKind, Document),
        definition(BLOCK_DELETE, Args::BlockTarget, Document),
        definition(BLOCK_DELETE_SELECTED, Args::None, Document),
        definition(BLOCK_DUPLICATE, Args::BlockTarget, Document),
        definition(BLOCK_DUPLICATE_SELECTED, Args::None, Document),
        definition(BLOCK_COPY_TEXT, Args::BlockTarget, ReadOnly),
        definition(BLOCK_ENTER, Args::None, Document),
        definition(BLOCK_INDENT, Args::None, Document),
        definition(BLOCK_OUTDENT, Args::None, Document),
        definition(BLOCK_FOLD, Args::BlockTarget, Document),
        definition(BLOCK_UNFOLD, Args::BlockTarget, Document),
        definition(BLOCK_TOGGLE_TODO, Args::BlockTarget, Document),
        definition(BLOCK_APPLY_SLASH, Args::SlashBlock, Document),
        definition(BLOCK_INSERT_TABLE, Args::InsertTable, Document),
        definition(BLOCK_INSERT_IMAGE, Args::BlockKind, Document),
        definition(BLOCK_INSERT_WHITEBOARD, Args::BlockKind, Document),
        definition(BLOCK_INSERT_MERMAID, Args::BlockKind, Document),
        definition(HEADING_FOLD, Args::None, Document),
        definition(HEADING_UNFOLD, Args::None, Document),
        definition(CODE_SET_LANGUAGE, Args::CodeLanguage, Document),
        definition(TABLE_INSERT, Args::InsertTable, Document),
        definition(TABLE_INSERT_AXIS, Args::TableInsertAxis, Document),
        definition(TABLE_DELETE_AXIS, Args::TableAxisTarget, Document),
        definition(TABLE_DUPLICATE_AXIS, Args::TableAxisTarget, Document),
        definition(TABLE_TOGGLE_HEADER, Args::TableAxisTarget, Document),
        definition(TABLE_CLEAR_RANGE, Args::TableRangeTarget, Document),
        definition(TABLE_SET_RANGE_COLOR, Args::TableRangeColor, Document),
        definition(TABLE_MERGE_CELLS, Args::TableRangeTarget, Document),
        definition(TABLE_SPLIT_CELL, Args::TableRangeTarget, Document),
        definition(TABLE_RESIZE_AXIS, Args::TableAxisResize, Document),
        definition(TABLE_MOVE_AXIS, Args::TableAxisMove, Document),
        definition(MEDIA_SET_WIDTH_RATIO, Args::MediaWidthRatio, Document),
        definition(AI_APPLY, Args::AiApply, Document),
    ]
}

fn definition(
    id: &'static str,
    argument_kind: CommandArgumentKind,
    mutability: CommandMutability,
) -> CommandDefinition {
    CommandDefinition::new(
        id,
        argument_kind,
        mutability,
        mutability == CommandMutability::Document,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_has_unique_valid_ids() {
        let definitions = builtin_definitions();
        let catalog = CommandCatalog::builtin();
        assert_eq!(catalog.len(), definitions.len());
        assert!(catalog.len() >= 50);
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut catalog = CommandCatalog::default();
        let definition = CommandDefinition::new(
            builtin::EDIT_UNDO,
            CommandArgumentKind::None,
            CommandMutability::Document,
            true,
        );
        catalog.register(definition.clone()).unwrap();
        assert_eq!(
            catalog.register(definition).unwrap_err().id.as_str(),
            builtin::EDIT_UNDO
        );
    }

    #[test]
    fn invocation_argument_kind_must_match_definition() {
        let catalog = CommandCatalog::builtin();
        let invalid = CommandInvocation::new(
            CommandId::builtin(builtin::TEXT_MOVE_CARET),
            CommandArgs::None,
            CommandSource::Sdk,
        );
        let error = catalog.validate_invocation(&invalid).unwrap_err();
        assert_eq!(error.code, CommandErrorCode::InvalidArguments);

        let valid = CommandInvocation::new(
            CommandId::builtin(builtin::TEXT_MOVE_CARET),
            CommandArgs::MoveCaret {
                direction: CaretDirection::NextVisual,
                extend_selection: false,
            },
            CommandSource::Keyboard,
        );
        assert_eq!(catalog.validate_invocation(&valid), Ok(()));
    }

    #[test]
    fn unknown_command_is_rejected_before_dispatch() {
        let catalog = CommandCatalog::builtin();
        let invocation = CommandInvocation::new(
            CommandId::new("plugin.missing").unwrap(),
            CommandArgs::None,
            CommandSource::Plugin,
        );
        assert_eq!(
            catalog.validate_invocation(&invocation).unwrap_err().code,
            CommandErrorCode::UnknownCommand
        );
    }
}
