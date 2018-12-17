extern crate pancurses;
extern crate regex;

use pancurses::Input;
use regex::Regex;
use std::cmp;
use std::fs;

/// Character that represents Backspace key.
const BACKSPACE: char = '\u{08}';

/// All data related to paper application.
pub struct Paper {
    window: pancurses::Window,
    mode: Box<dyn Mode>,
    view: String,
    sketch: String,
    first_line: usize,
    line_number_length: usize,
    index: usize,
    filters: Vec<Filter>,
}

struct Filter {
    row: usize,
}

enum ModeType {
    Display,
    Command,
    Filter,
    Action,
    Edit,
}

enum Operation {
    ChangeMode(ModeType),
    ExecuteCommand,
    ScrollDown,
    ScrollUp,
    AddToSketch(char),
    AddToView(char),
    AppendBelow,
    InsertAbove,
}

enum Enhancement {
    Filters(Vec<Filter>),
}

enum Notice {
    Quit,
}

trait Mode {
    fn handle_input(&self, c: char) -> Vec<Operation>;

    fn enhance(&self, _sketch: &String, _view: &String) -> Option<Enhancement> {
        None
    }
}

struct DisplayMode {}

impl DisplayMode {
    fn new() -> DisplayMode {
        DisplayMode {}
    }
}

impl Mode for DisplayMode {
    fn handle_input(&self, c: char) -> Vec<Operation> {
        let mut operations = Vec::new();

        match c {
            '.' => operations.push(Operation::ChangeMode(ModeType::Command)),
            '#' | '/' => {
                operations.push(Operation::ChangeMode(ModeType::Filter));
                operations.push(Operation::AddToSketch(c));
            }
            'j' => operations.push(Operation::ScrollDown),
            'k' => operations.push(Operation::ScrollUp),
            _ => {}
        }

        operations
    }
}

struct CommandMode {}

impl CommandMode {
    fn new() -> CommandMode {
        CommandMode {}
    }
}

impl Mode for CommandMode {
    fn handle_input(&self, c: char) -> Vec<Operation> {
        let mut operations = Vec::new();

        match c {
            '\n' => {
                operations.push(Operation::ExecuteCommand);
                operations.push(Operation::ChangeMode(ModeType::Display));
            }
            '' => operations.push(Operation::ChangeMode(ModeType::Display)),
            _ => operations.push(Operation::AddToSketch(c)),
        }

        operations
    }
}

struct FilterMode {}

impl FilterMode {
    fn new() -> FilterMode {
        FilterMode {}
    }
}

impl Mode for FilterMode {
    fn handle_input(&self, c: char) -> Vec<Operation> {
        let mut operations = Vec::new();

        match c {
            '\n' => operations.push(Operation::ChangeMode(ModeType::Action)),
            '' => operations.push(Operation::ChangeMode(ModeType::Display)),
            _ => operations.push(Operation::AddToSketch(c)),
        }

        operations
    }

    fn enhance(&self, sketch: &String, view: &String) -> Option<Enhancement> {
        let re = Regex::new(r"#(?P<line>\d+)|/(?P<key>.+)").unwrap();
        let mut filters = Vec::new();

        match &re.captures(sketch) {
            Some(captures) => {
                if let Some(line) = captures.name("line") {
                    // Subtract 1 to match row.
                    line
                        .as_str()
                        .parse::<i32>()
                        .map(|i| i - 1)
                        .ok()
                        .map(|r| filters.push(Filter{row: (r as usize)}));
                }

                if let Some(key) = captures.name("key") {
                    for (index, line) in view.lines().enumerate() {
                        if line.contains(key.as_str()) {
                            filters.push(Filter{row: (index as usize)});
                        }
                    }
                }
            }
            None => {}
        }

        Some(Enhancement::Filters(filters))
    }
}

struct ActionMode {}

impl ActionMode {
    fn new() -> ActionMode {
        ActionMode {}
    }
}

