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

extern crate rec;
extern crate regex;

mod ui;

use rec::{Atom, ChCls, Pattern, Quantifier, OPT, SOME, VAR};
use std::cmp;
use std::fmt;
use std::fs;
use std::ops::{Add, AddAssign, SubAssign};
use std::vec::IntoIter;
use ui::{Address, Length, Region, UserInterface};

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
    /// Index of the first displayed line.
    first_line: usize,
    /// [`Region`]s of the view that match the current filter.
    ///
    /// [`Region`]: .struct.Region.html
    signals: Vec<Region>,
    noises: Vec<Region>,
    /// Path of the file being edited.
    path: String,
    /// If the view should be redrawn.
    is_dirty: bool,
    /// [`Mark`] of the cursor.
    ///
    /// [`Mark`]: .struct.Mark.html
    marks: Vec<Mark>,
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
            marks: vec![Default::default()],
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
            let operations = self.mode.handle_input(self.ui.get_input());

            for operation in operations {
                match operation.operate(self) {
                    Some(Notice::Quit) => break 'main,
                    None => (),
                }
            }
        }

        self.ui.close();
    }

    /// Displays the view on the user interface.
    fn write_view(&mut self) {
        self.ui.clear();
        self.ui
            .calc_line_number_width(self.view.line_count());

        for (index, line) in self
            .view
            .data
            .lines()
            .skip(self.first_line)
            .take(self.ui.window_height())
            .enumerate()
        {
            self.ui.set_line(index, self.first_line + index + 1, line);
        }
    }

    /// Returns the height used for scrolling.
    fn scroll_height(&self) -> usize {
        self.ui.window_height() / 4
    }
}

#[derive(Debug, Default)]
struct View {
    data: String,
}

impl View {
    fn with_file(filename: &String) -> View {
        View {
            data: fs::read_to_string(filename).unwrap().replace('\r', ""),
        }
    }

    fn line_length(&self, address: &Address) -> usize {
        self.data.lines().nth(address.row).unwrap().len()
    }

    fn line_count(&self) -> usize {
        self.data.lines().count()
    }

    fn add(&mut self, c: char, index: Index) {
        // Ignore the case where index is not valid.
        if let Ok(i) = index.to_usize() {
            match c {
                ui::BACKSPACE => {
                    self.data.remove(i);
                }
                _ => {
                    self.data.insert(i - 1, c);
                }
            }
        }
    }
}

/// Indicates a specific Address of a given Region.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
enum Edge {
    /// Indicates the first Address of the Region.
    Start,
    /// Indicates the last Address of the Region.
    End,
}

impl fmt::Display for Edge {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Indicates changes to the sketch and view to be made.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
enum Edit {
    /// Removes the previous character from the sketch.
    Backspace,
    /// Clears the sketch and redraws the view.
    Wash(isize),
    /// Adds a character to the view.
    Add(char),
}

impl fmt::Display for Edit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

struct Marker {
    region: Region,
}

impl Marker {
    fn generate_mark(&self, edge: Edge, view: &View) -> Mark {
        let mut address = self.region.start();

        if edge == Edge::End {
            address.column += self.length(view);
        }

        Mark::with_address(address, view)
    }

    /// Returns the number of characters included in the region of the marker.
    fn length(&self, view: &View) -> usize {
        let length = self.region.length();

        match length {
            ui::EOL => view.line_length(&self.region.start()),
            _ => length.to_usize(),
        }
    }
}

/// An address and its respective index in a view.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
struct Mark {
    /// Index in view that corresponds with mark.
    index: Index,
    /// Address of mark.
    address: Address,
}

impl Mark {
    /// Creates a new mark at the given address.
    fn with_address(address: Address, view: &View) -> Mark {
        Mark {
            address: address,
            index: Index::with_address(address, view),
        }
    }

    /// Resets mark to default values.
    fn reset(&mut self) {
        self.index = Default::default();
        self.address.reset();
    }

    fn adjust(&mut self, adjustment: isize) {
        self.index += adjustment;
    }

    /// Moves mark based on the added [`String`] and returns the appropriate [`Edit`].
    ///
    /// [`String`]: https://doc.rust-lang.org/std/string/struct.String.html
    /// [`Edit`]: .enum.Edit.html
    fn add(&mut self, s: &String, view: &View) -> IntoIter<Edit> {
        let mut edits = Vec::new();

        for c in s.chars() {
            match c {
                ui::BACKSPACE => {
                    self.index -= 1;

                    if self.address.column == 0 {
                        self.address.row -= 1;
                        self.address.column = view.line_length(&self.address);
                        edits.push(Edit::Wash(-1));
                    }

                    self.address.column -= 1;
                    edits.push(Edit::Backspace);
                }
                ui::ENTER => {
                    self.index += 1;
                    self.address.row += 1;
                    self.address.column = 0;
                    edits.push(Edit::Wash(1));
                }
                _ => {
                    self.address.column += 1;
                    self.index += 1;

                    edits.push(Edit::Add(c));
                }
            }
        }

        edits.into_iter()
    }
}

impl fmt::Display for Mark {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.address, self.index)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct Index(Option<usize>);

impl Index {
    fn with_address(address: Address, view: &View) -> Index {
        Index::with_row(address.row, view) + address.column
    }

