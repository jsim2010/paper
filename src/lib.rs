extern crate pancurses;
extern crate regex;

use pancurses::Input;
use regex::Regex;
use std::fs;
use std::cmp;

const BACKSPACE: char = '\u{08}';

pub struct Paper {
    window: pancurses::Window,
    mode: Box<dyn Mode>,
    first_line: usize,
}

enum Operation {
    Noop,
    End,
    ChangeToCommand,
    ScrollDown,
    ScrollUp,
    SeeView(String),
    DeleteBack,
    AppendText(char),
}

enum Notice {
    Quit,
}

trait Mode {
    fn handle_input(&mut self, c: char) -> Operation;
    fn text(&self) -> &String;
}

struct DisplayMode {
    view: String
}

struct CommandMode {
    command: String,
}

impl DisplayMode {
    fn new(view: String) -> DisplayMode {
        DisplayMode {
            view
        }
    }
}

impl Mode for DisplayMode {
    fn handle_input(&mut self, c: char) -> Operation {
        match c {
            '.' => Operation::ChangeToCommand,
            'j' => Operation::ScrollDown,
            'k' => Operation::ScrollUp,
            _ => Operation::Noop,
        }
    }

    fn text(&self) -> &String {
        &self.view
    }
}

impl CommandMode {
    fn new() -> CommandMode {
        CommandMode {
            command: String::new(),
        }
    }

    fn process_command(&self, command: &str) -> Operation {
        match command {
            "see" => {
                let re = Regex::new(r"see\s*(?P<path>.*)").unwrap();
                Operation::SeeView(re.captures(&self.command).unwrap()["path"].to_string())
            },
            "end" => Operation::End,
            _ => Operation::Noop,
        }
    }
}

impl Mode for CommandMode {
    fn handle_input(&mut self, c: char) -> Operation {
        match c {
            '\n' => {
                let re = Regex::new(r"(?P<command>.+?)(?:\s|$)").unwrap();
                let cmd = self.command.clone();

                match re.captures(&cmd) {
                    Some(caps) => return self.process_command(&caps["command"]),
                    None => Operation::Noop,
                }
            },
            BACKSPACE => {
                self.command.pop();
                Operation::DeleteBack
            },
            _ => {
                self.command.push(c);
                Operation::AppendText(c)
            },
        }
    }

    fn text(&self) -> &String {
        &self.command
    }
}

impl Paper {
    pub fn new() -> Paper {
        let window = pancurses::initscr();
        let first_line = 0;
        let mode = Box::new(DisplayMode::new(String::new()));

        // Prevent curses from outputing keys.
        pancurses::noecho();

        Paper {
            window,
            mode,
            first_line,
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
            Operation::ChangeToCommand => {
                self.window.mv(0, 0);
                self.mode = Box::new(CommandMode::new());
            },
            Operation::ScrollDown => {
                self.first_line = cmp::min(self.first_line + self.scroll_height(), self.mode.text().lines().count() - 1);
                self.write_view();
            },
            Operation::ScrollUp => {
                let movement = self.scroll_height();

                if self.first_line < movement {
                    self.first_line = 0;
                } else {
                    self.first_line -= movement;
                }
                self.write_view();
            },
            Operation::DeleteBack => {
                // Move cursor back one space.
                self.window.addch(BACKSPACE);

                // Delete character at cursor.
                self.window.delch();
            },
            Operation::AppendText(c) => {
                self.window.addch(c);
            },
            Operation::SeeView(path) => {
                self.mode = Box::new(DisplayMode::new(fs::read_to_string(&path).unwrap()));
                self.first_line = 0;
                self.write_view();
            },
            Operation::End => {
                return Some(Notice::Quit)
            },
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
        let lines: Vec<&str> = self.mode.text().lines().collect();
        let max = cmp::min(self.window_height() + self.first_line, lines.len());

        for line in lines[self.first_line..max].iter() {
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
