use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionFocusTransition {
    NoComposition,
    PreservedSameSurface,
    CommittedAcrossSurface,
}

impl DocumentRuntime {
    pub fn prepare_input_focus_transition(
        &mut self,
        next_target: InputTarget,
    ) -> Result<CompositionFocusTransition, String> {
        let Some(editing) = self.editing.as_ref() else {
            return Ok(CompositionFocusTransition::NoComposition);
        };
        if editing.composition.is_none() {
            return Ok(CompositionFocusTransition::NoComposition);
        }
        let previous_target = editing.input_target;
        if previous_target == next_target {
            trace_input(
                "focus_transition.preserve_same_surface",
                format_args!("target={next_target:?}"),
            );
            return Ok(CompositionFocusTransition::PreservedSameSurface);
        }

        match self.commit_composition() {
            Ok(true) => {
                trace_input(
                    "focus_transition.commit_across_surface",
                    format_args!("from={previous_target:?} to={next_target:?}"),
                );
                Ok(CompositionFocusTransition::CommittedAcrossSurface)
            }
            Ok(false) => {
                let error = format!(
                    "active composition was not committed before focus transition from {previous_target:?} to {next_target:?}"
                );
                trace_input("focus_transition.commit_failed", &error);
                Err(error)
            }
            Err(source) => {
                let error = format!(
                    "failed to commit composition before focus transition from {previous_target:?} to {next_target:?}: {source}"
                );
                trace_input("focus_transition.commit_failed", &error);
                Err(error)
            }
        }
    }

    pub fn commit_composition_before_external_focus(&mut self) -> Result<bool, String> {
        if self
            .editing
            .as_ref()
            .is_none_or(|editing| editing.composition.is_none())
        {
            return Ok(false);
        }
        match self.commit_composition()? {
            true => Ok(true),
            false => Err("active composition was not committed before external focus".to_owned()),
        }
    }
}
