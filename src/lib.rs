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
//!
//! ```ignore
//! extern crate paper;
//!
//! use paper::Paper;
//!
//! fn main() {
//!     let mut paper = Paper::new();
//!
//!     paper.run();
//! }
//! ```

#![doc(html_root_url = "https://docs.rs/paper/0.1.0")]

mod ui;

use crate::ui::{Address, Change, Edit, Length, Region, UserInterface};
use rec::{Atom, ChCls, Pattern, Quantifier, OPT, SOME, VAR};
use std::cmp;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::iter::once;
use std::ops::{Add, AddAssign, Shr, SubAssign};

/// The paper application.
#[derive(Debug, Default)]
pub struct Paper {
    /// User interface of the application.
    ui: UserInterface,
    /// Current mode of the application.
    mode: Mode,
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
    patterns: PaperPatterns,
    filters: PaperFilters,
}

impl Paper {
    /// Creates a new paper application.
    pub fn new() -> Paper {
        Default::default()
    }

    /// Runs the application.
    pub fn run(&mut self) {
        self.ui.init();

        'main: loop {
            for operation in self.mode.handle_input(self.ui.receive_input()) {
                if let Some(notice) = operation.operate(self) {
                    match notice {
                        Notice::Quit => break 'main,
                    }
                }
            }
        }

        self.ui.close();
    }

    /// Displays the view on the user interface.
    fn display_view(&self) {
        for edit in self.view.redraw_edits().take(self.ui.pane_height()) {
            self.ui.apply(edit);
        }
    }

    /// Returns the height used for scrolling.
    fn scroll_height(&self) -> usize {
        self.ui.pane_height() / 4
    }
}

#[derive(Debug, Default)]
struct PaperFilters {
    line: LineFilter,
    pattern: PatternFilter,
}

