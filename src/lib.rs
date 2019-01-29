//! A terminal-based editor with goals to maximize simplicity and efficiency.
//!
//! This project is very much in an alpha state.
//!
//! Its features include:
//! - Modal editing (keys implement different functionality depending on the current mode).
//! - Extensive but relatively simple filter grammar that allows user to select any text.
//!
//! Future items on the Roadmap:
//! - Add more filter grammar.
//! - Implement suggestions for commands to improve user experience.
//! - Support Language Server Protocol.
//!
//! # Usage
//!
//! To use paper, install and run the binary. If you are developing a rust crate that runs paper,
//! then create and run an instance by calling the following:

// Lint checks currently not defined: missing_doc_code_examples, variant_size_differences
#![warn(
    rust_2018_idioms,
    future_incompatible,
    unused,
    box_pointers,
    macro_use_extern_crate,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    single_use_lifetimes,
    trivial_casts,
    trivial_numeric_casts,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes,
    unused_qualifications,
    unused_results
)]
#![warn(
    clippy::use_debug,
    clippy::option_unwrap_used,
    clippy::integer_arithmetic
)]
#![doc(html_root_url = "https://docs.rs/paper/0.2.0")]

mod engine;
mod ui;

use crate::engine::{Controller, Notice};
use crate::ui::{Address, Change, Color, Edit, Length, Region, UserInterface, END};
use rec::ChCls::{Any, Digit, End, Sign};
use rec::{Element, tkn, some, Pattern};
use std::cmp::{self, Ordering};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::fs;
use std::iter;
use std::ops::{Add, AddAssign, Shr, SubAssign};

/// The paper application.
// In general, Paper methods should contain as little logic as possible. Instead all logic should
// be included in Operations.
#[derive(Debug, Default)]
pub struct Paper {
    /// User interface of the application.
    ui: UserInterface,
    controller: Controller,
    /// Data of the file being edited.
    view: View,
    /// Characters being edited to be analyzed by the application.
    sketch: String,
    /// [`Section`]s of the view that match the current filter.
    ///
    /// [`Section`]: .struct.Section.html
    signals: Vec<Section>,
    noises: Vec<Section>,
    marks: Vec<Mark>,
    filters: PaperFilters,
    sketch_additions: String,
}

impl Paper {
    /// Creates a new paper application.
    #[inline]
    pub fn new() -> Paper {
        Default::default()
    }

    /// Runs the application.
    #[inline]
    pub fn run(&mut self) -> Result<(), String> {
        self.ui.init()?;
        let operations = engine::Operations::default();

        'main: loop {
            for opcode in self.controller.process_input(self.ui.receive_input()) {
                match operations.execute(self, opcode)? {
                    Some(Notice::Quit) => break 'main,
                    Some(Notice::Flash) => {
                        self.ui.flash()?;
                    }
                    None => {}
                }
            }
        }

