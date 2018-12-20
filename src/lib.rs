//! A terminal-based editor with goals to maximize simplicity and efficiency.
extern crate pancurses;
extern crate regex;

use pancurses::Input;
use regex::Regex;
use std::cmp;
use std::fs;

/// Specifies the functionality of the editor for a given state.
#[derive(PartialEq, Eq)]
enum Mode {
    /// Displays the current view.
    Display {},
    /// Displays the current command.
    Command {},
    /// Displays the current filter expression and highlights the characters that match the filter.
    Filter {},
    /// Displays the highlighting that has been selected.
    Action {},
    /// Displays the current view along with the current edits.
    Edit {},
}

/// The `Display` mode.
const DISPLAY_MODE: Mode = Mode::Display {};
/// The `Command` mode.
const COMMAND_MODE: Mode = Mode::Command {};
/// The `Filter` mode.
const FILTER_MODE: Mode = Mode::Filter {};
/// The `Action` mode.
const ACTION_MODE: Mode = Mode::Action {};
/// The 'Edit` mode.
const EDIT_MODE: Mode = Mode::Edit {};

#[derive(Clone, Copy)]
/// Location of a block in the terminal grid.
struct Address {
    /// Index of the row that contains the block (including 0).
    row: usize,
    /// Index of the column that contains the block (including 0).
    column: usize,
}

impl Address {
    /// Create an Address.
    fn new(row: usize, column: usize) -> Address {
        Address { row, column }
    }

    /// Return if address is start of a row.
    fn is_origin(&self) -> bool {
        self.column == 0
    }

    /// Moves address a given number of blocks to the left.
    fn move_left(&mut self, count: usize) {
        self.column -= count;
    }

    /// Moves address a given number of blocks to the right.
    fn move_right(&mut self, count: usize) {
        self.column += count;
    }

    /// Moves address forward to the start of the next row.
    fn move_to_next_origin(&mut self) {
        self.row += 1;
        self.column = 0;
    }

    /// Moves address 1 row up.
    fn move_up(&mut self) {
        self.row -= 1;
    }

    /// Moves address to column.
    fn move_to_column(&mut self, column: usize) {
        self.column = column;
    }
}

#[derive(PartialEq)]
/// Indicates a specific Address of a given Region.
enum Edge {
    /// Indicates the first Address of the Region.
    Start,
    /// Indicates the last Address of the Region.
    End,
}

#[derive(Copy, Clone, PartialEq, Eq)]
/// Specifies the length of a Region.
struct Length(usize);

/// Length that represents the number of characters until the end of the line.
const EOL: Length = Length(usize::max_value());

impl Length {
    /// Convert length to usize.
    fn as_usize(&self) -> &usize {
        &self.0
    }

    /// Convert length to i32.
    fn to_i32(&self) -> i32 {
        match *self {
            EOL => -1,
            _ => self.0 as i32,
        }
    }
}

/// Indicates changes to the sketch and view to be made.
enum Edit {
    /// Removes the previous character from the sketch.
    Backspace,
    /// Clears the sketch and redraws the view.
    Wash,
    /// Adds the character to the view.
    Add,
}

#[derive(Clone, Copy)]
/// An address and its respective index in a view.
struct Marker {
    /// Address of marker.
    address: Address,
    /// Index in view that corresponds with marker.
    index: Option<usize>,
}

impl Marker {
    /// Creates a new Marker.
    fn new() -> Marker {
        Marker {
            address: Address::new(0, 0),
            index: Some(0),
        }
    }

    /// Creates a new marker at the given address.
    fn with_address(address: Address, view: &String) -> Marker {
        Marker {
            address: address,
            index: match address.row {
                0 => Some(0),
                _ => view.match_indices(ENTER).nth(address.row - 1).map(|x| x.0 + 1),
            }.map(|i| i + address.column),
        }
    }

    /// Returns the length of the line at Marker.
    fn line_length(&self, view: &String) -> usize {
        view.lines().nth(self.address.row).unwrap().len()
    }

    /// Moves marker based on the added char and returns the appropriate Edit.
    fn add(&mut self, c: char, view: &String) -> Edit {
        match c {
            BACKSPACE => {
                self.index = self.index.map(|x| x - 1);

                if self.address.is_origin() {
                    self.address.move_up();
                    let column = self.line_length(view);
                    self.address.move_to_column(column);
                    return Edit::Wash;
                }

                self.address.move_left(1);
                Edit::Backspace
            }
            ENTER => {
                self.index = self.index.map(|x| x + 1);
                self.address.move_to_next_origin();
                Edit::Wash
            }
            _ => {
                self.move_right(1);
                Edit::Add
            }
        }
    }