impl PaperFilters {
    fn iter(&self) -> PaperFiltersIter {
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
    type Item = &'a Filter;

    fn next(&mut self) -> Option<Self::Item> {
        self.index += 1;

        match self.index {
            1 => Some(&self.filters.line),
            2 => Some(&self.filters.pattern),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct PaperPatterns {
    command: Pattern,
    see: Pattern,
    first_feature: Pattern,
}

impl Default for PaperPatterns {
    fn default() -> PaperPatterns {
        PaperPatterns {
            command: Pattern::define(
                ChCls::Any.rpt(SOME.lazy()).name("command") + (ChCls::WhSpc | ChCls::End),
            ),
            see: Pattern::define("see" + ChCls::WhSpc.rpt(SOME) + ChCls::Any.rpt(VAR).name("path")),
            first_feature: Pattern::define(
                ChCls::None("&").rpt(VAR).name("feature") + "&&".rpt(OPT),
            ),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct View {
    data: String,
    origin: RelativePlace,
    line_count: usize,
    path: String,
}

impl View {
    fn with_file(path: String) -> View {
        let mut view = View {
            data: fs::read_to_string(path.as_str()).unwrap().replace('\r', ""),
            origin: RelativePlace { line: 1, index: 0 },
            path: path,
            ..Default::default()
        };

        view.clean();
        view
    }

    fn add(&mut self, mark: &Mark, c: char) {
        let index = mark.pointer.to_usize();

        match c {
            ui::BACKSPACE => {
                self.data.remove(index);
            }
            _ => {
                self.data.insert(index - 1, c);
            }
        }
    }

    fn redraw_edits<'a>(&'a self) -> impl Iterator<Item = Edit> + 'a {
        // Clear the screen, then add each row.
        once(Edit::new(Default::default(), Change::Clear)).chain(
            self.lines()
                .skip(self.origin.line - 1)
                .enumerate()
                .map(move |x| {
                    Edit::new(
                        Region::row(x.0),
                        Change::Row(format!(
                            "{:>width$} {}",
                            self.origin.line + x.0,
                            x.1,
                            width = (-self.origin.index - 1) as usize
                        )),
                    )
                }),
        )
    }

    fn lines(&self) -> std::str::Lines {
        self.data.lines()
    }

    fn line(&self, line_number: usize) -> Option<&str> {
        self.lines().nth(line_number - 1)
    }

    fn clean(&mut self) {
        self.line_count = self.lines().count();
        self.origin.index = -(((self.line_count + 1) as f32).log10().ceil() as isize + 1);
    }

    fn scroll_down(&mut self, scroll: usize) {
        self.origin.line = cmp::min(self.origin.line + scroll, self.line_count);
    }

    fn scroll_up(&mut self, scroll: usize) {
        if self.origin.line <= scroll {
            self.origin.line = 1;
        } else {
            self.origin.line -= scroll;
        }
    }

    fn line_length(&self, place: &Place) -> usize {
        self.line(place.line).unwrap().len()
    }

    fn put(&self) {
        fs::write(&self.path, &self.data).unwrap();
    }
}

#[derive(Clone, Debug, Default)]
struct Adjustment {
    shift: isize,
    line_change: isize,
    indexes_changed: HashMap<usize, isize>,
    change: Change,
}

impl Adjustment {
    fn new(line: usize, shift: isize, index_change: isize, change: Change) -> Adjustment {
        let line_change = if change == Change::Clear { shift } else { 0 };

        Adjustment {
            shift,
            line_change,
            indexes_changed: [((line as isize + line_change) as usize, index_change)]
                .iter()
                .cloned()
                .collect(),
            change,
        }
    }

    fn create(c: char, place: &Place, view: &View) -> Adjustment {
        match c {
            ui::BACKSPACE => {
                if place.index == 0 {
                    Adjustment::new(
                        place.line,
                        -1,
                        view.line_length(place) as isize,
                        Change::Clear,
                    )
                } else {
                    Adjustment::new(place.line, -1, -1, Change::Backspace)
                }
            }
            ui::ENTER => Adjustment::new(place.line, 1, -(place.index as isize), Change::Clear),
            _ => Adjustment::new(place.line, 1, 1, Change::Insert(c)),
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

impl fmt::Display for Edge {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
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
        if -adjustment.shift < self.pointer.to_isize() {
            self.pointer += adjustment.shift;
            self.place.line = (self.place.line as isize + adjustment.line_change) as usize;

            for (&line, &change) in adjustment.indexes_changed.iter() {
                if line == self.place.line {
                    self.place.index = (self.place.index as isize + change) as usize;
                }
            }
        }

    }
}

impl fmt::Display for Mark {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.place, self.pointer)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct Pointer(Option<usize>);

impl Pointer {
    fn to_usize(&self) -> usize {
        self.0.unwrap()
    }

    fn to_isize(&self) -> isize {
        self.0.unwrap() as isize
    }
}

impl Add<Pointer> for usize {
    type Output = Pointer;

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

impl fmt::Display for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
struct Section {
    start: Place,
    length: Length,
}

impl Section {
    pub fn line(line: usize) -> Section {
        Section {
            start: Place { line, index: 0 },
            length: ui::EOL,
        }
    }

    fn to_region(&self, origin: &RelativePlace) -> Option<Region> {
        self.start
            .to_address(origin)
            .map(|x| Region::new(x, self.length))
    }
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}->{}", self.start, self.length)
    }
}

#[derive(Clone, Debug, Default)]
struct RelativePlace {
    line: usize,
    index: isize,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
struct Place {
    line: usize,
    index: usize,
}

impl Place {
    fn to_address(&self, origin: &RelativePlace) -> Option<Address> {
        if self.line < origin.line {
            None
        } else {
            Some(Address::new(
                self.line - origin.line,
                (self.index as isize - origin.index) as usize,
            ))
        }
    }

    fn to_region(&self, origin: &RelativePlace) -> Option<Region> {
        self.to_address(origin)
            .map(|x| Region::new(x, Length::from(1)))
    }
}

impl Shr<usize> for Place {
    type Output = Place;

    fn shr(self, rhs: usize) -> Place {
        Place {
            index: self.index + rhs,
            ..self
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ln {}, idx {}", self.line, self.index)
    }
}

trait Operation {
    fn operate(&self, paper: &mut Paper) -> Option<Notice>;
}

struct ChangeMode(Mode);

impl Operation for ChangeMode {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        paper.mode = self.0;

        match paper.mode {
            Mode::Display => {
                paper.display_view();
            }
            Mode::Command | Mode::Filter => {
                paper.marks.clear();
                paper.sketch.clear();
            }
            Mode::Action => {}
            Mode::Edit => {
                paper.display_view();
                paper.sketch.clear();
            }
        }

        None
    }
}

struct ExecuteCommand;

impl Operation for ExecuteCommand {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        match paper
            .patterns
            .command
            .tokenize(&paper.sketch)
            .get("command")
        {
            Some("see") => match paper.patterns.see.tokenize(&paper.sketch).get("path") {
                Some(path) => {
                    paper.view = View::with_file(String::from(path));
                    paper.noises.clear();

                    for line in 1..=paper.view.line_count {
                        paper.noises.push(Section::line(line));
                    }
                }
                None => {}
            },
            Some("put") => {
                paper.view.put();
            }
            Some("end") => return Some(Notice::Quit),
            Some(_) | None => {}
        }

        None
    }
}

struct IdentifyNoise;

impl Operation for IdentifyNoise {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        let mut sections = Vec::new();

        for line in 1..=paper.view.line_count {
            sections.push(Section::line(line));
        }

        for tokens in paper.patterns.first_feature.tokenize_iter(&paper.sketch) {
            if let Some(feature) = tokens.get("feature") {
                if let Some(id) = feature.chars().nth(0) {
                    for filter in paper.filters.iter() {
                        if id == filter.id() {
                            filter.extract(feature, &mut sections, &paper.view);
                            break;
                        }
                    }
                }
            }
        }

        paper.noises.clear();

        for section in sections {
            if let Some(region) = section.to_region(&paper.view.origin) {
                paper.ui.apply(Edit::new(region, Change::Format(2)));
            }

            paper.noises.push(section);
        }

        None
    }
}

struct AddToSketch(String);

impl Operation for AddToSketch {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        for c in self.0.chars() {
            match c {
                ui::BACKSPACE => {
                    paper.sketch.pop();
                }
                _ => {
                    paper.sketch.push(c);
                }
            }
        }

        match paper.mode.enhance(&paper) {
            Some(Enhancement::FilterSections(sections)) => {
                // Clear filter background.
                for row in 0..paper.ui.pane_height() {
                    paper
                        .ui
                        .apply(Edit::new(Region::row(row), Change::Format(0)));
                }

                // Add back in the noise
                for noise in paper.noises.iter() {
                    if let Some(region) = noise.to_region(&paper.view.origin) {
                        paper.ui.apply(Edit::new(region, Change::Format(2)));
                    }
                }

                for section in sections.iter() {
                    if let Some(region) = section.to_region(&paper.view.origin) {
                        paper.ui.apply(Edit::new(region, Change::Format(1)));
                    }
                }

                paper.signals = sections;
            }
            None => {}
        }

        None
    }
}

struct DrawSketch;

impl Operation for DrawSketch {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        paper
            .ui
            .apply(Edit::new(Region::row(0), Change::Row(paper.sketch.clone())));
        None
    }
}

struct UpdateView(char);

impl Operation for UpdateView {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        let mut adjustment: Adjustment = Default::default();

        for mark in paper.marks.iter_mut() {
            adjustment += Adjustment::create(self.0, &mark.place, &paper.view);

            if adjustment.change == Change::Clear {
                if let Some(region) = mark.place.to_region(&paper.view.origin) {
                    paper.ui.apply(Edit::new(region, adjustment.change.clone()));
                }
            }

            mark.adjust(&adjustment);
            paper.view.add(mark, self.0);
        }

        if adjustment.change == Change::Clear {
            paper.view.clean();
            paper.display_view();
        }

        None
    }
}

struct ScrollDown;

impl Operation for ScrollDown {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        paper.view.scroll_down(paper.scroll_height());
        paper.display_view();
        None
    }
}

struct ScrollUp;

impl Operation for ScrollUp {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        paper.view.scroll_up(paper.scroll_height());
        paper.display_view();
        None
    }
}