        self.ui.close()?;
        Ok(())
    }

    /// Displays the view on the user interface.
    fn display_view(&self) -> Result<(), String> {
        for edit in self.view.redraw_edits().take(self.ui.grid_height()) {
            self.ui.apply(edit)?;
        }

        Ok(())
    }

    fn change_view(&mut self, path: &str) {
        self.view = View::with_file(String::from(path));
        self.noises.clear();

        for line in 1..=self.view.line_count {
            if let Some(noise) = LineNumber::new(line).map(Section::line) {
                self.noises.push(noise);
            }
        }
    }

    fn save_view(&self) {
        self.view.put();
    }

    fn reduce_noise(&mut self) {
        self.noises = self.signals.clone();
    }

    fn filter_signals(&mut self, feature: &str) {
        self.signals = self.noises.clone();

        if let Some(id) = feature.chars().nth(0) {
            for filter in self.filters.iter() {
                if id == filter.id() {
                    filter.extract(feature, &mut self.signals, &self.view);
                    break;
                }
            }
        }
    }

    fn sketch(&self) -> &String {
        &self.sketch
    }

    fn sketch_mut(&mut self) -> &mut String {
        &mut self.sketch
    }

    fn draw_popup(&self) -> Result<(), String> {
        self.ui
            .apply(Edit::new(Region::row(0), Change::Row(self.sketch.clone())))
    }

    fn clear_background(&self) -> Result<(), String> {
        for row in 0..self.ui.grid_height() {
            self.format_region(Region::row(row), Color::Default)?;
        }

        Ok(())
    }

    fn set_marks(&mut self, edge: Edge) {
        self.marks.clear();

        for signal in self.signals.iter() {
            let mut place = signal.start;

            if edge == Edge::End {
                let length = signal.length;

                place.index += match length {
                    END => self.view.line_length(&signal.start).unwrap_or_default(),
                    _ => length.into_usize(),
                };
            }

            self.marks.push(Mark {
                place,
                pointer: place.index
                    + Pointer(match place.line.index() {
                        0 => Some(0),
                        index => self
                            .view
                            .data
                            .match_indices(ui::ENTER)
                            .nth(index - 1)
                            .map(|x| x.0 + 1),
                    }),
            });
        }
    }

    fn scroll(&mut self, movement: isize) {
        self.view.scroll(movement);
    }

    fn draw_filter_backgrounds(&self) -> Result<(), String> {
        for noise in self.noises.iter() {
            self.format_section(noise, Color::Blue)?;
        }

        for signal in self.signals.iter() {
            self.format_section(signal, Color::Red)?;
        }

        Ok(())
    }

    /// Sets the [`Color`] of a [`Section`].
    fn format_section(&self, section: &Section, color: Color) -> Result<(), String> {
        // It is okay for region_at() to return None; this just means section is not displayed.
        if let Some(region) = self.view.region_at(section) {
            self.format_region(region, color)?;
        }

        Ok(())
    }

    fn format_region(&self, region: Region, color: Color) -> Result<(), String> {
        self.ui.apply(Edit::new(region, Change::Format(color)))
    }

    fn update_view(&mut self, c: char) -> Result<(), String> {
        let mut adjustment: Adjustment = Default::default();

        for mark in self.marks.iter_mut() {
            if let Some(new_adjustment) = Adjustment::create(c, &mark.place, &self.view) {
                adjustment += new_adjustment;

                if adjustment.change != Change::Clear {
                    if let Some(region) = self.view.region_at(&mark.place) {
                        self.ui
                            .apply(Edit::new(region, adjustment.change.clone()))?;
                    }
                }

                mark.adjust(&adjustment);
                self.view.add(mark, c);
            }
        }

        if adjustment.change == Change::Clear {
            self.view.clean();
            self.display_view()?;
        }

        Ok(())
    }

    fn change_mode(&mut self, mode: engine::Mode) {
        self.controller.set_mode(mode);
    }

    /// Returns the height used for scrolling.
    fn scroll_height(&self) -> usize {
        self.ui.grid_height() / 4
    }
}

#[derive(Debug, Default)]
struct PaperFilters {
    line: LineFilter,
    pattern: PatternFilter,
}

impl PaperFilters {
    fn iter(&self) -> PaperFiltersIter<'_> {
        PaperFiltersIter {
            index: 0,
            filters: self,
        }
    }
}

struct PaperFiltersIter<'a> {
    index: usize,
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

#[derive(Clone, Debug, Default)]
struct View {
    data: String,
    first_line: LineNumber,
    margin_width: usize,
    line_count: usize,
    path: String,
}

impl View {
    fn with_file(path: String) -> View {
        let mut view = View {
            data: fs::read_to_string(path.as_str()).unwrap().replace('\r', ""),
            path,
            ..Default::default()
        };

        view.clean();
        view
    }

    fn add(&mut self, mark: &Mark, c: char) {
        if let Some(index) = mark.pointer.0 {
            match c {
                ui::BACKSPACE => {
                    // For now, do not care to check what is removed. But this may become important for
                    // multi-byte characters.
                    match self.data.remove(index) {
                        _ => {}
                    }
                }
                _ => {
                    self.data.insert(index - 1, c);
                }
            }
        }
    }

    fn address_at(&self, place: &Place) -> Option<Address> {
        place.line.diff(self.first_line).map(|x| Address::new(x, place.index + self.margin_width))
    }

