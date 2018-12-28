use std::fmt;
use std::ops::{Add, BitOr};

use regex::Regex;

pub const OPT: RepeatConst = RepeatConst("?");
pub const VAR: RepeatConst = RepeatConst("*");
pub const SOME: RepeatConst = RepeatConst("+");
const LAZY: &str = "?";

pub trait Rec {
    fn reg_exp(&self) -> String;

    fn re(&self) -> Re {
        Re::new(self.reg_exp())
    }

    fn rpt(&self, rpt: impl Rpt) -> Re {
        self.re().repeat(rpt)
    }
}

impl<'a> Rec for ChCls<'a> {
    fn reg_exp(&self) -> String {
        match self {
            ChCls::AllBut(chars) => String::from("[^") + chars + "]",
            ChCls::Digit => String::from(r"\d"),
            ChCls::Any => String::from("."),
            ChCls::WhSpc => String::from(r"\s"),
            ChCls::End => String::from("$"),
        }
    }
}

impl<'a> Rec for &'a str {
    fn reg_exp(&self) -> String {
        String::from(*self).replace(".", r"\.").replace("+", r"\+")
    }
}

impl<'a> Add<Re> for &'a str {
    type Output = Re;

    fn add(self, other: Re) -> Re {
        self.re() + other
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
    fn new(expression: String) -> Re {
        Re { expression }
    }

    pub fn name(mut self, name: &str) -> Re {
        self.expression = String::from("(?P<") + name + ">" + &self.expression + ")";
        self
    }

    fn group(mut self) -> Re {
        let length = self.expression.chars().count();

        if length > 2 || (length == 2 && self.expression.chars().nth(0) != Some('\\')) {
            self.expression = String::from("(?:") + &self.expression + ")";
        }

        self
    }

    fn repeat(self, repeat: impl Rpt) -> Re {
        Re::new(self.group().expression + repeat.repr())
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

impl<'a> Add<&'a str> for Re {
    type Output = Re;

    fn add(self, other: &str) -> Re {
        self + other.re()
    }
}

impl Add<String> for Re {
    type Output = Re;

    fn add(self, other: String) -> Re {
        self + other.as_str().re()
    }
}

impl<'a> Add<ChCls<'a>> for Re {
    type Output = Re;

    fn add(self, other: ChCls<'a>) -> Re {
        self + other.re()
    }
}

impl BitOr for Re {
    type Output = Re;

    fn bitor(self, rhs: Re) -> Re {
        Re::new(self.expression + "|" + &rhs.expression).group()
    }
}

impl<'a> BitOr<&'a str> for Re {
    type Output = Re;

    fn bitor(self, rhs: &'a str) -> Re {
        self | rhs.re()
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
    End,
}

impl<'a, 'b> BitOr<&'a str> for ChCls<'b> {
    type Output = Re;

    fn bitor(self, rhs: &'a str) -> Re {
        self.re() | rhs.re()
    }
}

impl<'a> BitOr for ChCls<'a> {
    type Output = Re;

    fn bitor(self, rhs: ChCls<'a>) -> Re {
        self.re() | rhs.re()
    }
}