struct SetMarks(Edge);

impl Operation for SetMarks {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        paper.marks.clear();

        for signal in paper.signals.iter() {
            let mut place = signal.start;

            if self.0 == Edge::End {
                let length = signal.length;

                place.index += match length {
                    ui::EOL => paper.view.line_length(&signal.start),
                    _ => length.to_usize(),
                };
            }

            paper.marks.push(Mark {
                place,
                pointer: place.index
                    + Pointer(match place.line {
                        1 => Some(0),
                        _ => paper
                            .view
                            .data
                            .match_indices(ui::ENTER)
                            .nth(place.line - 2)
                            .map(|x| x.0 + 1),
                    }),
            });
        }

        None
    }
}

/// Specifies a procedure to enhance the current sketch.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
enum Enhancement {
    /// Highlights specified regions.
    FilterSections(Vec<Section>),
}

impl fmt::Display for Enhancement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Enhancement::FilterSections(regions) => {
                write!(f, "FilterSections [")?;

                for region in regions {
                    write!(f, "  {}", region)?;
                }

                write!(f, "]")
            }
        }
    }
}

/// Specifies the result of an Op to be processed by the application.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
enum Notice {
    /// Ends the application.
    Quit,
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Specifies the functionality of the editor for a given state.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
enum Mode {
    /// Displays the current view.
    Display,
    /// Displays the current command.
    Command,
    /// Displays the current filter expression and highlights the characters that match the filter.
    Filter,
    /// Displays the highlighting that has been selected.
    Action,
    /// Displays the current view along with the current edits.
    Edit,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for Mode {
    fn default() -> Mode {
        Mode::Display
    }
}

impl Mode {
    // TODO: Can this be converted to return an Iterator?
    /// Returns the operations to be executed based on user input.
    fn handle_input(&self, input: Option<char>) -> Vec<Box<dyn Operation>> {
        let mut operations: Vec<Box<dyn Operation>> = Vec::new();

        if let Some(c) = input {
            match *self {
                Mode::Display => match c {
                    '.' => operations.push(Box::new(ChangeMode(Mode::Command))),
                    '#' | '/' => {
                        operations.push(Box::new(ChangeMode(Mode::Filter)));
                        operations.push(Box::new(AddToSketch(c.to_string())));
                        operations.push(Box::new(DrawSketch));
                    }
                    'j' => operations.push(Box::new(ScrollDown)),
                    'k' => operations.push(Box::new(ScrollUp)),
                    _ => {}
                },
                Mode::Command => match c {
                    ui::ENTER => {
                        operations.push(Box::new(ExecuteCommand));
                        operations.push(Box::new(ChangeMode(Mode::Display)));
                    }
                    ui::ESC => operations.push(Box::new(ChangeMode(Mode::Display))),
                    _ => {
                        operations.push(Box::new(AddToSketch(c.to_string())));
                        operations.push(Box::new(DrawSketch));
                    }
                },
                Mode::Filter => match c {
                    ui::ENTER => operations.push(Box::new(ChangeMode(Mode::Action))),
                    '\t' => {
                        operations.push(Box::new(IdentifyNoise));
                        operations.push(Box::new(AddToSketch(String::from("&&"))));
                        operations.push(Box::new(DrawSketch));
                    }
                    ui::ESC => operations.push(Box::new(ChangeMode(Mode::Display))),
                    _ => {
                        operations.push(Box::new(AddToSketch(c.to_string())));
                        operations.push(Box::new(DrawSketch));
                    }
                },
                Mode::Action => match c {
                    ui::ESC => operations.push(Box::new(ChangeMode(Mode::Display))),
                    'i' => {
                        operations.push(Box::new(SetMarks(Edge::Start)));
                        operations.push(Box::new(ChangeMode(Mode::Edit)));
                    }
                    'I' => {
                        operations.push(Box::new(SetMarks(Edge::End)));
                        operations.push(Box::new(ChangeMode(Mode::Edit)));
                    }
                    _ => {}
                },
                Mode::Edit => match c {
                    ui::ESC => operations.push(Box::new(ChangeMode(Mode::Display))),
                    _ => {
                        operations.push(Box::new(AddToSketch(c.to_string())));
                        operations.push(Box::new(UpdateView(c)));
                    }
                },
            }
        }

        operations
    }