    /// Moves marker a given number of blocks to the right.
    fn move_right(&mut self, count: usize) {
        self.address.move_right(count);
        self.index = self.index.map(|x| x + count);
    }

    /// Returns the column of marker.
    fn x(&self) -> i32 {
        self.address.column as i32
    }

    /// Returns the row of marker.
    fn y(&self) -> i32 {
        self.address.row as i32
    }
}

/// Specifies a group of adjacent Addresses.
struct Region {
    /// Marker at the first Address.
    start: Marker,
    /// The number of included Addresses.
    length: Length,
}

/// Character that represents the `Backspace` key.
const BACKSPACE: char = '\u{08}';
/// Character that represents the `Enter` key.
const ENTER: char = '\n';
// Currently Ctrl + C to allow manual testing within vim terminal where ESC is already mapped.
/// Character that represents the `Esc` key.
const ESC: char = '';

impl Region {
    /// Creates a Region.
    fn new(address: Address, length: Length, view: &String) -> Region {
        Region {
            start: Marker::with_address(address, view),
            length,
        }
    }

    /// Returns the number of characters included in the region of a view.
    fn length(&self, view: &String) -> usize {
        match self.length {
            EOL => self.start.line_length(view),
            _ => *self.length.as_usize(),
        }
    }

    /// Returns the marker at an Edge of the region;
    fn marker(&self, edge: Edge, view: &String) -> Marker {
        let mut edge_marker = self.start;

        if edge == Edge::End {
            edge_marker.move_right(self.length(view));
        }

        edge_marker
    }
}

/// Specifies a procedure based on user input to be executed by the application.
enum Operation {
    /// Changes the mode.
    ChangeMode(Mode),
    /// Executes the command in the sketch.
    ExecuteCommand,
    /// Scrolls the view down by 1/4 of the window.
    ScrollDown,
    /// Scrolls the view up by 1/4 of the window.
    ScrollUp,
    /// Adds a character to the sketch.
    AddToSketch(char),
    /// Adds a character to the view.
    AddToView(char),
    /// Sets the marker to be an edge of the filtered regions.
    SetMarker(Edge),
}

/// Specifies a procedure to enhance the current sketch.
enum Enhancement {
    /// Highlights specified regions.
    FilterRegions(Vec<Region>),
}

/// Specifies the result of an Operation to be processed by the application.
enum Notice {
    /// Ends the application.
    Quit,
}

impl Mode {
    /// Returns the operations to be executed based on user input.
    fn handle_input(&self, input: Option<char>) -> Vec<Operation> {
        let mut operations = Vec::new();

        match input {
            Some(c) => {
                match *self {
                    DISPLAY_MODE => match c {
                        '.' => operations.push(Operation::ChangeMode(COMMAND_MODE)),
                        '#' | '/' => {
                            operations.push(Operation::ChangeMode(FILTER_MODE));
                            operations.push(Operation::AddToSketch(c));
                        }
                        'j' => operations.push(Operation::ScrollDown),
                        'k' => operations.push(Operation::ScrollUp),
                        _ => {}
                    },
                    COMMAND_MODE => match c {
                        ENTER => {
                            operations.push(Operation::ExecuteCommand);
                            operations.push(Operation::ChangeMode(DISPLAY_MODE));
                        }
                        ESC => operations.push(Operation::ChangeMode(DISPLAY_MODE)),
                        _ => operations.push(Operation::AddToSketch(c)),
                    },
                    FILTER_MODE => match c {
                        ENTER => operations.push(Operation::ChangeMode(ACTION_MODE)),
                        ESC => operations.push(Operation::ChangeMode(DISPLAY_MODE)),
                        _ => operations.push(Operation::AddToSketch(c)),
                    },
                    ACTION_MODE => match c {
                        ESC => operations.push(Operation::ChangeMode(DISPLAY_MODE)),
                        'i' => {
                            operations.push(Operation::SetMarker(Edge::Start));
                            operations.push(Operation::ChangeMode(EDIT_MODE));
                        }
                        'I' => {
                            operations.push(Operation::SetMarker(Edge::End));
                            operations.push(Operation::ChangeMode(EDIT_MODE));
                        }
                        _ => {}
                    },
                    EDIT_MODE => match c {
                        ESC => operations.push(Operation::ChangeMode(DISPLAY_MODE)),
                        _ => {
                            operations.push(Operation::AddToSketch(c));
                            operations.push(Operation::AddToView(c));
                        }
                    },
                }
            }
            None => {}
        }

        operations
    }

