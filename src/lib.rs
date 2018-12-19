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

    /// Moves address backward to the end of the previous row.
    fn move_to_previous_conclusion(&mut self, view: &String) {
        self.row -= 1;
        self.column = self.row_length(view);
    }

    /// Returns length of row at address.
    fn row_length(&self, view: &String) -> usize {
        view.lines().nth(self.row).unwrap().len()
    }

    /// Returns the index of view that is equivalent to address.
    fn marker(&self, view: &String) -> usize {
        self.column + match self.row {
            0 => 0,
            _ => view.match_indices(ENTER).nth(self.row - 1).unwrap().0 + 1,
        }
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

impl From<Length> for usize {
    /// Convert a Length to a usize.
    fn from(length: Length) -> usize {
        length.0
    }
}

impl From<Length> for i32 {
    /// Convert a Length to a i32.
    fn from(length: Length) -> i32 {
        match length {
            EOL => -1,
            _ => length.0 as i32,
        }
    }
}

/// Specifies a group of adjacent Addresses.
struct Region {
    /// The first Address.
    start: Address,
    /// The number of included Addresses.
    length: Length,
}

/// Character that represents the `Backspace` key.
const BACKSPACE: char = '\u{08}';
/// Character that represents the `Enter` key.
const ENTER: char = '\n';
/// Character that represents the `Esc` key.
const ESC: char = '';

impl Region {
    /// Creates a Region.
    fn new(row: usize, column: usize, length: Length) -> Region {
        Region {
            start: Address::new(row, column),
            length,
        }
    }

    /// Returns the number of characters included in the region of a view.
    fn length(&self, view: &String) -> usize {
        match self.length {
            EOL => self.start.row_length(view),
            _ => usize::from(self.length),
        }
    }

    /// Returns the address at an Edge of the region of a view.
    fn address(&self, edge: Edge, view: &String) -> Address {
        let mut start_address = Address::new(self.start.row, self.start.column);

        if edge == Edge::End {
            start_address.move_right(self.length(view));
        }

        start_address
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

/// Specifies an Operation result that should be processed by the application.
enum Notice {
    /// Ends the application.
    Quit,
}

impl Mode {
    fn handle_input(&self, c: char) -> Vec<Operation> {
        let mut operations = Vec::new();

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

        operations
    }

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
                                    regions.push(Region::new(row, 0, EOL));
                                });
                        }

                        if let Some(key) = captures.name("key") {
                            let length = Length(key.as_str().len());

                            for (row, line) in view.lines().enumerate() {
                                for (key_index, _) in line.match_indices(key.as_str()) {
                                    regions.push(Region::new(row, key_index, length));
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

/// All data related to paper application.
pub struct Paper {
    window: pancurses::Window,
    mode: Mode,
    view: String,
    sketch: String,
    first_line: usize,
    line_number_length: usize,
    marker: usize,
    filter_regions: Vec<Region>,
    cursor_address: Address,
    path: String,
    is_dirty: bool,
}

impl Paper {
    pub fn new() -> Paper {
        // Must call initscr() first.
        let window = pancurses::initscr();

        // Prevent curses from outputing keys.
        pancurses::noecho();

        pancurses::start_color();
        pancurses::use_default_colors();
        pancurses::init_pair(0, -1, -1);
        pancurses::init_pair(1, -1, pancurses::COLOR_BLUE);

        Paper {
            window,
            mode: DISPLAY_MODE,
            first_line: 0,
            sketch: String::new(),
            view: String::new(),
            line_number_length: 0,
            marker: 0,
            filter_regions: Vec::new(),
            cursor_address: Address::new(0, 0),
            path: String::new(),
            is_dirty: false,
        }
    }

    pub fn run(&mut self) {
        'main: loop {
            let operations = self.process_input();

            for operation in operations {
                match self.operate(operation) {
                    Some(Notice::Quit) => break 'main,
                    None => (),
                }
            }
        }

        pancurses::endwin();
    }

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
                match c {
                    BACKSPACE => {
                        if self.cursor_address.is_origin() {
                            // Because drawing BACKSPACE across a newline is complicated, just
                            // reset with write_view().
                            self.is_dirty = true;
                            self.sketch.clear();
                            self.cursor_address.move_to_previous_conclusion(&self.view);
                        } else {
                            self.window.addch(c);
                            self.window.delch();

                            self.sketch.pop();
                            self.cursor_address.move_left(1);
                        }
                    }
                    ENTER => {
                        // Because drawing an Enter character is complicated, just reset with
                        // write_view().
                        self.is_dirty = true;
                        self.sketch.clear();
                        self.cursor_address.move_to_next_origin();
                    }
                    _ => {
                        self.window.insch(c);

                        self.sketch.push(c);
                        self.cursor_address.move_right(1);
                    }
                }

                match self.mode.enhance(&self.sketch, &self.view) {
                    Some(Enhancement::FilterRegions(regions)) => {
                        // Clear filter background.
                        for line in 0..self.window_height() {
                            self.window.mvchgat(
                                line as i32,
                                0,
                                i32::from(EOL),
                                pancurses::A_NORMAL,
                                0,
                            );
                        }

                        for region in regions.iter() {
                            self.highlight_region(region, 1);
                        }

                        self.filter_regions = regions;
                    }
                    None => {}
                }

                self.move_cursor();
            }
            Operation::AddToView(c) => {
                match c {
                    BACKSPACE => {
                        self.marker -= 1;
                        self.view.remove(self.marker);
                    }
                    _ => {
                        self.view.insert(self.marker, c);
                        self.marker += 1;
                    }
                }

                if self.is_dirty {
                    self.write_view();
                    // write_view() moves cursor so move it back
                    self.move_cursor();
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
                        self.cursor_address = Address::new(0, 0);
                        self.move_cursor();
                        self.sketch.clear();
                    }
                    ACTION_MODE => {}
                    EDIT_MODE => {
                        self.write_view();
                        self.move_cursor();
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
                self.cursor_address = self.filter_regions[0].address(edge, &self.view);
                self.marker = self.cursor_address.marker(&self.view);
            }
        }

        None
    }

    fn process_input(&mut self) -> Vec<Operation> {
        match self.window.getch() {
            Some(Input::Character(c)) => self.mode.handle_input(c),
            _ => Vec::new(),
        }
    }

    fn write_view(&mut self) {
        self.window.clear();
        self.window.mv(0, 0);
        let lines: Vec<&str> = self.view.lines().collect();
        let length = lines.len();
        self.line_number_length = ((length as f32).log10() as usize) + 2;
        let max = cmp::min(self.window_height() + self.first_line, length);

        for (index, line) in lines[self.first_line..max].iter().enumerate() {
            self.window.addstr(format!(
                "{:>width$} ",
                index + self.first_line + 1,
                width = self.line_number_length
            ));
            self.window.addstr(line);
            self.window.addch(ENTER);
        }
    }

    fn window_height(&self) -> usize {
        self.window.get_max_y() as usize
    }

    fn scroll_height(&self) -> usize {
        self.window_height() / 4
    }

    fn highlight_region(&mut self, region: &Region, color_pair: i16) {
        self.window.mvchgat(
            region.start.row as i32,
            (region.start.column + self.line_number_length + 1) as i32,
            i32::from(region.length),
            pancurses::A_NORMAL,
            color_pair,
        );
    }

    fn move_cursor(&mut self) {
        self.window.mv(
            self.cursor_address.row as i32,
            (self.line_number_length + 1 + self.cursor_address.column) as i32,
        );
    }
}