    fn region_at<T: RegionWrapper>(&self, region_wrapper: &T) -> Option<Region> {
        self.address_at(&region_wrapper.start()).map(|address| Region::new(address, region_wrapper.length()))
    }

    fn redraw_edits(&self) -> impl Iterator<Item = Edit> + '_ {
        // Clear the screen, then add each row.
        iter::once(Edit::new(Default::default(), Change::Clear)).chain(
            self.lines()
                .enumerate()
                .skip(self.first_line.index())
                .map(move |x| {
                    Edit::new(
                        Region::row(x.0),
                        Change::Row(format!(
                            "{:>width$} {}",
                            x.0 + 1,
                            x.1,
                            width = self.margin_width - 1
                        )),
                    )
                }),
        )
    }

    fn lines(&self) -> std::str::Lines<'_> {
        self.data.lines()
    }

    fn line(&self, line_number: LineNumber) -> Option<&str> {
        self.lines().nth(line_number.index())
    }

    fn clean(&mut self) {
        self.line_count = self.lines().count();
        self.margin_width = ((self.line_count + 1) as f32).log10().ceil() as usize + 1;
    }

    fn scroll(&mut self, movement: isize) {
        self.first_line = cmp::min(
            self.first_line.shift(movement).unwrap_or_default(),
            LineNumber::new(self.line_count).unwrap_or_default(),
        );
    }

    fn line_length(&self, place: &Place) -> Option<usize> {
        self.line(place.line).map(|x| x.len())
    }

    fn put(&self) {
        fs::write(&self.path, &self.data).unwrap();
    }
}

#[derive(Clone, Debug, Default)]
struct Adjustment {
    shift: isize,
    line_change: isize,
    indexes_changed: HashMap<LineNumber, isize>,
    change: Change,
}

impl Adjustment {
    fn new(line: LineNumber, shift: isize, index_change: isize, change: Change) -> Adjustment {
        let line_change = if change == Change::Clear { shift } else { 0 };

        Adjustment {
            shift,
            line_change,
            indexes_changed: [(line.shift(line_change).unwrap(), index_change)]
                .iter()
                .cloned()
                .collect(),
            change,
        }
    }

    fn create(c: char, place: &Place, view: &View) -> Option<Adjustment> {
        match c {
            ui::BACKSPACE => {
                if place.index == 0 {
                    view.line_length(place)
                        .map(|x| Adjustment::new(place.line, -1, x as isize, Change::Clear))
                } else {
                    Some(Adjustment::new(place.line, -1, -1, Change::Backspace))
                }
            }
            ui::ENTER => Some(Adjustment::new(
                place.line,
                1,
                -(place.index as isize),
                Change::Clear,
            )),
            _ => Some(Adjustment::new(place.line, 1, 1, Change::Insert(c))),
        }
    }
}

impl AddAssign for Adjustment {
    fn add_assign(&mut self, other: Adjustment) {
        self.shift += other.shift;
        self.line_change += other.line_change;

        for (line, change) in other.indexes_changed {
            *self.indexes_changed.entry(line).or_default() += change;
        }

        if self.change != Change::Clear {
            self.change = other.change
        }
    }
}

/// Indicates a specific Place of a given Section.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
enum Edge {
    /// Indicates the first Place of the Section.
    Start,
    /// Indicates the last Place of the Section.
    End,
}

impl Default for Edge {
    fn default() -> Edge {
        Edge::Start
    }
}

impl Display for Edge {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Edge::Start => write!(f, "Starting edge"),
            Edge::End => write!(f, "Ending edge"),
        }
    }
}

/// An address and its respective pointer in a view.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
struct Mark {
    /// Pointer in view that corresponds with mark.
    pointer: Pointer,
    /// Place of mark.
    place: Place,
}

impl Mark {
    fn adjust(&mut self, adjustment: &Adjustment) {
        if -adjustment.shift < self.pointer {
            self.pointer += adjustment.shift;
            self.place.line = self.place.line.shift(adjustment.line_change).unwrap();

            for (&line, &change) in adjustment.indexes_changed.iter() {
                if line == self.place.line {
                    self.place.index = (self.place.index as isize + change) as usize;
                }
            }
        }
    }
}

