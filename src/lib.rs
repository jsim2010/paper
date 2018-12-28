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

extern crate regex;

mod rec;
mod ui;

use rec::{ChCls, Rec, Rpt, OPT, SOME, VAR};
use std::cmp;
use std::fmt;
use std::fs;
use std::ops::{Add, AddAssign, SubAssign};
use std::vec::IntoIter;
use ui::{Address, Length, Region, UserInterface};

/// The paper application.
#[derive(Debug, Default)]
pub struct Paper<'a> {
    /// User interface of the application.
    ui: UserInterface,
    /// Current mode of the application.
    mode: Mode,
    /// Data of the file being edited.
    view: View,
    command_hunter: Hunter<'a>,
    see_hunter: Hunter<'a>,
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

impl<'a> Paper<'a> {
    /// Creates a new paper application.
    ///
    /// # Examples
    /// ```ignore
    /// # use paper::Paper;
    /// let paper = Paper::new();
    /// ```
    pub fn new() -> Paper<'a> {
        Paper {
            marks: vec![Default::default()],
            command_hunter: Hunter::new(CommandPattern),
            see_hunter: Hunter::new(SeePattern),
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
                match self.operate(operation) {
                    Some(Notice::Quit) => break 'main,
                    None => (),
                }
            }
        }

        self.ui.close();
    }

    /// Performs the given [`Operation`].
    ///
    /// [`Operation`]: enum.Operation.html
    fn operate(&mut self, op: Operation) -> Option<Notice> {
        match op {
            Operation::ExecuteCommand => {
                match self.command_hunter.capture(&self.sketch) {
                    Some("see") => {
                        match self.see_hunter.capture(&self.sketch) {
                            Some(path) => {
                                self.path = String::from(path);
                                self.view = View::with_file(&self.path);
                                self.first_line = 0;
                                self.noises.clear();

                                for row in 0..self.view.data.lines().count() {
                                    self.noises.push(Region::line(row));
                                }
                            }
                            None => {}
                        }
                    }
                    Some("put") => {
                        fs::write(&self.path, &self.view.data).unwrap();
                    }
                    Some("end") => return Some(Notice::Quit),
                    Some(_) => {}
                    None => {}
                }
            }
            Operation::IdentifyNoise => {
                let first_filter =
                    (ChCls::AllBut("&").rpt(VAR).name("filter") + "&&".rpt(OPT)).form();
                let filter = ("#" + ChCls::Digit.rpt(SOME).name("line")
                    | "/" + ChCls::Any.rpt(SOME).name("key")).form();
                let mut regions = Vec::new();

                for row in 0..self.view.data.lines().count() {
                    regions.push(Region::line(row));
                }

                for caps in first_filter.captures_iter(&self.sketch) {
                    match &filter.captures(&caps["filter"]) {
                        Some(captures) => {
                            if let Some(line) = captures.name("line") {
                                // Subtract 1 to match row.
                                line.as_str()
                                    .parse::<usize>()
                                    .map(|i| i - 1)
                                    .ok()
                                    .map(|row| {
                                        regions.retain(|&x| x.start().row == row);
                                    });
                            }

                            if let Some(key) = captures.name("key") {
                                let mut new_regions = Vec::new();

                                for region in regions {
                                    let pre_filter = self
                                        .view
                                        .data
                                        .lines()
                                        .nth(region.start().row)
                                        .unwrap()
                                        .chars()
                                        .skip(region.start().column)
                                        .collect::<String>();

                                    for (key_index, key_match) in
                                        pre_filter.match_indices(key.as_str())
                                    {
                                        new_regions.push(Region::with_address_length(
                                            Address::with_row_column(
                                                region.start().row,
                                                region.start().column + key_index,
                                            ),
                                            Length::from(key_match.len()),
                                        ));
                                    }
                                }

                                regions = new_regions;
                            }
                        }
                        None => {}
                    }
                }

                self.noises.clear();

                for region in regions {
                    self.ui.set_background(&region, 2);
                    self.noises.push(region);
                }

                self.ui.move_to(&self.marks[0].address);
            }
            Operation::AddToSketch(s) => {
                let mut adjustment = 0;

                for mark in self.marks.iter_mut() {
                    mark.adjust(adjustment);
                    self.ui.move_to(&mark.address);

                    for edit in mark.add(&s, &self.view) {
                        match edit {
                            Edit::Backspace => {
                                self.ui.delete_back();
                                self.sketch.pop();
                                adjustment -= 1;
                            }
                            Edit::Wash(x) => {
                                self.is_dirty = true;
                                self.sketch.clear();
                                adjustment += x;
                            }
                            Edit::Add(c) => {
                                self.ui.insert_char(c);
                                self.sketch.push(c);
                                adjustment += 1;
                            }
                        }
                    }
                }

                match self
                    .mode
                    .enhance(&self.sketch, &self.view.data, &self.noises)
                {
                    Some(Enhancement::FilterRegions(regions)) => {
                        // Clear filter background.
                        for line in 0..self.ui.window_height() {
                            self.ui.set_background(&Region::line(line), 0);
                        }

                        // Add back in the noise
                        for noise in self.noises.iter() {
                            self.ui.set_background(noise, 2);
                        }

                        for region in regions.iter() {
                            self.ui.set_background(region, 1);
                        }

                        self.signals = regions;
                    }
                    None => {}
                }

                self.ui.move_to(&self.marks[0].address);
            }
            Operation::AddToView(c) => {
                for mark in self.marks.iter() {
                    self.view.add(c, mark.index);
                }

                if self.is_dirty {
                    self.write_view();
                    // write_view() moves cursor so move it back
                    self.ui.move_to(&self.marks[0].address);
                    self.is_dirty = false;
                }
            }
            Operation::ChangeMode(mode) => {
                self.mode = mode;

                match self.mode {
                    Mode::Display => {
                        self.write_view();
                    }
                    Mode::Command | Mode::Filter => {
                        self.marks.truncate(1);
                        self.marks[0].reset();
                        self.ui.move_to(&self.marks[0].address);
                        self.sketch.clear();
                    }
                    Mode::Action => {}
                    Mode::Edit => {
                        self.write_view();
                        self.ui.move_to(&self.marks[0].address);
                        self.sketch.clear();
                    }
                }
            }
            Operation::ScrollDown => {
                self.first_line = cmp::min(
                    self.first_line + self.scroll_height(),
                    self.view.data.lines().count() - 1,
                );
                self.write_view();
            }
            Operation::ScrollUp => {
                let movement = self.scroll_height();

                if self.first_line < movement {
                    self.first_line = 0;
                } else {
                    self.first_line -= movement;
                }
                self.write_view();
            }
            Operation::SetMarks(edge) => {
                self.marks.clear();

                for signal in self.signals.iter() {
                    self.marks
                        .push(Marker { region: *signal }.generate_mark(edge, &self.view));
                }
            }
        }

        None
    }

    /// Displays the view on the user interface.
    fn write_view(&mut self) {
        self.ui.clear();
        self.ui
            .calc_line_number_width(self.view.data.lines().count());

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

#[derive(Debug)]
struct Hunter<'a> {
    re: regex::Regex,
    prey: &'a str,
}