    fn with_row(row: usize, view: &View) -> Index {
        match row {
            0 => Default::default(),
            _ => Index(
                view.data
                    .match_indices(ui::ENTER)
                    .nth(row - 1)
                    .map(|x| x.0 + 1),
            ),
        }
    }

    fn to_usize(&self) -> Result<usize, ()> {
        self.0.ok_or(())
    }
}

impl Add<usize> for Index {
    type Output = Index;

    fn add(self, other: usize) -> Index {
        Index(self.0.map(|x| x + other))
    }
}

impl SubAssign<usize> for Index {
    fn sub_assign(&mut self, other: usize) {
        self.0 = self.0.map(|x| x - other);
    }
}

impl AddAssign<isize> for Index {
    fn add_assign(&mut self, other: isize) {
        self.0 = self.0.map(|x| (x as isize + other) as usize);
    }
}

impl Default for Index {
    fn default() -> Index {
        Index(Some(0))
    }
}

impl fmt::Display for Index {
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

trait Operation {
    fn operate(&self, paper: &mut Paper) -> Option<Notice>;
}

struct ChangeMode(Mode);

impl Operation for ChangeMode {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        paper.mode = self.0;

        match paper.mode {
            Mode::Display => {
                paper.write_view();
            }
            Mode::Command | Mode::Filter => {
                paper.marks.truncate(1);
                paper.marks[0].reset();
                paper.ui.move_to(&paper.marks[0].address);
                paper.sketch.clear();
            }
            Mode::Action => {}
            Mode::Edit => {
                paper.write_view();
                paper.ui.move_to(&paper.marks[0].address);
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
                    paper.path = String::from(path);
                    paper.view = View::with_file(&paper.path);
                    paper.first_line = 0;
                    paper.noises.clear();

                    for row in 0..paper.view.line_count() {
                        paper.noises.push(Region::line(row));
                    }
                }
                None => {}
            },
            Some("put") => {
                fs::write(&paper.path, &paper.view.data).unwrap();
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
        let mut regions = Vec::new();

        for row in 0..paper.view.line_count() {
            regions.push(Region::line(row));
        }

        for tokens in paper.first_feature_pattern.tokenize_iter(&paper.sketch) {
            if let Some(feature) = tokens.get("feature") {
                if let Some(id) = feature.chars().nth(0) {
                    for filter in paper.filters.iter() {
                        if id == filter.id() {
                            filter.extract(feature, &mut regions, &paper.view.data);
                            break;
                        }
                    }
                }
            }
        }

        paper.noises.clear();

        for region in regions {
            paper.ui.set_background(&region, 2);
            paper.noises.push(region);
        }

        paper.ui.move_to(&paper.marks[0].address);
        None
    }
}

struct AddToSketch(String);

impl Operation for AddToSketch {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        let mut adjustment = 0;

        for mark in paper.marks.iter_mut() {
            mark.adjust(adjustment);
            paper.ui.move_to(&mark.address);

            for edit in mark.add(&self.0, &paper.view) {
                match edit {
                    Edit::Backspace => {
                        paper.ui.delete_back();
                        paper.sketch.pop();
                        adjustment -= 1;
                    }
                    Edit::Wash(x) => {
                        paper.is_dirty = true;
                        paper.sketch.clear();
                        adjustment += x;
                    }
                    Edit::Add(c) => {
                        paper.ui.insert_char(c);
                        paper.sketch.push(c);
                        adjustment += 1;
                    }
                }
            }
        }

        match paper.mode.enhance(&paper, &paper.view.data, &paper.noises) {
            Some(Enhancement::FilterRegions(regions)) => {
                // Clear filter background.
                for line in 0..paper.ui.window_height() {
                    paper.ui.set_background(&Region::line(line), 0);
                }

                // Add back in the noise
                for noise in paper.noises.iter() {
                    paper.ui.set_background(noise, 2);
                }

                for region in regions.iter() {
                    paper.ui.set_background(region, 1);
                }

                paper.signals = regions;
            }
            None => {}
        }

        paper.ui.move_to(&paper.marks[0].address);
        None
    }
}

struct AddToView(char);

impl Operation for AddToView {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        for mark in paper.marks.iter() {
            paper.view.add(self.0, mark.index);
        }

