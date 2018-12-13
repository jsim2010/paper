extern crate pancurses;
extern crate regex;

use pancurses::Input;
use regex::Regex;
use std::cmp;
use std::fs;

const BACKSPACE: char = '\u{08}';

pub struct Paper {
    window: pancurses::Window,
    modes: Vec<Box<dyn Mode>>,
    first_line: usize,
}

enum Operation {
    Noop,
    End,
    ChangeToDisplay,
    ChangeToCommand,
    ChangeToLineFilter,
    ScrollDown,
    ScrollUp,
    SeeView(String),
    DeleteBack,
    AppendText(char),
    FilterLines(String),
}

enum Notice {
    Quit,
}

trait Mode {
    fn handle_input(&mut self, c: char) -> Operation;
    fn text(&self) -> &String;
}

struct DisplayMode {
    view: String,
}

impl DisplayMode {
    fn new(view: String) -> DisplayMode {
        DisplayMode { view }
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

    fn text(&self) -> &String {
        &self.view
    }
}

struct CommandMode {
    command: String,
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
            }
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
            }
            BACKSPACE => {
                self.command.pop();
                Operation::DeleteBack
            }
            '' => Operation::ChangeToDisplay,
            _ => {
                self.command.push(c);
                Operation::AppendText(c)
            }
        }
    }

    fn text(&self) -> &String {
        &self.command
    }
}

struct LineFilterMode {
    text: String,
}

impl LineFilterMode {
    fn new() -> LineFilterMode {
        LineFilterMode {
            text: String::new(),
        }
    }
}

impl Mode for LineFilterMode {
    fn handle_input(&mut self, c: char) -> Operation {
        match c {
            '0'...'9' => {
                self.text.push(c);
                Operation::FilterLines(self.text.clone())
            }
            BACKSPACE => {
                self.text.pop();
                Operation::FilterLines(self.text.clone())
            }
            '' => Operation::ChangeToDisplay,
            _ => Operation::Noop,
        }
    }

    fn text(&self) -> &String {
        &self.text
    }
}

impl Paper {
    pub fn new() -> Paper {
        let window = pancurses::initscr();
        let first_line = 0;
        let modes: Vec<Box<(dyn Mode)>> = vec![Box::new(DisplayMode::new(String::new()))];

        // Prevent curses from outputing keys.
        pancurses::noecho();

        pancurses::start_color();
        pancurses::use_default_colors();
        pancurses::init_pair(0, -1, -1);
        pancurses::init_pair(1, -1, pancurses::COLOR_BLUE);

        Paper {
            window,
            modes,
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
            Operation::ChangeToDisplay => {
                self.modes.truncate(1);
                self.write_view();
            }
            Operation::ChangeToCommand => {
                self.window.mv(0, 0);
                self.modes.push(Box::new(CommandMode::new()));
            }
            Operation::ChangeToLineFilter => {
                self.modes.push(Box::new(LineFilterMode::new()));
            }
            Operation::ScrollDown => {
                self.first_line = cmp::min(
                    self.first_line + self.scroll_height(),
                    self.modes.last().unwrap().text().lines().count() - 1,
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
            Operation::DeleteBack => {
                // Move cursor back one space.
                self.window.addch(BACKSPACE);

                // Delete character at cursor.
                self.window.delch();
            }
            Operation::AppendText(c) => {
                self.window.addch(c);
            }
            Operation::SeeView(path) => {
                self.modes.clear();
                self.modes.push(Box::new(DisplayMode::new(
                    fs::read_to_string(&path).unwrap(),
                )));
                self.first_line = 0;
                self.write_view();
            }
            Operation::FilterLines(lines) => {
                // Subtract 1 to match line index.
                let target_line = lines.parse::<i32>().map(|i| i - 1).ok();

                for line in 0..self.window_height() {
                    let line = line as i32;

                    if Some(line) == target_line {
                        self.window.mvchgat(line, 0, -1, pancurses::A_NORMAL, 1);
                    } else {
                        self.window.mvchgat(line, 0, -1, pancurses::A_NORMAL, 0);
                    }
                }
            }
            Operation::End => return Some(Notice::Quit),
            Operation::Noop => (),
        }

        None
    }

    fn process_input(&mut self) -> Operation {
        match self.window.getch() {
            Some(Input::Character(c)) => self.modes.last_mut().unwrap().handle_input(c),
            _ => Operation::Noop,
        }
    }

    fn write_view(&mut self) {
        self.window.clear();
        self.window.mv(0, 0);
        let lines: Vec<&str> = self.modes.last().unwrap().text().lines().collect();
        let length = lines.len();
        let line_length = ((length as f32).log10() as usize) + 2;
        let max = cmp::min(self.window_height() + self.first_line, length);

        for (index, line) in lines[self.first_line..max].iter().enumerate() {
            self.window.addstr(format!(
                "{:>width$} ",
                index + self.first_line + 1,
                width = line_length
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
