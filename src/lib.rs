extern crate pancurses;

use pancurses::Input;
use std::fs;

/// Runs an instance of paper.
pub fn run() -> Result<(), &'static str> {
    let mut paper = Paper::new();

    loop {
        match paper.window.getch() {
            Some(Input::Character('')) => break,
            Some(Input::Character(c)) => paper.process_char(c),
            Some(_) => continue,
            None => (),
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

    fn process_char(&mut self, c: char) {
        match self.mode {
            Mode::Display => {
                if c == '.' {
                    self.mode = Mode::Command;
                    self.command.clear();
                }
            },
            Mode::Command => {
                if c == '\n' {
                    self.mode = Mode::Display;
                    self.window.mv(0, 0);
                    
                    for ch in fs::read_to_string(&self.command).unwrap().chars() {
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
                } else {
                    self.command.push(c);
                    self.window.addch(c);
                }
            },
        }
    }
}
