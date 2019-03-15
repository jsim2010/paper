use crate::Mrc;
use super::{Section, IndexType, Length, EditableString, Pane, LineNumber, Operation, Name, Initiation, Output};
use crate::ui::{Change, Edit, Region, Color, ESC, ENTER};
use rec::{tkn, opt, Element, Pattern, some, var};
use rec::ChCls::{Digit, End, Sign, Any, Not};
use std::fmt::Debug;
use std::cmp;
use try_from::{TryFrom, TryFromIntError};

#[derive(Debug)]
pub(crate) struct Processor {
    filter: EditableString,
    noises: Vec<Section>,
    signals: Vec<Section>,
    first_feature_pattern: Pattern,
    filters: PaperFilters,
    pane: Mrc<Pane>,
}

impl Processor {
    pub(crate) fn new(pane: &Mrc<Pane>) -> Self {
        Self {
            filter: EditableString::new(),
            noises: Vec::new(),
            signals: Vec::new(),
            first_feature_pattern: Pattern::define(tkn!(var(Not("&")) => "feature") + opt("&&")),
            filters: PaperFilters::default(),
            pane: pane.clone(),
        }
    }
}

impl super::Processor for Processor {
    fn enter(&mut self, initiation: Option<Initiation>) -> Output<Vec<Edit>> {
        self.filter.clear();

        if let Some(Initiation::StartFilter(c)) = initiation {
            // TODO: Currently we assume that c is not BACKSPACE.
            self.filter.add_non_bs(c);
        }

        self.noises.clear();

        for line in 1..=self.pane.borrow().line_count {
            if let Some(noise) = LineNumber::new(line).map(Section::line) {
                self.noises.push(noise);
            }
        }

        Ok(self.filter.edits())
    }

    fn decode(&mut self, input: char) -> Output<Operation> {
        let pane: &Pane = &self.pane.borrow();

        return match input {
            ENTER => {
                Ok(Operation::EnterMode(Name::Action, Some(Initiation::SetSignals(self.signals.clone()))))
            }
            ESC => {
                Ok(Operation::EnterMode(Name::Display, None))
            }
            _ => {
                if input == '\t' {
                    self.filter.add_non_bs('&');
                    self.filter.add_non_bs('&');
                } else if !self.filter.add(input) {
                    return Ok(Operation::EditUi(self.filter.flash_edits()));
                }

                if let Some(last_feature) = self.first_feature_pattern.tokenize_iter(&self.filter).last().and_then(|tokens| tokens.get("feature")) {
                    self.signals = self.noises.clone();

                    if let Some(id) = last_feature.chars().nth(0) {
                        for filter in self.filters.iter() {
                            if id == filter.id() {
                                filter.extract(last_feature, &mut self.signals, pane)?;
                                break;
                            }
                        }
                    }
                }

                let mut edits = self.filter.edits();

                for row in 0..pane.height {
                    edits.push(Edit::new(Region::with_row(row).unwrap(), Change::Format(Color::Default)));
                }

                for noise in &self.noises {
                    if let Some(region) = pane.region_at(noise) {
                        edits.push(Edit::new(region, Change::Format(Color::Blue)));
                    }
                }

                for signal in &self.signals {
                    if let Some(region) = pane.region_at(signal) {
                        edits.push(Edit::new(region, Change::Format(Color::Red)));
                    }
                }

                Ok(Operation::EditUi(edits))
            }
        }
    }
}

/// Signifies all of the [`Filters`] used by the application.
#[derive(Debug, Default)]
struct PaperFilters {
    /// The [`Filter`] that matches lines.
    line: LineFilter,
    /// The [`Filter`] that matches patterns.
    pattern: PatternFilter,
}

impl PaperFilters {
    /// Returns the [`Iterator`] of [`Filters`].
    fn iter(&self) -> PaperFiltersIter<'_> {
        PaperFiltersIter {
            index: 0,
            filters: self,
        }
    }
}

/// Signifies an [`Iterator`] through all of the [`Filters`].
struct PaperFiltersIter<'a> {
    /// The current index of the iteration.
    index: usize,
    /// The filters to be iterated.
    filters: &'a PaperFilters,
}

impl<'a> Iterator for PaperFiltersIter<'a> {
    type Item = &'a dyn Filter;

    fn next(&mut self) -> Option<Self::Item> {
        self.index += 1;

        match self.index {
            1 => Some(&self.filters.line),
            2 => Some(&self.filters.pattern),
            _ => None,
        }
    }
}

/// Used for modifying [`Section`]s to match a feature.
trait Filter: Debug {
    /// Returns the identifying character of the `Filter`.
    fn id(&self) -> char;
    /// Modifies `sections` such that it matches the given feature.
    fn extract(
        &self,
        feature: &str,
        sections: &mut Vec<Section>,
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
            pattern: Pattern::define(
                "#" + ((tkn!(some(Digit) => "line") + End)
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
        sections: &mut Vec<Section>,
        _view: &Pane,
    ) -> Result<(), TryFromIntError> {
        let tokens = self.pattern.tokenize(feature);

        if let Ok(line) = tokens.parse::<LineNumber>("line") {
            sections.retain(|&x| x.start.line == line);
        } else if let (Ok(start), Ok(end)) = (
            tokens.parse::<LineNumber>("start"),
            tokens.parse::<LineNumber>("end"),
        ) {
            let top = cmp::min(start, end);
            let bottom = cmp::max(start, end);

            sections.retain(|&x| {
                let row = x.start.line;
                row >= top && row <= bottom
            })
        } else if let (Ok(origin), Ok(movement)) = (
            tokens.parse::<LineNumber>("origin"),
            tokens.parse::<IndexType>("movement"),
        ) {
            let end = origin + movement;
            let top = cmp::min(origin, end);
            let bottom = cmp::max(origin, end);

            sections.retain(|&x| {
                let row = x.start.line;
                row >= top && row <= bottom
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
            pattern: Pattern::define("/" + tkn!(some(Any) => "pattern")),
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
        sections: &mut Vec<Section>,
        pane: &Pane,
    ) -> Result<(), TryFromIntError> {
        if let Some(user_pattern) = self.pattern.tokenize(feature).get("pattern") {
            if let Ok(search_pattern) = Pattern::load(user_pattern) {
                let target_sections = sections.clone();
                sections.clear();

                for target_section in target_sections {
                    let start = usize::try_from(target_section.start.column)?;

                    if let Some(target) = pane
                        .line(target_section.start.line)
                        .map(|x| x.chars().skip(start).collect::<String>())
                    {
                        for location in search_pattern.locate_iter(&target) {
                            sections.push(Section {
                                start: target_section.start
                                    >> IndexType::try_from(location.start())?,
                                length: Length::try_from(location.length())?,
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