impl<'a> Hunter<'a> {
    fn new(pattern: impl Pattern<'a>) -> Hunter<'a> {
        Hunter {
            re: pattern.regex(),
            prey: pattern.prey(),
        }
    }

    fn capture(&self, field: &'a str) -> Option<&'a str> {
        match self.re.captures(field) {
            Some(captures) => captures.name(self.prey).map(|x| x.as_str()),
            None => None,
        }
    }
}

impl<'a> Default for Hunter<'a> {
    fn default() -> Hunter<'a> {
        Hunter {
            re: regex::Regex::new("").unwrap(),
            prey: Default::default(),
        }
    }
}

trait Pattern<'a> {
    fn regex(&self) -> regex::Regex;
    fn prey(&self) -> &'a str;
}

struct CommandPattern;

impl<'a> Pattern<'a> for CommandPattern {
    fn regex(&self) -> regex::Regex {
        (ChCls::Any.rpt(SOME.lazy()).name("cmd") + (ChCls::WhSpc | ChCls::End)).form()
    }

    fn prey(&self) -> &'a str {
        "cmd"
    }
}

struct SeePattern;

impl <'a> Pattern<'a> for SeePattern {
    fn regex(&self) -> regex::Regex {
        ("see" + ChCls::WhSpc.rpt(SOME) + ChCls::Any.rpt(VAR).name("path")).form()
    }

