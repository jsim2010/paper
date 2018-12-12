extern crate pancurses;
extern crate regex;

use pancurses::Input;
use regex::Regex;
use std::fs;
use std::cmp;

enum Mode {
    Display,
    Command,
}

pub struct Paper {
    window: pancurses::Window,
    command: String,
    mode: Mode,
    view: String,
    first_line: usize,
}

enum Operation {
    Noop,
    End,
    ChangeToCommand,
    ScrollDown,
    ScrollUp,
    SeeView(String),
    EditCommand(char),
}

enum Notice {
    Quit,
}

impl Paper {
    pub fn new() -> Paper {
        let window = pancurses::initscr();
        let command = String::new();
        let mode = Mode::Display;
        let view = String::new();
        let first_line = 0;

        // Prevent curses from outputing keys.
        pancurses::noecho();

        Paper {
            window,
            command,
            mode,
            view,
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
                self.command.clear();
                self.mode = Mode::Command;
            },
            Operation::ScrollDown => {
                self.first_line = cmp::min(self.first_line + self.scroll_height(), self.view.lines().count() - 1);
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
            Operation::EditCommand(c) => {
                self.window.addch(c);

                if c == '\u{08}' {
                    self.command.pop();
                    // Backspace moves cursor back one but does not delete the character.
                    self.window.delch();
                } else {
                    self.command.push(c);
                }
            },
            Operation::SeeView(path) => {
                self.mode = Mode::Display;
                self.view = fs::read_to_string(&path).unwrap();
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

    fn process_input(&self) -> Operation {
        match self.window.getch() {
            Some(Input::Character(c)) => self.process_char(c),
            _ => Operation::Noop,
        }
    }

    fn process_char(&self, c: char) -> Operation {
        match self.mode {
            Mode::Display => {
                match c {
                    '.' => Operation::ChangeToCommand,
                    'j' => Operation::ScrollDown,
                    'k' => Operation::ScrollUp,
                    _ => Operation::Noop,
                }
            },
            Mode::Command => {
                match c {
                    '\n' => {
                        let re = Regex::new(r"(?P<command>.+?)(?:\s|$)").unwrap();
                        let cmd = self.command.clone();

                        match re.captures(&cmd) {
                            Some(caps) => return self.process_command(&caps["command"]),
                            None => Operation::Noop,
                        }
                    },
                    _ => Operation::EditCommand(c),
                }
            },
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

    fn write_view(&mut self) {
        self.window.clear();
        self.window.mv(0, 0);
        let lines: Vec<&str> = self.view.lines().collect();
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