    /// Returns the Enhancement to be added.
    fn enhance(&self, sketch: &String, view: &String) -> Option<Enhancement> {
        match *self {
            FILTER_MODE => {
                let re = Regex::new(r"#(?P<line>\d+)|/(?P<key>.+)").unwrap();
                let mut regions = Vec::new();

                match &re.captures(sketch) {
                    Some(captures) => {
                        if let Some(line) = captures.name("line") {
                            // Subtract 1 to match row.
                            line.as_str()
                                .parse::<usize>()
                                .map(|i| i - 1)
                                .ok()
                                .map(|row| {
                                    regions.push(Region::new(Address::new(row, 0), EOL, view));
                                });
                        }

                        if let Some(key) = captures.name("key") {
                            let length = Length(key.as_str().len());

                            for (row, line) in view.lines().enumerate() {
                                for (key_index, _) in line.match_indices(key.as_str()) {
                                    regions.push(Region::new(Address::new(row, key_index), length, view));
                                }
                            }
                        }
                    }
                    None => {}
                }

                Some(Enhancement::FilterRegions(regions))
            }
            DISPLAY_MODE | COMMAND_MODE | ACTION_MODE | EDIT_MODE => None,
        }
    }
}

/// Displays output and receives input from the user.
struct UserInterface {
    /// Interface to the terminal output.
    window: pancurses::Window,
    /// The number of characters used to output line numbers.
    line_number_width: usize,
}

impl UserInterface {
    /// Creates a new UserInterace.
    fn new() -> UserInterface {
        // Must call initscr() first.
        let window = pancurses::initscr();

        // Prevent curses from outputing keys.
        pancurses::noecho();

        pancurses::start_color();
        pancurses::use_default_colors();
        pancurses::init_pair(0, -1, -1);
        pancurses::init_pair(1, -1, pancurses::COLOR_BLUE);

        UserInterface {
            window,
            line_number_width: 0,
        }
    }

    /// Outputs a BACKSPACE (moves back 1 block and deletes the character there).
    fn backspace(&self) {
        self.window.addch(BACKSPACE);
        self.window.delch();
    }

    /// Outputs a character.
    fn insert(&self, c: char) {
        self.window.insch(c);
    }

    /// Changes the background color of a region.
    fn background(&self, region: &Region, color_pair: i16) {
        self.window.mvchgat(region.start.y(), self.origin() + region.start.x(), region.length.to_i32(), pancurses::A_NORMAL, color_pair);
    }

    /// Returns the user input.
    fn get(&self) -> Option<char> {
        match self.window.getch() {
            Some(Input::Character(c)) => Some(c),
            _ => None,
        }
    }

    /// Moves the cursor to a Marker.
    fn move_to(&self, marker: Marker) {
        self.window.mv(marker.y(), self.origin() + marker.x());
    }

    /// Clears the output.
    fn clear(&self) {
        self.window.clear();
    }

    /// Outputs a line, including its line number.
    fn line(&self, row: usize, line_number: usize, line: &str) {
        self.window.mv(row as i32, 0);
        self.window.addstr(format!(
            "{:>width$} ",
            line_number,
            width = self.line_number_width,
        ));
        self.window.addstr(line);
    }

    /// Returns the height of the terminal.
    fn window_height(&self) -> usize {
        self.window.get_max_y() as usize
    }

    /// Sets the width needed for to display line numbers for a given number of lines.
    fn set_line_number_width(&mut self, line_count: usize) {
        self.line_number_width = ((line_count as f32).log10() as usize) + 2;
    }

    /// Returns the column index at which view output starts.
    fn origin(&self) -> i32 {
        (self.line_number_width + 1) as i32
    }
}

/// Application data.
pub struct Paper {
    /// User interface of the application.
    ui: UserInterface,
    /// Current mode of the application.
    mode: Mode,
    /// Data of the file being edited.
    view: String,
    /// Characters being edited to be analyzed by the application.
    sketch: String,
    /// Index of the first displayed line.
    first_line: usize,
    /// Regions that match the current filter.
    filter_regions: Vec<Region>,
    /// Path of the file being edited.
    path: String,
    /// If the view should be redrawn.
    ///
    /// Used to handle complicated edits.
    is_dirty: bool,
    /// Marker of the cursor.
    marker: Marker,
}

