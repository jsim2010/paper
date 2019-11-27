//! Implements functionality for the application while in filter mode.
use super::{Mode, LineNumber, Operation, Output, Sheet};
use core::{convert::TryFrom, num::TryFromIntError};
use crate::{Alert, ui::{Input, ENTER, ESC}};
use rec::{ChCls::{Any, Digit, End, /*Not,*/ Sign}, /*opt,*/ some, tkn, var, Element, Pattern};
use std::{cmp, /*collections::HashMap,*/ fmt::Debug, /*rc::Rc*/};

use lsp_msg::Range;

/// The [`Processor`] of the filter mode.
#[derive(Debug)]
pub(crate) struct Processor {
    /// All [`Range`]s that are being filtered.
    noises: Vec<Range>,
    /// All [`Range`]s that match the current filter.
    signals: Vec<Range>,
    ///// Matches the first feature of a filter.
    //first_feature_pattern: Pattern,
    ///// Filters supported by the application
    //filters: HashMap<char, Rc<dyn Filter>>,
}

impl Processor {
    /// Creates a new `Processor` for the filter mode.
    pub(crate) const fn new() -> Self {
        //let line_filter: Rc<dyn Filter> = Rc::new(LineFilter::default());
        //let pattern_filter: Rc<dyn Filter> = Rc::new(PatternFilter::default());

        Self {
            noises: Vec::new(),
            signals: Vec::new(),
            //first_feature_pattern: Pattern::new(tkn!(var(Not("&")) => "feature") + opt("&&")),
            //filters: [('#', line_filter), ('/', pattern_filter)]
            //    .iter()
            //    .cloned()
            //    .collect(),
        }
    }
}

impl super::Processor for Processor {
    fn decode(&self, _sheet: &Sheet, input: Input) -> Output<Vec<Operation>> {
        if let Input::Key(key) = input {
            Ok(vec![match key {
                ENTER => Operation::EnterMode(Mode::Action),
                ESC => Operation::EnterMode(Mode::Display),
                _ => Operation::AddToControlPanel(key),
            }])
        } else {
            Ok(vec![])
        }
                //if input == '\t' {
                //    pane.control_panel.add_non_bs('&');
                //    pane.control_panel.add_non_bs('&');
                //} else {
                //    pane.input_to_control_panel(input);
                //}

                //if let Some(last_feature) = self
                //    .first_feature_pattern
                //    .tokenize_iter(&pane.control_panel)
                //    .last()
                //    .and_then(|tokens| tokens.get("feature"))
                //{
                //    self.signals = self.noises.clone();

                //    if let Some(id) = last_feature.chars().nth(0) {
                //        if let Some(filter) = self.filters.get(&id) {
                //            filter.extract(last_feature, &mut self.signals, &pane)?;
                //        }
                //    }
                //}

                //pane.apply_filter(&self.noises, &self.signals);
                //OldOperation::maintain()
    }
}

/// Used for modifying [`Range`]s to match a feature.
trait Filter: Debug {
    /// Modifies `sections` such that it matches the given feature.
    fn extract(
        &self,
        feature: &str,
        sections: &mut Vec<Range>,
        pane: &Sheet,
    ) -> Result<(), Error>;
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
    fn extract(
        &self,
        feature: &str,
        sections: &mut Vec<Range>,
        _view: &Sheet,
    ) -> Result<(), Error> {
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
            tokens.parse::<isize>("movement"),
        ) {
            let end = origin.move_by(movement)?;
            let top = cmp::min(origin, end);
            let bottom = cmp::max(origin, end);

            sections.retain(|&x| {
                let row = x.start.line;
                row >= top.row() as u64 && row <= bottom.row() as u64
            })
        } else {
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
    fn extract(
        &self,
        feature: &str,
        ranges: &mut Vec<Range>,
        pane: &Sheet,
    ) -> Result<(), Error> {
        if let Some(user_pattern) = self.pattern.tokenize(feature).get("pattern") {
            if let Ok(search_pattern) = user_pattern.parse::<Pattern>() {
                let target_ranges = ranges.clone();
                ranges.clear();

                for target_range in target_ranges {
                    let start_character = usize::try_from(target_range.start.character)?;

                    if let Some(target) = pane
                        .line_data(LineNumber::try_from(target_range.start.line)?)
                        .map(|x| x.chars().skip(start_character).collect::<String>())
                    {
                        for location in search_pattern.locate_iter(&target) {
                            ranges.push(Range::with_partial_line(target_range.start.line, location.start() as u64, location.end() as u64));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Error for [`Filter::extract`].
struct Error;

impl From<Error> for Alert {
    fn from(_: Error) -> Self {
        Self::User
    }
}

impl From<()> for Error {
    fn from(_: ()) -> Self {
        Self{}
    }
}

impl From<TryFromIntError> for Error {
    fn from(_: TryFromIntError) -> Self {
        Self{}
    }
}