impl Display for Mark {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}{}", self.place, self.pointer)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Debug)]
struct Pointer(Option<usize>);

impl PartialEq<isize> for Pointer {
    fn eq(&self, other: &isize) -> bool {
        match self.0 {
            Some(value) => (value as isize) == *other,
            None => false,
        }
    }
}

impl PartialOrd<isize> for Pointer {
    fn partial_cmp(&self, other: &isize) -> Option<Ordering> {
        self.0.map(|x| (x as isize).cmp(other))
    }
}

impl Add<Pointer> for usize {
    type Output = Pointer;

    #[inline]
    fn add(self, other: Pointer) -> Pointer {
        Pointer(other.0.map(|x| x + self))
    }
}

impl Add<usize> for Pointer {
    type Output = Pointer;

    fn add(self, other: usize) -> Pointer {
        Pointer(self.0.map(|x| x + other))
    }
}

impl SubAssign<usize> for Pointer {
    fn sub_assign(&mut self, other: usize) {
        self.0 = self.0.map(|x| x - other);
    }
}

impl AddAssign<isize> for Pointer {
    fn add_assign(&mut self, other: isize) {
        self.0 = self.0.map(|x| (x as isize + other) as usize);
    }
}

impl Default for Pointer {
    fn default() -> Pointer {
        Pointer(Some(0))
    }
}

impl Display for Pointer {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "[{}]",
            match self.0 {
                None => String::from("None"),
                Some(i) => format!("{}", i),
            }
        )
    }
}

impl PartialEq<Pointer> for isize {
    fn eq(&self, other: &Pointer) -> bool {
        other == self
    }
}

impl PartialOrd<Pointer> for isize {
    fn partial_cmp(&self, other: &Pointer) -> Option<Ordering> {
        other.partial_cmp(self).map(|x| x.reverse())
    }
}

trait RegionWrapper {
    fn start(&self) -> Place;
    fn length(&self) -> Length;
}

/// Signifies adjacent [`Place`]s.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Section {
    start: Place,
    length: Length,
}

impl Section {
    /// Creates a new `Section` that signifies an entire line.
    #[inline]
    fn line(line: LineNumber) -> Section {
        Section {
            start: Place { line, index: 0 },
            length: END,
        }
    }
}

impl RegionWrapper for Section {
    fn start(&self) -> Place {
        self.start
    }

    fn length(&self) -> Length {
        self.length
    }
}

impl Display for Section {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}->{}", self.start, self.length)
    }
}

#[derive(Clone, Debug, Default)]
struct RelativePlace {
    line: LineNumber,
    index: isize,
}

/// Signifies the location of a character within a view.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Place {
    line: LineNumber,
    index: usize,
}

impl RegionWrapper for Place {
    fn start(&self) -> Place {
        *self
    }

    fn length(&self) -> Length {
        Length::from(1)
    }
}

impl Shr<usize> for Place {
    type Output = Place;

    #[inline]
    fn shr(self, rhs: usize) -> Place {
        Place {
            index: self.index + rhs,
            ..self
        }
    }
}

impl Display for Place {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "ln {}, idx {}", self.line, self.index)
    }
}

/// Signifies a line number.
#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Debug)]
struct LineNumber(usize);

impl LineNumber {
    fn new(value: usize) -> Option<LineNumber> {
        match value {
            0 => None,
            _ => Some(LineNumber(value)),
        }
    }

    #[allow(clippy::integer_arithmetic)] // Integer arithmetic is okay because self.0 > 0 by definition of new().
    fn index(self) -> usize {
        self.0 - 1
    }

    fn shift(self, movement: isize) -> Option<LineNumber> {
        let new_value = if movement < 0 {
            movement.checked_neg().and_then(|x| self.0.checked_sub(x as usize))
        } else {
            self.0.checked_add(movement as usize)
        };

        new_value.and_then(LineNumber::new)
    }

    fn diff(self, other: LineNumber) -> Option<usize> {
        self.0.checked_sub(other.0)
    }
}

