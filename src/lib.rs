extern crate pancurses;
extern crate regex;

use pancurses::Input;
use regex::Regex;
use std::fs;

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
}

impl Paper {
    fn new() -> Paper {
        let window = pancurses::initscr();
        let command = String::new();
        let mode = Mode::Display;

        window.keypad(true);
        // Prevent curses from outputing keys.
        pancurses::noecho();

        Paper {
            window,
            command,
            mode,
        }
    }

    fn process_input(&mut self) -> Result<(), ()> {
        match self.window.getch() {
            Some(Input::Character('')) => return Err(()),
            Some(Input::Character(c)) => return self.process_char(c),
            _ => (),
        }
        
        Ok(())
    }

    fn process_char(&mut self, c: char) -> Result<(), ()> {
        match self.mode {
            Mode::Display => {
                if c == '.' {
                    self.mode = Mode::Command;
                    self.command.clear();
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
                    self.command.push(c);
                    self.window.addch(c);
                }
            },
        }

        Ok(())
    }

    fn process_command(&mut self, command: &str) -> Result<(), ()> {
        match command {
            "see" => {
                let re = Regex::new(r"see\s*(?P<file>.*)").unwrap();
                let filename = &re.captures(&self.command).unwrap()["file"];
                self.mode = Mode::Display;
                self.window.mv(0, 0);
                
                for ch in fs::read_to_string(&filename).unwrap().chars() {
                    match ch {
                        '\r' => (),
                        '\n' => {
                            if self.window.get_cur_y() + 1 < self.window.get_max_y() {
                                self.window.addch('\n');
                            } else {
                                break;
                            }
                        },
                        h => {
                            self.window.addch(h);
                        },
                    }
                }
            },
            "end" => return Err(()),
            _ => return Err(()),
        }

        Ok(())
    }
}
