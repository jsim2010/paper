use std::fmt;
use std::ops::{Add, BitOr};

use regex::Regex;

pub trait Rec {
    fn regex(&self) -> String;

    fn re(&self) -> Re {
        Re {
            expression: self.regex(),
        }
    }

    fn rpt(&self, repeat: Rpt) -> Re {
        Re {
            expression: String::from("(?:") + &self.regex() + ")" + repeat.as_str(),
        }
    }
}

impl<'a> Rec for ChCls<'a> {
    fn regex(&self) -> String {
        match self {
            ChCls::AllBut(chars) => String::from("[^") + chars + "]",
            ChCls::Digit => String::from(r"\d"),
            ChCls::All => String::from("."),
        }
    }
}

impl<'a> Rec for &'a str {
    fn regex(&self) -> String {
        String::from(*self)
    }
}

#[derive(Default)]
pub struct Re {
    expression: String,
}

impl Re {
    pub fn name(mut self, name: &str) -> Re {
        self.expression = String::from("(?P<") + name + ">" + &self.expression + ")";
        self
    }

    pub fn build(&self) -> Result<Regex, regex::Error> {
        Regex::new(&self.expression)
    }

    pub fn form(&self) -> Regex {
        Regex::new(&self.expression).unwrap()
    }
}

impl Add for Re {
    type Output = Re;

    fn add(self, other: Re) -> Re {
        Re {
            expression: self.expression + &other.expression,
        }
    }
}

impl<'a> Add<Re> for &'a str {
    type Output = Re;

    fn add(self, other: Re) -> Re {
        Re {
            expression: String::from(self) + &other.expression,
        }
    }
}

impl BitOr for Re {
    type Output = Re;

    fn bitor(self, rhs: Re) -> Re {
        Re {
            expression: self.expression + "|" + &rhs.expression,
        }
    }
}

impl fmt::Display for Re {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.expression)
    }
}

pub enum Rpt {
    Opt,
    Any,
    Mult,
}

impl Rpt {
    fn as_str(self) -> &'static str {
        match self {
            Rpt::Opt => "?",
            Rpt::Any => "*",
            Rpt::Mult => "+",
        }
    }
}

pub enum ChCls<'a> {
    All,
    AllBut(&'a str),
    Digit,
}
