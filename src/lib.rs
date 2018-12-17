extern crate pancurses;
extern crate regex;

use pancurses::Input;
use regex::Regex;
use std::cmp;
use std::fs;

/// Character that represents Backspace key.
const BACKSPACE: char = '\u{08}';

const DISPLAY_MODE: Mode = Mode::Display {};
const COMMAND_MODE: Mode = Mode::Command {};
const FILTER_MODE: Mode = Mode::Filter {};
const ACTION_MODE: Mode = Mode::Action {};
const EDIT_MODE: Mode = Mode::Edit {};

/// All data related to paper application.
pub struct Paper {
    window: pancurses::Window,
    mode: Mode,
    view: String,
    sketch: String,
    first_line: usize,
    line_number_length: usize,
    index: usize,
    filters: Vec<Filter>,
}

struct Filter {
    row: usize,
    column: usize,
    length: i32,
}

#[derive(PartialEq, Eq)]
enum Mode {
    Display {},
    Command {},
    Filter {},
    Action {},
    Edit {},
}

enum Operation {
    ChangeMode(Mode),
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
                '\n' => {
                    operations.push(Operation::ExecuteCommand);
                    operations.push(Operation::ChangeMode(DISPLAY_MODE));
                }
                '' => operations.push(Operation::ChangeMode(DISPLAY_MODE)),
                _ => operations.push(Operation::AddToSketch(c)),
            },
            FILTER_MODE => match c {
                '\n' => operations.push(Operation::ChangeMode(ACTION_MODE)),
                '' => operations.push(Operation::ChangeMode(DISPLAY_MODE)),
                _ => operations.push(Operation::AddToSketch(c)),
            },
            ACTION_MODE => match c {
                'A' => {
                    operations.push(Operation::AppendBelow);
                    operations.push(Operation::AddToView('\n'));
                    operations.push(Operation::ChangeMode(EDIT_MODE));
                }
                'I' => {
                    operations.push(Operation::InsertAbove);
                    operations.push(Operation::AddToView('\n'));
                    operations.push(Operation::ChangeMode(EDIT_MODE));
                }
                _ => {}
            },
            EDIT_MODE => match c {
                '' => operations.push(Operation::ChangeMode(DISPLAY_MODE)),
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
                let mut filters = Vec::new();

                match &re.captures(sketch) {
                    Some(captures) => {
                        if let Some(line) = captures.name("line") {
                            // Subtract 1 to match row.
                            line.as_str().parse::<i32>().map(|i| i - 1).ok().map(|r| {
                                filters.push(Filter {
                                    row: (r as usize),
                                    column: 0,
                                    length: -1,
                                })
                            });
                        }

                        if let Some(key) = captures.name("key") {
                            let length = key.as_str().len() as i32;

                            for (row, line) in view.lines().enumerate() {
                                for (key_index, _) in line.match_indices(key.as_str()) {
                                    filters.push(Filter {
                                        row: (row as usize),
                                        column: key_index,
                                        length,
                                    });
                                }
                            }
                        }
                    }
                    None => {}
                }

                Some(Enhancement::Filters(filters))
            }
            DISPLAY_MODE | COMMAND_MODE | ACTION_MODE | EDIT_MODE => None,
        }
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
            mode: DISPLAY_MODE,
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
                            self.window
                                .mvchgat(line as i32, 0, -1, pancurses::A_NORMAL, 0);
                        }

                        self.filters = filters;

                        for filter in self.filters.iter() {
                            self.window.mvchgat(
                                filter.row as i32,
                                (filter.column + self.line_number_length + 1) as i32,
                                filter.length,
                                pancurses::A_NORMAL,
                                1,
                            );
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
                self.mode = mode;

                match self.mode {
                    DISPLAY_MODE => {
                        self.write_view();
                    }
                    COMMAND_MODE | FILTER_MODE => {
                        self.window.mv(0, 0);
                        self.sketch.clear();
                    }
                    ACTION_MODE => {}
                    EDIT_MODE => {
                        self.write_view();
                        self.sketch.clear();
                        self.window.mv(
                            self.filters[0].row as i32,
                            (self.line_number_length as i32) + 1,
                        );
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