    /// Returns the Enhancement to be added.
    fn enhance(&self, paper: &Paper) -> Option<Enhancement> {
        match *self {
            Mode::Filter => {
                let mut sections = paper.noises.clone();

                if let Some(last_feature) = paper
                    .patterns
                    .first_feature
                    .tokenize_iter(&paper.sketch)
                    .last()
                    .and_then(|x| x.get("feature"))
                {
                    if let Some(id) = last_feature.chars().nth(0) {
                        for filter in paper.filters.iter() {
                            if id == filter.id() {
                                filter.extract(last_feature, &mut sections, &paper.view);
                                break;
                            }
                        }
                    }
                }

                Some(Enhancement::FilterSections(sections))
            }
            Mode::Display | Mode::Command | Mode::Action | Mode::Edit => None,
        }
    }
}

trait Filter: fmt::Debug {
    fn id(&self) -> char;
    fn extract<'a>(&self, feature: &'a str, sections: &mut Vec<Section>, view: &View);
}

#[derive(Debug)]
struct LineFilter {
    pattern: Pattern,
}

impl Default for LineFilter {
    fn default() -> LineFilter {
        LineFilter {
            pattern: Pattern::define(
                "#" + (ChCls::Digit.rpt(SOME).name("line") + ChCls::End
                    | ChCls::Digit.rpt(SOME).name("start")
                        + "."
                        + ChCls::Digit.rpt(SOME).name("end")
                    | ChCls::Digit.rpt(SOME).name("origin")
                        + (("+".to_rec() | "-") + ChCls::Digit.rpt(SOME)).name("movement")),
            ),
        }
    }
}

impl Filter for LineFilter {
    fn id(&self) -> char {
        '#'
    }