impl Paper {
    /// Creates a new Paper.
    pub fn new() -> Paper {
        Paper {
            ui: UserInterface::new(),
            mode: DISPLAY_MODE,
            first_line: 0,
            sketch: String::new(),
            view: String::new(),
            marker: Marker::new(),
            filter_regions: Vec::new(),
            path: String::new(),
            is_dirty: false,
        }
    }

    /// Runs paper application.
    pub fn run(&mut self) {
        'main: loop {
            let operations = self.mode.handle_input(self.ui.get());

            for operation in operations {
                match self.operate(operation) {
                    Some(Notice::Quit) => break 'main,
                    None => (),
                }
            }
        }

        pancurses::endwin();
    }

    /// Performs the given operation.
    fn operate(&mut self, op: Operation) -> Option<Notice> {
        match op {
            Operation::ExecuteCommand => {
                let re = Regex::new(r"(?P<command>.+?)(?:\s|$)").unwrap();
                let command = self.sketch.clone();

                match re.captures(&command) {
                    Some(caps) => match &caps["command"] {
                        "see" => {
                            let see_re = Regex::new(r"see\s*(?P<path>.*)").unwrap();
                            self.path = see_re.captures(&self.sketch).unwrap()["path"].to_string();
                            self.view = fs::read_to_string(&self.path).unwrap().replace('\r', "");
                            self.first_line = 0;
                        }
                        "put" => {
                            fs::write(&self.path, &self.view).unwrap();
                        }
                        "end" => return Some(Notice::Quit),
                        _ => {}
                    },
                    None => {}
                }
            }
            Operation::AddToSketch(c) => {
                match self.marker.add(c, &self.view) {
                    Edit::Backspace => {
                        self.ui.backspace();
                        self.sketch.pop();
                    }
                    Edit::Wash => {
                        self.is_dirty = true;
                        self.sketch.clear();
                    }
                    Edit::Add => {
                        self.ui.insert(c);
                        self.sketch.push(c);
                    }
                }

                match self.mode.enhance(&self.sketch, &self.view) {
                    Some(Enhancement::FilterRegions(regions)) => {
                        // Clear filter background.
                        for line in 0..self.ui.window_height() {
                            self.ui.background(&Region::new(Address::new(line, 0), EOL, &self.view), 0);
                        }

                        for region in regions.iter() {
                            self.ui.background(region, 1);
                        }

                        self.filter_regions = regions;
                    }
                    None => {}
                }

                self.move_marker();
            }
            Operation::AddToView(c) => {
                if let Some(index) = self.marker.index {
                    match c {
                        BACKSPACE => {
                            self.view.remove(index);
                        }
                        _ => {
                            self.view.insert(index - 1, c);
                        }
                    }
                }

                if self.is_dirty {
                    self.write_view();
                    // write_view() moves cursor so move it back
                    self.move_marker();
                    self.is_dirty = false;
                }
            }
            Operation::ChangeMode(mode) => {
                self.mode = mode;

                match self.mode {
                    DISPLAY_MODE => {
                        self.write_view();
                    }
                    COMMAND_MODE | FILTER_MODE => {
                        self.marker = Marker::new();
                        self.move_marker();
                        self.sketch.clear();
                    }
                    ACTION_MODE => {}
                    EDIT_MODE => {
                        self.write_view();
                        self.move_marker();
                        self.sketch.clear();
                    }
                }
            }
            Operation::ScrollDown => {
                self.first_line = cmp::min(
                    self.first_line + self.scroll_height(),
                    self.view.lines().count() - 1,
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
            Operation::SetMarker(edge) => {
                self.marker = self.filter_regions[0].marker(edge, &self.view);
            }
        }

        None
    }

    /// Writes the view to the user interface.
    fn write_view(&mut self) {
        self.ui.clear();
        let lines: Vec<&str> = self.view.lines().collect();
        let line_count = lines.len();

        self.ui.set_line_number_width(line_count);
        let max = cmp::min(self.ui.window_height() + self.first_line, line_count);

        for (index, line) in lines[self.first_line..max].iter().enumerate() {
            self.ui.line(index, self.first_line + index + 1, line);
        }
    }

    /// Returns the height used for scrolling.
    fn scroll_height(&self) -> usize {
        self.ui.window_height() / 4
    }

    /// Moves cursor to the marker.
    fn move_marker(&self) {
        self.ui.move_to(self.marker);
    }
}
