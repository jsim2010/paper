extern crate pancurses;
extern crate regex;

use pancurses::Input;
use regex::Regex;
use std::cmp;
use std::fs;

const BACKSPACE: char = '\u{08}';

pub struct Paper {
    window: pancurses::Window,
    mode: Box<dyn Mode>,
    view: String,
    sketch: String,
    first_line: usize,
    line_number_length: usize,
}

enum Operation {
    Noop,
    ChangeToDisplay,
    ChangeToCommand,
    ChangeToLineFilter,
    ChangeToAction,
    ExecuteCommand,
    ScrollDown,
    ScrollUp,
    AppendChar(char),
    InsertAbove,
}

enum Notice {
    Quit,
}

trait Mode {
    fn handle_input(&mut self, c: char) -> Operation;
    fn process_sketch(&self, sketch: &String, window: &mut pancurses::Window);
}

struct DisplayMode {
}

impl DisplayMode {
    fn new() -> DisplayMode {
        DisplayMode { }
    }
}

impl Mode for DisplayMode {
    fn handle_input(&mut self, c: char) -> Operation {
        match c {
            '.' => Operation::ChangeToCommand,
            '#' => Operation::ChangeToLineFilter,
            'j' => Operation::ScrollDown,
            'k' => Operation::ScrollUp,
            _ => Operation::Noop,
        }
    }

    fn process_sketch(&self, _sketch: &String, _window: &mut pancurses::Window) {
    }
}

struct CommandMode {
}

impl CommandMode {
    fn new() -> CommandMode {
        CommandMode { }
    }
}

impl Mode for CommandMode {
    fn handle_input(&mut self, c: char) -> Operation {
        match c {
            '\n' => Operation::ExecuteCommand,
            '' => Operation::ChangeToDisplay,
            _ => Operation::AppendChar(c),
        }
    }

    fn process_sketch(&self, _sketch: &String, _window: &mut pancurses::Window) {
    }
}

struct LineFilterMode {
    window_height: usize,
}

impl LineFilterMode {
    fn new(window_height: usize) -> LineFilterMode {
        LineFilterMode {
            window_height,
        }
    }
}

impl Mode for LineFilterMode {
    fn handle_input(&mut self, c: char) -> Operation {
        match c {
            '0'...'9' | BACKSPACE => Operation::AppendChar(c),
            '\n' => Operation::ChangeToAction,
            '' => Operation::ChangeToDisplay,
            _ => Operation::Noop,
        }
    }

    fn process_sketch(&self, sketch: &String, window: &mut pancurses::Window) {
        // Subtract 1 to match line index.
        let target_line = sketch.parse::<i32>().map(|i| i - 1).ok();

        for line in 0..self.window_height {
            let line = line as i32;

            if Some(line) == target_line {
                window.mvchgat(line, 0, -1, pancurses::A_NORMAL, 1);
            } else {
                window.mvchgat(line, 0, -1, pancurses::A_NORMAL, 0);
            }
        }
    }
}

struct ActionMode {
}

impl ActionMode {
    fn new() -> ActionMode {
        ActionMode { }
    }
}

impl Mode for ActionMode {
    fn handle_input(&mut self, c: char)  -> Operation {
        match c {
            'I' => Operation::InsertAbove,
            _ => Operation::Noop,
        }
    }

    fn process_sketch(&self, _sketch: &String, _window: &mut pancurses::Window) {
    }
}

struct EditMode {
}

impl EditMode {
    fn new() -> EditMode {
        EditMode { }
    }
}

impl Mode for EditMode {
    fn handle_input(&mut self, c:char) -> Operation {
        match c {
            '' => Operation::ChangeToDisplay,
            _ => Operation::AppendChar(c),
        }
    }

    fn process_sketch(&self, _sketch: &String, _window: &mut pancurses::Window) {
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
        }
    }

    pub fn run(&mut self) {
        loop {
            let operation = self.process_input();

            match self.operate(operation) {
                Some(Notice::Quit) => break,
                None => (),
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
                    Some(caps) => {
                        match &caps["command"] {
                            "see" => {
                                let see_re = Regex::new(r"see\s*(?P<path>.*)").unwrap();
                                let path = see_re.captures(&self.sketch).unwrap()["path"].to_string();
                                self.view = fs::read_to_string(&path).unwrap();
                                self.mode = Box::new(DisplayMode::new());
                                self.first_line = 0;
                                self.write_view();
                            }
                            "end" => return Some(Notice::Quit),
                            _ => {}
                        }
                    }
                    None => {}
                }
            }
            Operation::AppendChar(c) => {
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

                self.mode.process_sketch(&self.sketch, &mut self.window);
            }
            Operation::ChangeToDisplay => {
                self.mode = Box::new(DisplayMode::new());
                self.write_view();
            }
            Operation::ChangeToCommand => {
                self.window.mv(0, 0);
                self.mode = Box::new(CommandMode::new());
                self.sketch.clear();
            }
            Operation::ChangeToLineFilter => {
                self.sketch.clear();
                let window_height = self.window_height();
                self.mode = Box::new(LineFilterMode::new(window_height));
                self.window.mv(0, 0);
            }
            Operation::ChangeToAction => {
                self.mode = Box::new(ActionMode::new());
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
                let old_view = self.view.clone();
                let mut lines: Vec<&str> = old_view.lines().collect();
                let target_line = filter.parse::<i32>().map(|i| i - 1).ok();

                match target_line {
                    Some(line) => lines.insert(line as usize, ""),
                    None => {},
                }

                self.view = lines.join("\n");
                // TODO: Store index of view at which to insert new chars in EditMode.
                self.sketch = String::from("");
                self.mode = Box::new(EditMode::new());
                self.write_view();
                self.window.mv(target_line.unwrap(), (self.line_number_length as i32) + 1);
            }
            Operation::Noop => (),
        }

        None
    }

    fn process_input(&mut self) -> Operation {
        match self.window.getch() {
            Some(Input::Character(c)) => self.mode.handle_input(c),
            _ => Operation::Noop,
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