    fn extract<'a>(&self, feature: &'a str, sections: &mut Vec<Section>, _view: &View) {
        let tokens = self.pattern.tokenize(feature);

        if let Some(line) = tokens.get("line") {
            line.parse::<usize>().ok().map(|row| {
                sections.retain(|&x| x.start.line == row);
            });
        } else if let (Some(line_start), Some(line_end)) = (tokens.get("start"), tokens.get("end"))
        {
            if let (Ok(start), Ok(end)) = (line_start.parse::<usize>(), line_end.parse::<usize>()) {
                let top = cmp::min(start, end);
                let bottom = cmp::max(start, end);

                sections.retain(|&x| {
                    let row = x.start.line;
                    row >= top && row <= bottom
                })
            }
        } else if let (Some(line_origin), Some(line_movement)) =
            (tokens.get("origin"), tokens.get("movement"))
        {
            if let (Ok(origin), Ok(movement)) =
                (line_origin.parse::<usize>(), line_movement.parse::<isize>())
            {
                let end = (origin as isize + movement) as usize;
                let top = cmp::min(origin, end);
                let bottom = cmp::max(origin, end);

                sections.retain(|&x| {
                    let row = x.start.line;
                    row >= top && row <= bottom
                })
            }
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
            pattern: Pattern::define("/" + ChCls::Any.rpt(SOME).name("pattern")),
        }
    }
}

impl Filter for PatternFilter {
    fn id(&self) -> char {
        '/'
    }

    fn extract<'a>(&self, feature: &'a str, sections: &mut Vec<Section>, view: &View) {
        if let Some(user_pattern) = self.pattern.tokenize(feature).get("pattern") {
            if let Ok(search_pattern) = Pattern::load(user_pattern.to_rec()) {
                let target_sections = sections.clone();
                sections.clear();

                for target_section in target_sections {
                    let target = view
                        .line(target_section.start.line)
                        .unwrap()
                        .chars()
                        .skip(target_section.start.index)
                        .collect::<String>();

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
