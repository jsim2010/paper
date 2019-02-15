//! Implements the [`Operation`] to filter signals.
use crate::engine::{opt, tkn, var, Element, Not, OpCode, Operation, Output, Paper, Pattern};

/// Sets signals to match filters described in the sketch.
#[derive(Clone, Debug)]
pub(crate) struct Op {
    /// The [`Pattern`] that matches the first feature.
    first_feature_pattern: Pattern,
}

impl Op {
    /// Creates a new `Op`.
    pub(crate) fn new() -> Self {
        Self {
            first_feature_pattern: Pattern::define(tkn!(var(Not("&")) => "feature") + opt("&&")),
        }
    }
}

impl Operation for Op {
    fn name(&self) -> String {
        String::from("FilterSignals")
    }

    fn operate(&self, paper: &mut Paper<'_>, _opcode: OpCode) -> Output {
        let filter = paper.sketch().clone();

        if let Some(last_feature) = self
            .first_feature_pattern
            .tokenize_iter(&filter)
            .last()
            .and_then(|x| x.get("feature"))
        {
            paper.filter_signals(last_feature)?;
        }

        paper.clear_background()?;
        paper.draw_filter_backgrounds()?;
        Ok(None)
    }
}