impl Display for LineNumber {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.0)
    }
}

impl Default for LineNumber {
    #[inline]
    fn default() -> LineNumber {
        LineNumber(1)
    }
}

impl std::str::FromStr for LineNumber {
    type Err = ParseLineNumberError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LineNumber::new(s.parse::<usize>()?).ok_or(ParseLineNumberError::InvalidValue)
    }
}

#[derive(Debug)]
enum ParseLineNumberError {
    InvalidValue,
    ParseInt(std::num::ParseIntError),
}

impl std::error::Error for ParseLineNumberError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            ParseLineNumberError::InvalidValue => None,
            ParseLineNumberError::ParseInt(ref err) => Some(err),
        }
    }
}

impl Display for ParseLineNumberError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match *self {
            ParseLineNumberError::InvalidValue => write!(f, "Invalid line number provided."),
            ParseLineNumberError::ParseInt(ref err) => write!(f, "{}", err),
        }
    }
}

impl From<std::num::ParseIntError> for ParseLineNumberError {
    fn from(error: std::num::ParseIntError) -> ParseLineNumberError {
        ParseLineNumberError::ParseInt(error)
    }
}

trait Filter: Debug {
    fn id(&self) -> char;
    fn extract(&self, feature: &str, sections: &mut Vec<Section>, view: &View);
}

#[derive(Debug)]
struct LineFilter {
    pattern: Pattern,
}

impl Default for LineFilter {
    fn default() -> LineFilter {
        LineFilter {
            pattern: Pattern::define(
                "#" + ((tkn!(some(Digit) => "line") + End)
                    | (tkn!(some(Digit) => "start") + "." + tkn!(some(Digit) => "end"))
                    | (tkn!(some(Digit) => "origin")
                        + tkn!(Sign + some(Digit) => "movement"))),
            ),
        }
    }
}

impl Filter for LineFilter {
    fn id(&self) -> char {
        '#'
    }

    fn extract(&self, feature: &str, sections: &mut Vec<Section>, _view: &View) {
        let tokens = self.pattern.tokenize(feature);

        if let Ok(line) = tokens.parse::<LineNumber>("line") {
            sections.retain(|&x| x.start.line == line);
        } else if let (Ok(start), Ok(end)) = (tokens.parse::<LineNumber>("start"), tokens.parse::<LineNumber>("end")) {
            let top = cmp::min(start, end);
            let bottom = cmp::max(start, end);

            sections.retain(|&x| {
                let row = x.start.line;
                row >= top && row <= bottom
            })
        } else if let (Ok(origin), Ok(movement)) = (tokens.parse::<LineNumber>("origin"), tokens.parse::<isize>("movement")) {
            let end = origin.shift(movement).unwrap_or_default();
            let top = cmp::min(origin, end);
            let bottom = cmp::max(origin, end);

            sections.retain(|&x| {
                let row = x.start.line;
                row >= top && row <= bottom
            })
        }
    }
}

#[derive(Debug)]
struct PatternFilter {
    pattern: Pattern,
}

impl Default for PatternFilter {
    fn default() -> PatternFilter {
        PatternFilter {
            pattern: Pattern::define("/" + tkn!(some(Any) => "pattern")),
        }
    }
}

impl Filter for PatternFilter {
    fn id(&self) -> char {
        '/'
    }

    fn extract(&self, feature: &str, sections: &mut Vec<Section>, view: &View) {
        if let Some(user_pattern) = self.pattern.tokenize(feature).get("pattern") {
            if let Ok(search_pattern) = Pattern::load(user_pattern) {
                let target_sections = sections.clone();
                sections.clear();

                for target_section in target_sections {
                    if let Some(target) = view.line(target_section.start.line).map(|x| {
                        x.chars()
                            .skip(target_section.start.index)
                            .collect::<String>()
                    }) {
                        for location in search_pattern.locate_iter(&target) {
                            sections.push(Section {
                                start: target_section.start >> location.start(),
                                length: Length::from(location.length()),
                            });
                        }
                    }
                }
            }
        }
    }
}
