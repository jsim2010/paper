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
use std::fmt;
use std::fs;
use std::ops::{Add, AddAssign, SubAssign};

const ORIGIN_POINTER: Pointer = Pointer(Some(0));

/// The paper application.
#[derive(Debug, Default)]
pub struct Paper {
    /// User interface of the application.
    ui: UserInterface,
    /// Current mode of the application.
    mode: Mode,
    /// Data of the file being edited.
    view: View,
    command_pattern: Pattern,
    see_pattern: Pattern,
    first_feature_pattern: Pattern,
    filters: Vec<Box<dyn Filter>>,
    /// Characters being edited to be analyzed by the application.
    sketch: String,
    /// [`Section`]s of the view that match the current filter.
    ///
    /// [`Section`]: .struct.Section.html
    signals: Vec<Section>,
    noises: Vec<Section>,
    /// Path of the file being edited.
    path: String,
}

impl Paper {
    /// Creates a new paper application.
    ///
    /// # Examples
    /// ```ignore
    /// # use paper::Paper;
    /// let paper = Paper::new();
    /// ```
    pub fn new() -> Paper {
        Paper {
            command_pattern: Pattern::define(
                ChCls::Any.rpt(SOME.lazy()).name("command") + (ChCls::WhSpc | ChCls::End),
            ),
            see_pattern: Pattern::define(
                "see" + ChCls::WhSpc.rpt(SOME) + ChCls::Any.rpt(VAR).name("path"),
            ),
            first_feature_pattern: Pattern::define(
                ChCls::None("&").rpt(VAR).name("feature") + "&&".rpt(OPT),
            ),
            filters: vec![Box::new(LineFilter::new()), Box::new(PatternFilter::new())],
            ..Default::default()
        }
    }

    /// Runs the application.
    ///
    /// # Examples
    /// ```ignore
    /// # use paper::Paper;
    /// let mut paper = Paper::new();
    /// paper.run();
    /// ```
    pub fn run(&mut self) {
        self.ui.init();

        'main: loop {
            for operation in self.mode.handle_input(self.ui.receive_input()) {
                match operation.operate(self) {
                    Some(Notice::Quit) => break 'main,
                    None => (),
                }
            }
        }

        self.ui.close();
    }

    /// Displays the view on the user interface.
    fn display_view(&self) {
        for edit in self.view.redraw(self.ui.window_height()) {
            self.ui.apply(edit);
        }
    }

    /// Returns the height used for scrolling.
    fn scroll_height(&self) -> usize {
        self.ui.window_height() / 4
    }
}

#[derive(Clone, Debug, Default)]
struct View {
    data: String,
    first_line: usize,
    line_count: usize,
    /// The number of characters needed to output everything in margin (ex: line numbers).
    margin_width: usize,
    marks: Vec<Mark>,
    path: String,
    is_dirty: bool,
}

impl View {
    fn with_file(path: String) -> View {
        let mut view = View {
            data: fs::read_to_string(path.as_str()).unwrap().replace('\r', ""),
            first_line: 1,
            path: path,
            ..Default::default()
        };

        view.clean();
        view
    }

    fn redraw(&self, line_count: usize) -> Vec<Edit> {
        let mut edits = vec![Edit::new(None, vec![Change::ClearAll])];

        for (row, data) in self.lines().skip(self.first_line - 1).take(line_count).enumerate() {
            edits.push(Edit::new(Some(Region::row(row)), vec![Change::Add(format!("{:>width$} {}", self.first_line + row, data, width = self.margin_width - 1))]));
        }

        edits
    }

