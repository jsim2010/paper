extern crate pancurses;
extern crate regex;

use pancurses::Input;
use regex::Regex;
use std::fs;
use std::cmp;

/// Runs an instance of paper.
pub fn run() -> Result<(), &'static str> {
    let mut paper = Paper::new();

    loop {
        if let Err(()) = paper.process_input() {
            break;
        }
    }

    pancurses::endwin();

    Ok(())
}

enum Mode {
    Display,
    Command,
}

struct Paper {
    window: pancurses::Window,
    command: String,
    mode: Mode,
    view: String,
    first_line: usize,
}

impl Paper {
    fn new() -> Paper {
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

    fn process_input(&mut self) -> Result<(), ()> {
        match self.window.getch() {
            Some(Input::Character(c)) => return self.process_char(c),
            _ => (),
        }
        
        Ok(())
    }

    fn process_char(&mut self, c: char) -> Result<(), ()> {
        match self.mode {
            Mode::Display => {
                match c {
                    '.' => {
                        self.window.mv(0, 0);
                        self.mode = Mode::Command;
                        self.command.clear();
                    },
                    'j' => {
                        self.first_line = cmp::min(self.first_line + self.quarter_window_height(), self.view.lines().count() - 1);
                        self.write_view();
                    },
                    'k' => {
                        let movement = self.quarter_window_height();

                        if self.first_line < movement {
                            self.first_line = 0;
                        } else {
                            self.first_line -= movement;
                        }
                        self.write_view();
                    },
                    _ => (),
                }
            },
            Mode::Command => {
                if c == '\n' {
                    let re = Regex::new(r"(?P<command>.+?)(?:\s|$)").unwrap();
                    let cmd = self.command.clone();

                    match re.captures(&cmd) {
                        Some(caps) => return self.process_command(&caps["command"]),
                        None => (),
                    }
                } else {
                    self.window.addch(c);

                    if c == '\u{08}' {
                        self.command.pop();
                        // Backspace moves cursor back one but does not delete the character.
                        self.window.delch();
                    } else {
                        self.command.push(c);
                    }
                }
            },
        }

        Ok(())
    }

    fn process_command(&mut self, command: &str) -> Result<(), ()> {
        match command {
            "see" => {
                let re = Regex::new(r"see\s*(?P<file>.*)").unwrap();
                {
                    let filename = &re.captures(&self.command).unwrap()["file"];

                    self.mode = Mode::Display;
                    self.view = fs::read_to_string(&filename).unwrap();
                }
                self.first_line = 0;
                self.write_view();
            },
            "end" => return Err(()),
            _ => (),
        }

        Ok(())
    }

    fn write_view(&mut self) {
        self.window.clear();
        self.window.mv(0, 0);
        let lines: Vec<&str> = self.view.lines().collect();
        let max = cmp::min((self.window.get_max_y() as usize) + self.first_line, lines.len());

        for line in lines[self.first_line..max].iter() {
            self.window.addstr(line);
            self.window.addch('\n');
        }
    }

    fn quarter_window_height(&self) -> usize {
        (self.window.get_max_y() as usize) / 4
    }
}