    fn prey(&self) -> &'a str {
        "path"
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

/// Specifies a procedure based on user input to be executed by the application.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
enum Operation {
    /// Changes the mode.
    ChangeMode(Mode),
    /// Executes the command in the sketch.
    ExecuteCommand,
    /// Scrolls the view down by 1/4 of the window.
    ScrollDown,
    /// Scrolls the view up by 1/4 of the window.
    ScrollUp,
    /// Adds a string to the sketch.
    AddToSketch(String),
    /// Adds a character to the view.
    AddToView(char),
    /// Sets the marks to be an edge of the filtered regions.
    SetMarks(Edge),
    IdentifyNoise,
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
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

/// Specifies the result of an Operation to be processed by the application.
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
    fn handle_input(&self, input: Option<char>) -> Vec<Operation> {
        let mut operations = Vec::new();

        match input {
            Some(c) => match *self {
                Mode::Display => match c {
                    '.' => operations.push(Operation::ChangeMode(Mode::Command)),
                    '#' | '/' => {
                        operations.push(Operation::ChangeMode(Mode::Filter));
                        operations.push(Operation::AddToSketch(c.to_string()));
                    }
                    'j' => operations.push(Operation::ScrollDown),
                    'k' => operations.push(Operation::ScrollUp),
                    _ => {}
                },
                Mode::Command => match c {
                    ui::ENTER => {
                        operations.push(Operation::ExecuteCommand);
                        operations.push(Operation::ChangeMode(Mode::Display));
                    }
                    ui::ESC => operations.push(Operation::ChangeMode(Mode::Display)),
                    _ => operations.push(Operation::AddToSketch(c.to_string())),
                },
                Mode::Filter => match c {
                    ui::ENTER => operations.push(Operation::ChangeMode(Mode::Action)),
                    '\t' => {
                        operations.push(Operation::IdentifyNoise);
                        operations.push(Operation::AddToSketch(String::from("&&")));
                    }
                    ui::ESC => operations.push(Operation::ChangeMode(Mode::Display)),
                    _ => operations.push(Operation::AddToSketch(c.to_string())),
                },
                Mode::Action => match c {
                    ui::ESC => operations.push(Operation::ChangeMode(Mode::Display)),
                    'i' => {
                        operations.push(Operation::SetMarks(Edge::Start));
                        operations.push(Operation::ChangeMode(Mode::Edit));
                    }
                    'I' => {
                        operations.push(Operation::SetMarks(Edge::End));
                        operations.push(Operation::ChangeMode(Mode::Edit));
                    }
                    _ => {}
                },
                Mode::Edit => match c {
                    ui::ESC => operations.push(Operation::ChangeMode(Mode::Display)),
                    _ => {
                        operations.push(Operation::AddToSketch(c.to_string()));
                        operations.push(Operation::AddToView(c));
                    }
                },
            },
            None => {}
        }

        operations
    }

    /// Returns the Enhancement to be added.
    fn enhance(&self, sketch: &String, view: &String, noises: &Vec<Region>) -> Option<Enhancement> {
        match *self {
            Mode::Filter => {
                let each_filter =
                    (ChCls::AllBut("&").rpt(VAR).name("filter") + "&&".rpt(OPT)).form();
                let filter = (("#"
                    + (ChCls::Digit.rpt(SOME).name("line") + ChCls::End
                        | ChCls::Digit.rpt(SOME).name("start")
                            + "."
                            + ChCls::Digit.rpt(SOME).name("end")
                        | ChCls::Digit.rpt(SOME).name("origin")
                            + (("+".re() | "-") + ChCls::Digit.rpt(SOME)).name("movement")))
                    | ("/" + ChCls::Any.rpt(SOME).name("key"))).form();
                let mut regions = noises.clone();

                match &filter.captures(&each_filter.captures_iter(sketch).last().unwrap()["filter"])
                {
                    Some(captures) => {
                        if let Some(line) = captures.name("line") {
                            // Subtract 1 to match row.
                            line.as_str()
                                .parse::<usize>()
                                .map(|i| i - 1)
                                .ok()
                                .map(|row| {
                                    regions.retain(|&x| x.start().row == row);
                                });
                        } else if let (Some(line_start), Some(line_end)) =
                            (captures.name("start"), captures.name("end"))
                        {
                            if let (Ok(start), Ok(end)) = (
                                line_start.as_str().parse::<usize>().map(|i| i - 1),
                                line_end.as_str().parse::<usize>().map(|i| i - 1),
                            ) {
                                let top = cmp::min(start, end);
                                let bottom = cmp::max(start, end);

                                regions.retain(|&x| {
                                    let row = x.start().row;
                                    row >= top && row <= bottom
                                })
                            }
                        } else if let (Some(line_origin), Some(line_movement)) =
                            (captures.name("origin"), captures.name("movement"))
                        {
                            if let (Ok(origin), Ok(movement)) = (
                                line_origin.as_str().parse::<usize>().map(|i| i - 1),
                                line_movement.as_str().parse::<isize>(),
                            ) {
                                let end = (origin as isize + movement) as usize;
                                let top = cmp::min(origin, end);
                                let bottom = cmp::max(origin, end);

                                regions.retain(|&x| {
                                    let row = x.start().row;
                                    row >= top && row <= bottom
                                })
                            }
                        } else if let Some(key) = captures.name("key") {
                            let mut new_regions = Vec::new();

                            for region in regions {
                                let pre_filter = view
                                    .lines()
                                    .nth(region.start().row)
                                    .unwrap()
                                    .chars()
                                    .skip(region.start().column)
                                    .collect::<String>();

                                for (key_index, key_match) in pre_filter.match_indices(key.as_str())
                                {
                                    new_regions.push(Region::with_address_length(
                                        Address::with_row_column(
                                            region.start().row,
                                            region.start().column + key_index,
                                        ),
                                        Length::from(key_match.len()),
                                    ));
                                }
                            }

                            regions = new_regions;
                        }
                    }
                    None => {}
                }

                Some(Enhancement::FilterRegions(regions))
            }
            Mode::Display | Mode::Command | Mode::Action | Mode::Edit => None,
        }
    }
}