    fn add_to_marks(&mut self, c: char) -> Vec<Edit> {
        let mut edits = Vec::new();
        let mut adjustment = 0;

        for mark in self.marks.iter_mut() {
            let mut changes = Vec::new();
            *mark += adjustment;
            let address = Address::new(
                mark.place.line - self.first_line,
                self.margin_width + mark.place.index,
            );

            match c {
                ui::BACKSPACE => {
                    if mark.pointer != ORIGIN_POINTER {
                        mark.pointer -= 1;

                        if mark.place.index == 0 {
                            mark.place.line -= 1;
                            mark.place.index =
                                self.data.lines().nth(&mark.place.line - 1).unwrap().len();
                            adjustment -= 1;
                            self.is_dirty = true;
                        } else {
                            mark.place.index -= 1;
                        }

                        adjustment -= 1;
                        changes.push(Change::Backspace);
                    }

                    self.data.remove(mark.pointer.to_usize());
                }
                ui::ENTER => {
                    mark.pointer += 1;
                    mark.place.line += 1;
                    mark.place.index = 0;
                    adjustment += 1;
                    self.is_dirty = true;
                    self.data.insert(mark.pointer.to_usize() - 1, c);
                }
                _ => {
                    mark.place.index += 1;
                    mark.pointer += 1;
                    adjustment += 1;
                    changes.push(Change::Insert(c));
                    self.data.insert(mark.pointer.to_usize() - 1, c);
                }
            }

            edits.push(Edit::new(Some(Region::address(address)), changes));
        }

        edits
    }

    fn set_marks(&mut self, edge: Edge, signals: &Vec<Section>) {
        self.clear_marks();

        for signal in signals.iter() {
            let mut place = signal.start;

            if edge == Edge::End {
                let length = signal.length;

                place.index += match length {
                    ui::EOL => self.line_length(&signal.start),
                    _ => length.to_usize(),
                };
            }

            self.marks.push(Mark {
                place,
                pointer: place.index
                    + Pointer(match place.line {
                        1 => Some(0),
                        _ => self
                            .data
                            .match_indices(ui::ENTER)
                            .nth(place.line - 2)
                            .map(|x| x.0 + 1),
                    }),
            });
        }
    }

    fn clear_marks(&mut self) {
        self.marks.clear();
    }

    fn address_at_place(&self, place: &Place) -> Option<Address> {
        if place.line < self.first_line {
            None
        } else {
            Some(Address::new(
                place.line - self.first_line,
                self.margin_width + place.index,
            ))
        }
    }

    fn lines(&self) -> std::str::Lines {
        self.data.lines()
    }

    fn clean(&mut self) {
        self.line_count = self.lines().count();
        self.margin_width = ((self.line_count + 1) as f32).log10().ceil() as usize + 1;
        self.is_dirty = false;
    }

    fn scroll_down(&mut self, scroll: usize) {
        self.first_line = cmp::min(self.first_line + scroll, self.line_count);
    }

    fn scroll_up(&mut self, scroll: usize) {
        if self.first_line <= scroll {
            self.first_line = 1;
        } else {
            self.first_line -= scroll;
        }
    }

    fn line_length(&self, place: &Place) -> usize {
        self.lines().nth(place.line - 1).unwrap().len()
    }

    fn region_at_section(&self, section: &Section) -> Option<Region> {
        self.address_at_place(&section.start).map(|x| Region::new(x, section.length))
    }