impl Mode for ActionMode {
    fn handle_input(&self, c: char) -> Vec<Operation> {
        let mut operations = Vec::new();

        match c {
            'A' => {
                operations.push(Operation::AppendBelow);
                operations.push(Operation::AddToView('\n'));
                operations.push(Operation::ChangeMode(ModeType::Edit));
            }
            'I' => {
                operations.push(Operation::InsertAbove);
                operations.push(Operation::AddToView('\n'));
                operations.push(Operation::ChangeMode(ModeType::Edit));
            }
            _ => {}
        }

        operations
    }
}

struct EditMode {}

impl EditMode {
    fn new() -> EditMode {
        EditMode {}
    }
}

impl Mode for EditMode {
    fn handle_input(&self, c: char) -> Vec<Operation> {
        let mut operations = Vec::new();

        match c {
            '' => operations.push(Operation::ChangeMode(ModeType::Display)),
            _ => {
                operations.push(Operation::AddToSketch(c));
                operations.push(Operation::AddToView(c));
            }
        }

        operations
    }
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
            mode: Box::new(DisplayMode::new()),
            first_line: 0,
            sketch: String::new(),
            view: String::new(),
            line_number_length: 0,
            index: 0,
            filters: Vec::new(),
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
                            let path = see_re.captures(&self.sketch).unwrap()["path"].to_string();
                            self.view = fs::read_to_string(&path).unwrap();
                            self.first_line = 0;
                        }
                        "end" => return Some(Notice::Quit),
                        _ => {}
                    },
                    None => {}
                }
            }
            Operation::AddToSketch(c) => {
                self.window.addch(c);

                match c {
                    BACKSPACE => {
                        // addch(BACKSPACE) moves cursor back 1, so cursor is at desired location.
                        // Delete character and then add space so everything after is kept in
                        // place.
                        self.window.delch();
                        self.window.insch(' ');
                        self.sketch.pop();
                    }
                    _ => {
                        self.sketch.push(c);
                    }
                }

                match self.mode.enhance(&self.sketch, &self.view) {
                    Some(Enhancement::Filters(filters)) => {
                        // Clear filter background.
                        for line in 0..self.window_height() {
                            self.window.mvchgat(line as i32, 0, -1, pancurses::A_NORMAL, 0);
                        }

                        self.filters = filters;

                        for filter in self.filters.iter() {
                            self.window.mvchgat(filter.row as i32, 0, -1, pancurses::A_NORMAL, 1);
                        }

                        // Move cursor back to the correct location.
                        self.window.mv(0, self.sketch.len() as i32);
                    }
                    None => {}
                }
            }
            Operation::AddToView(c) => match c {
                BACKSPACE => {
                    self.index -= 1;
                    self.view.remove(self.index);
                }
                _ => {
                    self.view.insert(self.index, c);
                    self.index += 1;
                }
            },
            Operation::ChangeMode(mode) => {
                match mode {
                    ModeType::Display => {
                        self.mode = Box::new(DisplayMode::new());
                        self.write_view();
                    }
                    ModeType::Command => {
                        self.mode = Box::new(CommandMode::new());
                        self.window.mv(0, 0);
                        self.sketch.clear();
                    }
                    ModeType::Filter => {
                        self.mode = Box::new(FilterMode::new());
                        self.window.mv(0, 0);
                        self.sketch.clear();
                    }
                    ModeType::Action => {
                        self.mode = Box::new(ActionMode::new());
                    }
                    ModeType::Edit => {
                        self.mode = Box::new(EditMode::new());
                        self.write_view();
                        self.sketch.clear();
                        self.window
                            .mv(self.filters[0].row as i32, (self.line_number_length as i32) + 1);
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
            Operation::InsertAbove => {
                self.index = self.calc_index(self.filters[0].row);
            }
            Operation::AppendBelow => {
                self.filters[0].row += 1;
                self.index = self.calc_index(self.filters[0].row);
            }
        }

        None
    }

    fn calc_index(&self, target: usize) -> usize {
        match target {
            0 => 0,
            _ => {
                let newline_indices: Vec<_> = self.view.match_indices("\n").collect();
                let (index, _) = *newline_indices.get(target - 1).unwrap();
                index
            }
        }
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
            self.window.addch('\n');
        }
    }

    fn window_height(&self) -> usize {
        self.window.get_max_y() as usize
    }

    fn scroll_height(&self) -> usize {
        self.window_height() / 4
    }
}
