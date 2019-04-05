//! Implements functionality for the application while in filter mode.
use super::{Initiation, LineNumber, Operation, Output, Pane};
use crate::{ptr::Mrc, ui::{ENTER, ESC}};
use lsp_types::{Position, Range};
use rec::{ChCls::{Any, Digit, End, Not, Sign}, opt, some, tkn, var, Element, Pattern};
use std::{cmp, collections::HashMap, fmt::Debug, rc::Rc};
use try_from::{TryFrom, TryFromIntError};

/// The [`Processor`] of the filter mode.
#[derive(Debug)]
pub(crate) struct Processor {
    /// All [`Range`]s that are being filtered.
    noises: Vec<Range>,
    /// All [`Range`]s that match the current filter.
    signals: Vec<Range>,
    /// Matches the first feature of a filter.
    first_feature_pattern: Pattern,
    /// Filters supported by the application
    filters: HashMap<char, Rc<dyn Filter>>,
    /// The [`Pane`] of the application.
    pane: Mrc<Pane>,
}

impl Processor {
    /// Creates a new `Processor` for the filter mode.
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        let line_filter: Rc<dyn Filter> = Rc::new(LineFilter::default());
        let pattern_filter: Rc<dyn Filter> = Rc::new(PatternFilter::default());

        Self {
            noises: Vec::new(),
            signals: Vec::new(),
            first_feature_pattern: Pattern::new(tkn!(var(Not("&")) => "feature") + opt("&&")),
            filters: [('#', line_filter), ('/', pattern_filter)]
                .iter()
                .cloned()
                .collect(),
            pane: Mrc::clone(pane),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, initiation: &Option<Initiation>) -> Output<()> {
        let mut pane = self.pane.borrow_mut();

        let id = if let Some(Initiation::StartFilter(c)) = *initiation {
            Some(c)
        } else {
            None
        };
        pane.reset_control_panel(id);

        self.noises.clear();

        for line in 0..pane.line_count {
            self.noises.push(Range::new(
                Position::new(line, 0),
                Position::new(line, u64::max_value()),
            ));
        }

        Ok(())
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        let mut pane = self.pane.borrow_mut();

        match input {
            ENTER => Ok(Operation::enter_action(self.signals.clone())),
            ESC => Ok(Operation::enter_display()),
            _ => {
                if input == '\t' {
                    pane.control_panel.add_non_bs('&');
                    pane.control_panel.add_non_bs('&');
                } else {
                    pane.input_to_control_panel(input);
                }

                if let Some(last_feature) = self
                    .first_feature_pattern
                    .tokenize_iter(&pane.control_panel)
                    .last()
                    .and_then(|tokens| tokens.get("feature"))
                {
                    self.signals = self.noises.clone();

                    if let Some(id) = last_feature.chars().nth(0) {
                        if let Some(filter) = self.filters.get(&id) {
                            filter.extract(last_feature, &mut self.signals, &pane)?;
                        }
                    }
                }

                pane.apply_filter(&self.noises, &self.signals);
                Ok(Operation::maintain())
            }
        }
    }
}

/// Used for modifying [`Range`]s to match a feature.
trait Filter: Debug {
    /// Returns the identifying character of the `Filter`.
    fn id(&self) -> char;
    /// Modifies `sections` such that it matches the given feature.
    fn extract(
        &self,
        feature: &str,
        sections: &mut Vec<Range>,
        pane: &Pane,
    ) -> Result<(), TryFromIntError>;
}

/// The [`Filter`] used to match a line.
#[derive(Debug)]
struct LineFilter {
    /// The [`Pattern`] used to match one or more [`LineNumber`]s.
    pattern: Pattern,
}

impl Default for LineFilter {
    fn default() -> Self {
        Self {
            pattern: Pattern::new(
                "#" + ((tkn!(some(Digit) => "line") + var(".") + End)
                    | (tkn!(some(Digit) => "start") + "." + tkn!(some(Digit) => "end"))
                    | (tkn!(some(Digit) => "origin") + tkn!(Sign + some(Digit) => "movement"))),
            ),
        }
    }
}

impl Filter for LineFilter {
    fn id(&self) -> char {
        '#'
    }

    fn extract(
        &self,
        feature: &str,
        sections: &mut Vec<Range>,
        _view: &Pane,
    ) -> Result<(), TryFromIntError> {
        let tokens = self.pattern.tokenize(feature);

        if let Ok(line) = tokens.parse::<LineNumber>("line") {
            sections.retain(|&x| x.start.line == line.row() as u64);
        } else if let (Ok(start), Ok(end)) = (
            tokens.parse::<LineNumber>("start"),
            tokens.parse::<LineNumber>("end"),
        ) {
            let top = cmp::min(start, end);
            let bottom = cmp::max(start, end);

            sections.retain(|&x| {
                let row = x.start.line;
                row >= top.row() as u64 && row <= bottom.row() as u64
            })
        } else if let (Ok(origin), Ok(movement)) = (
            tokens.parse::<LineNumber>("origin"),
            tokens.parse::<i128>("movement"),
        ) {
            let end = origin + movement;
            let top = cmp::min(origin, end);
            let bottom = cmp::max(origin, end);

            sections.retain(|&x| {
                let row = x.start.line;
                row >= top.row() as u64 && row <= bottom.row() as u64
            })
        }

        Ok(())
    }
}

/// A [`Filter`] that extracts matches of a [`Pattern`].
#[derive(Debug)]
struct PatternFilter {
    /// The [`Pattern`] used to match patterns.
    pattern: Pattern,
}

impl Default for PatternFilter {
    fn default() -> Self {
        Self {
            pattern: Pattern::new("/" + tkn!(some(Any) => "pattern")),
        }
    }
}

impl Filter for PatternFilter {
    fn id(&self) -> char {
        '/'
    }

    fn extract(
        &self,
        feature: &str,
        ranges: &mut Vec<Range>,
        pane: &Pane,
    ) -> Result<(), TryFromIntError> {
        if let Some(user_pattern) = self.pattern.tokenize(feature).get("pattern") {
            if let Ok(search_pattern) = user_pattern.parse::<Pattern>() {
                let target_ranges = ranges.clone();
                ranges.clear();

                for target_range in target_ranges {
                    let start_character = usize::try_from(target_range.start.character)?;
                    let line_index = usize::try_from(target_range.start.line)?;

                    if let Some(target) = pane
                        .line_data(line_index)
                        .map(|x| x.chars().skip(start_character).collect::<String>())
                    {
                        for location in search_pattern.locate_iter(&target) {
                            let mut new_range = target_range;
                            new_range.start.character += location.start() as u64;
                            new_range.end.character += location.end() as u64;
                            ranges.push(new_range);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