        if paper.is_dirty {
            paper.write_view();
            // write_view() moves cursor so move it back
            paper.ui.move_to(&paper.marks[0].address);
            paper.is_dirty = false;
        }

        None
    }
}

struct ScrollDown;

impl Operation for ScrollDown {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        paper.first_line = cmp::min(
            paper.first_line + paper.scroll_height(),
            paper.view.line_count() - 1,
        );

        paper.write_view();
        None
    }
}

struct ScrollUp;

impl Operation for ScrollUp {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        let movement = paper.scroll_height();

        if paper.first_line < movement {
            paper.first_line = 0;
        } else {
            paper.first_line -= movement;
        }

        paper.write_view();
        None
    }
}

struct SetMarks(Edge);

impl Operation for SetMarks {
    fn operate(&self, paper: &mut Paper) -> Option<Notice> {
        paper.marks.clear();

        for signal in paper.signals.iter() {
            paper
                .marks
                .push(Marker { region: *signal }.generate_mark(self.0, &paper.view));
        }

        None
    }
}

/// Specifies a procedure to enhance the current sketch.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
enum Enhancement {
    /// Highlights specified regions.
    FilterRegions(Vec<Region>),
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
                    _ => operations.push(Box::new(AddToSketch(c.to_string()))),
                },
                Mode::Filter => match c {
                    ui::ENTER => operations.push(Box::new(ChangeMode(Mode::Action))),
                    '\t' => {
                        operations.push(Box::new(IdentifyNoise));
                        operations.push(Box::new(AddToSketch(String::from("&&"))));
                    }
                    ui::ESC => operations.push(Box::new(ChangeMode(Mode::Display))),
                    _ => operations.push(Box::new(AddToSketch(c.to_string()))),
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
                        operations.push(Box::new(AddToView(c)));
                    }
                },
            },
            None => {}
        }

        operations
    }

    /// Returns the Enhancement to be added.
    fn enhance(&self, paper: &Paper, view: &String, noises: &Vec<Region>) -> Option<Enhancement> {
        match *self {
            Mode::Filter => {
                let mut regions = noises.clone();

                if let Some(last_feature) = paper
                    .first_feature_pattern
                    .tokenize_iter(&paper.sketch)
                    .last()
                    .and_then(|x| x.get("feature"))
                {
                    if let Some(id) = last_feature.chars().nth(0) {
                        for filter in paper.filters.iter() {
                            if id == filter.id() {
                                filter.extract(last_feature, &mut regions, view);
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
    fn extract<'a>(&self, feature: &'a str, regions: &mut Vec<Region>, view: &String);
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

    fn extract<'a>(&self, feature: &'a str, regions: &mut Vec<Region>, _view: &String) {
        let tokens = self.pattern.tokenize(feature);

        if let Some(line) = tokens.get("line") {
            // Subtract 1 to match row.
            line.parse::<usize>().map(|i| i - 1).ok().map(|row| {
                regions.retain(|&x| x.start().row == row);
            });
        } else if let (Some(line_start), Some(line_end)) = (tokens.get("start"), tokens.get("end"))
        {
            if let (Ok(start), Ok(end)) = (
                line_start.parse::<usize>().map(|i| i - 1),
                line_end.parse::<usize>().map(|i| i - 1),
            ) {
                let top = cmp::min(start, end);
                let bottom = cmp::max(start, end);

                regions.retain(|&x| {
                    let row = x.start().row;
                    row >= top && row <= bottom
                })
            }
        } else if let (Some(line_origin), Some(line_movement)) =
            (tokens.get("origin"), tokens.get("movement"))
        {
            if let (Ok(origin), Ok(movement)) = (
                line_origin.parse::<usize>().map(|i| i - 1),
                line_movement.parse::<isize>(),
            ) {
                let end = (origin as isize + movement) as usize;
                let top = cmp::min(origin, end);
                let bottom = cmp::max(origin, end);

                regions.retain(|&x| {
                    let row = x.start().row;
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

    fn extract<'a>(&self, feature: &'a str, regions: &mut Vec<Region>, view: &String) {
        if let Some(pattern) = self.pattern.tokenize(feature).get("pattern") {
            let noise = regions.clone();
            regions.clear();

            for region in noise {
                let pre_filter = view
                    .lines()
                    .nth(region.start().row)
                    .unwrap()
                    .chars()
                    .skip(region.start().column)
                    .collect::<String>();

                for (key_index, key_match) in pre_filter.match_indices(pattern) {
                    regions.push(Region::with_address_length(
                        Address::with_row_column(
                            region.start().row,
                            region.start().column + key_index,
                        ),
                        Length::from(key_match.len()),
                    ));
                }
            }
        }
    }
}
