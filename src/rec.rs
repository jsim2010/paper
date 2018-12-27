use std::fmt;
use std::ops::{Add, BitOr};

use regex::Regex;

pub const OPT: RepeatConst = RepeatConst("?");
pub const VAR: RepeatConst = RepeatConst("*");
pub const SOME: RepeatConst = RepeatConst("+");
const LAZY: &str = "?";

pub trait Rec {
    fn regex(&self) -> String;

    fn re(&self) -> Re {
        Re::new(self.regex())
    }

    fn rpt(&self, rpt: impl Rpt) -> Re {
        self.re().group() + rpt.repr()
    }
}

impl<'a> Rec for ChCls<'a> {
    fn regex(&self) -> String {
        match self {
            ChCls::AllBut(chars) => String::from("[^") + chars + "]",
            ChCls::Digit => String::from(r"\d"),
            ChCls::Any => String::from("."),
            ChCls::WhSpc => String::from(r"\s"),
        }
    }
}

impl<'a> Rec for &'a str {
    fn regex(&self) -> String {
        String::from(*self)
    }
}

pub trait Rpt {
    fn repr(&self) -> &str;
    
    fn lazy(&self) -> Repeat {
        Repeat(String::from(self.repr()) + LAZY)
    }
}

pub struct RepeatConst<'a>(&'a str);
pub struct Repeat(String);

impl<'a> Rpt for RepeatConst<'a> {
    fn repr(&self) -> &str {
        self.0
    }
}

impl Rpt for Repeat {
    fn repr(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Default)]
pub struct Re {
    expression: String,
}

impl Re {
    fn new (expression: String) -> Re {
        Re {expression}
    }

    pub fn name(mut self, name: &str) -> Re {
        self.expression = String::from("(?P<") + name + ">" + &self.expression + ")";
        self
    }

    fn group(mut self) -> Re {
        self.expression = String::from("(?:") + &self.expression + ")";
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
        Re::new(self.expression + &other.expression)
    }
}

impl<'a> Add<Re> for &'a str {
    type Output = Re;

    fn add(self, other: Re) -> Re {
        Re::new(String::from(self) + &other.expression)
    }
}

impl<'a> Add<&'a str> for Re {
    type Output = Re;

    fn add(self, other: &str) -> Re {
        Re::new(self.expression + other)
    }
}

impl Add<String> for Re {
    type Output = Re;

    fn add(self, other: String) -> Re {
        Re::new(self.expression + &other)
    }
}

impl BitOr for Re {
    type Output = Re;

    fn bitor(self, rhs: Re) -> Re {
        Re::new(self.expression + "|" + &rhs.expression).group()
    }
}

impl fmt::Display for Re {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.expression)
    }
}

pub enum ChCls<'a> {
    Any,
    AllBut(&'a str),
    Digit,
    WhSpc,
}

impl<'a, 'b> BitOr<&'a str> for ChCls<'b> {
    type Output = Re;

    fn bitor(self, rhs: &'a str) -> Re {
        self.re() | rhs.re()
    }
}