    fn put(&self) {
        fs::write(&self.path, &self.data).unwrap();
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

impl AddAssign<isize> for Mark {
    fn add_assign(&mut self, other: isize) {
        self.pointer += other;
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
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}->{}", self.start, self.length)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
struct Place {
    line: usize,
    index: usize,
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
                paper.view.clear_marks();
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
        match paper.command_pattern.tokenize(&paper.sketch).get("command") {
            Some("see") => match paper.see_pattern.tokenize(&paper.sketch).get("path") {
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

        for tokens in paper.first_feature_pattern.tokenize_iter(&paper.sketch) {
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
            paper.ui.apply(Edit::new(paper.view.region_at_section(&section), vec![Change::Format(2)]));
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
            Some(Enhancement::FilterRegions(regions)) => {
                // Clear filter background.
                for row in 0..paper.ui.window_height() {
                    paper.ui.apply(Edit::new(Some(Region::row(row)), vec![Change::Format(0)]));
                }

                // Add back in the noise
                for noise in paper.noises.iter() {
                    paper.ui.apply(Edit::new(paper.view.region_at_section(noise), vec![Change::Format(2)]));
                }

                for region in regions.iter() {
                    paper.ui.apply(Edit::new(paper.view.region_at_section(region), vec![Change::Format(1)]));
                }

                paper.signals = regions;
            }
            None => {}
        }

        None
    }
}

struct DrawSketch;

impl Operation for DrawSketch {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        paper.ui.apply(Edit::new(Some(Region::row(0)), vec![Change::Add(paper.sketch.clone()), Change::ClearEol]));
        None
    }
}

struct UpdateView(char);

impl Operation for UpdateView {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        for edit in paper.view.add_to_marks(self.0) {
            paper.ui.apply(edit);
        }

        if paper.view.is_dirty {
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
        paper.view.set_marks(self.0, &paper.signals);
        None
    }
}

/// Specifies a procedure to enhance the current sketch.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
enum Enhancement {
    /// Highlights specified regions.
    FilterRegions(Vec<Section>),
}

impl fmt::Display for Enhancement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Enhancement::FilterRegions(regions) => {
                write!(f, "FilterRegions [")?;

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
    /// Returns the operations to be executed based on user input.
    fn handle_input(&self, input: Option<char>) -> Vec<Box<dyn Operation>> {
        let mut operations: Vec<Box<dyn Operation>> = Vec::new();

        match input {
            Some(c) => match *self {
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
            },
            None => {}
        }

        operations
    }

    /// Returns the Enhancement to be added.
    fn enhance(&self, paper: &Paper) -> Option<Enhancement> {
        match *self {
            Mode::Filter => {
                let mut regions = paper.noises.clone();

                if let Some(last_feature) = paper
                    .first_feature_pattern
                    .tokenize_iter(&paper.sketch)
                    .last()
                    .and_then(|x| x.get("feature"))
                {
                    if let Some(id) = last_feature.chars().nth(0) {
                        for filter in paper.filters.iter() {
                            if id == filter.id() {
                                filter.extract(last_feature, &mut regions, &paper.view);
                                break;
                            }
                        }
                    }
                }

                Some(Enhancement::FilterRegions(regions))
            }
            Mode::Display | Mode::Command | Mode::Action | Mode::Edit => None,
        }
    }
}

trait Filter: fmt::Debug {
    fn id(&self) -> char;
    fn extract<'a>(&self, feature: &'a str, regions: &mut Vec<Section>, view: &View);
}

#[derive(Debug)]
struct LineFilter {
    pattern: Pattern,
}

impl LineFilter {
    fn new() -> LineFilter {
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

impl PatternFilter {
    fn new() -> PatternFilter {
        PatternFilter {
            pattern: Pattern::define("/" + ChCls::Any.rpt(SOME).name("pattern")),
        }
    }
}

impl Filter for PatternFilter {
    fn id(&self) -> char {
        '/'
    }

    fn extract<'a>(&self, feature: &'a str, regions: &mut Vec<Section>, view: &View) {
        if let Some(pattern) = self.pattern.tokenize(feature).get("pattern") {
            let noise = regions.clone();
            regions.clear();

            for region in noise {
                let pre_filter = view
                    .lines()
                    .nth(region.start.line - 1)
                    .unwrap()
                    .chars()
                    .skip(region.start.index)
                    .collect::<String>();

                for (key_index, key_match) in pre_filter.match_indices(pattern) {
                    regions.push(Section {
                        start: Place {
                            line: region.start.line,
                            index: region.start.index + key_index,
                        },
                        length: Length::from(key_match.len()),
                    });
                }
            }
        }
    }
}
