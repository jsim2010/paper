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
}

enum ModeType {
    Display,
    Command,
    LineFilter,
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
    FilterLine(i32),
}

enum Notice {
    Quit,
}

trait Mode {
    fn handle_input(&self, c: char) -> Vec<Operation>;
    fn enhance(&self, _sketch: &String) -> Option<Enhancement> {
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
            '#' => operations.push(Operation::ChangeMode(ModeType::LineFilter)),
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
            '\n' => operations.push(Operation::ExecuteCommand),
            '' => operations.push(Operation::ChangeMode(ModeType::Display)),
            _ => operations.push(Operation::AddToSketch(c)),
        }

        operations
    }
}

struct LineFilterMode {}

impl LineFilterMode {
    fn new() -> LineFilterMode {
        LineFilterMode {}
    }
}

impl Mode for LineFilterMode {
    fn handle_input(&self, c: char) -> Vec<Operation> {
        let mut operations = Vec::new();

        match c {
            '0'...'9' | BACKSPACE => operations.push(Operation::AddToSketch(c)),
            '\n' => operations.push(Operation::ChangeMode(ModeType::Action)),
            '' => operations.push(Operation::ChangeMode(ModeType::Display)),
            _ => {}
        }

        operations
    }

    fn enhance(&self, sketch: &String) -> Option<Enhancement> {
        // Subtract 1 to match line index.
        sketch
            .parse::<i32>()
            .map(|i| i - 1)
            .ok()
            .map(|line| Enhancement::FilterLine(line))
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
            'A' => operations.push(Operation::AppendBelow),
            'I' => operations.push(Operation::InsertAbove),
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
        let window = pancurses::initscr();
        let first_line = 0;

        // Prevent curses from outputing keys.
        pancurses::noecho();

        pancurses::start_color();
        pancurses::use_default_colors();
        pancurses::init_pair(0, -1, -1);
        pancurses::init_pair(1, -1, pancurses::COLOR_BLUE);

        Paper {
            window,
            mode: Box::new(DisplayMode::new()),
            first_line,
            sketch: String::new(),
            view: String::new(),
            line_number_length: 0,
            index: 0,
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

    fn set_mode(&mut self, mode: ModeType) {
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
            ModeType::LineFilter => {
                self.mode = Box::new(LineFilterMode::new());
                self.window.mv(0, 0);
                self.sketch.clear();
            }
            ModeType::Action => {
                self.mode = Box::new(ActionMode::new());
            }
            ModeType::Edit => {
                self.mode = Box::new(EditMode::new());
                self.write_view();
            }
        }
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
                            self.set_mode(ModeType::Display);
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
                        // addch(BACKSPACE) moves cursor back 1, so delete char at cursor.
                        self.window.delch();
                        self.sketch.pop();
                    }
                    _ => {
                        self.sketch.push(c);
                    }
                }

                match self.mode.enhance(&self.sketch) {
                    Some(Enhancement::FilterLine(target)) => {
                        for line in 0..self.window_height() as i32 {
                            if line == target {
                                self.window.mvchgat(line, 0, -1, pancurses::A_NORMAL, 1);
                            } else {
                                self.window.mvchgat(line, 0, -1, pancurses::A_NORMAL, 0);
                            }
                        }
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
                self.set_mode(mode);
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
                let filter = self.sketch.to_string();
                let target_line = (filter.parse::<i32>().unwrap() - 1) as usize;
                let line_index = target_line - 1;

                if target_line == 0 {
                    self.index = 0;
                } else {
                    let newline_indices: Vec<_> = self.view.match_indices("\n").collect();
                    let (index, _) = *newline_indices.get(line_index).unwrap();
                    self.index = index;
                }

                self.view.insert(self.index, '\n');
                self.index += 1;
                self.sketch = String::from("");
                self.set_mode(ModeType::Edit);
                self.window
                    .mv(target_line as i32, (self.line_number_length as i32) + 1);
            }
            Operation::AppendBelow => {
                let filter = self.sketch.to_string();
                let target_line = filter.parse::<i32>().unwrap() as usize;
                let line_index = target_line - 1;

                {
                    let newline_indices: Vec<_> = self.view.match_indices("\n").collect();
                    let (index, _) = *newline_indices.get(line_index).unwrap();
                    self.index = index;
                }

                self.view.insert(self.index, '\n');
                self.index += 1;
                self.sketch = String::from("");
                self.set_mode(ModeType::Edit);
                self.window
                    .mv(target_line as i32, (self.line_number_length as i32) + 1);
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
