use std::fmt;
use std::ops::{Add, BitOr};

use regex::{CaptureMatches, Captures, Regex};

pub const OPT: ConstantQuantifier = ConstantQuantifier("?");
pub const VAR: ConstantQuantifier = ConstantQuantifier("*");
pub const SOME: ConstantQuantifier = ConstantQuantifier("+");
const LAZY: &str = "?";

#[derive(Debug)]
pub struct Pattern {
    re: Regex,
}

impl Pattern {
    /// Assumes rec is valid.
    pub fn define(rec: Rec) -> Pattern {
        Pattern { re: rec.build() }
    }

    pub fn tokenize<'t>(&self, target: &'t str) -> Tokens<'t> {
        Tokens::new(self.re.captures(target))
    }

    pub fn tokenize_iter<'r, 't>(&'r self, target: &'t str) -> TokensIter<'r, 't> {
        TokensIter::new(self.re.captures_iter(target))
    }
}

impl Default for Pattern {
    fn default() -> Pattern {
        Pattern {
            re: Regex::new("").unwrap(),
        }
    }
}

pub struct Tokens<'t> {
    captures: Option<Captures<'t>>,
}

impl<'t> Tokens<'t> {
    fn new(captures: Option<Captures<'t>>) -> Tokens<'t> {
        Tokens { captures }
    }

    pub fn get<'a>(&self, name: &'a str) -> Option<&'t str> {
        self.captures
            .as_ref()
            .and_then(|c| c.name(name).map(|x| x.as_str()))
    }
}

pub struct TokensIter<'r, 't> {
    capture_matches: CaptureMatches<'r, 't>,
}

impl<'r, 't> TokensIter<'r, 't> {
    fn new(capture_matches: CaptureMatches<'r, 't>) -> TokensIter<'r, 't> {
        TokensIter { capture_matches }
    }
}

impl<'r, 't> Iterator for TokensIter<'r, 't> {
    type Item = Tokens<'t>;

    fn next(&mut self) -> Option<Tokens<'t>> {
        self.capture_matches
            .next()
            .and_then(|x| Some(Tokens::new(Some(x))))
    }
}

#[derive(Debug, Default)]
pub struct Rec(String);

impl Rec {
    pub fn name(self, name: &str) -> Rec {
        Rec(String::from("(?P<") + name + ">" + &self.0 + ")")
    }

    fn group(self) -> Rec {
        let length = self.0.chars().count();

        if length > 2 || (length == 2 && self.0.chars().nth(0) != Some('\\')) {
            return Rec(String::from("(?:") + &self.0 + ")");
        }

        self
    }

    fn quantify(self, quantifier: impl Quantifier) -> Rec {
        Rec(self.group().0 + quantifier.regex())
    }

    pub fn build(&self) -> Regex {
        self.try_build().unwrap()
    }

    pub fn try_build(&self) -> Result<Regex, regex::Error> {
        Regex::new(&self.0)
    }
}

impl Add for Rec {
    type Output = Rec;

    fn add(self, other: Rec) -> Rec {
        Rec(self.0 + &other.0)
    }
}

impl<T> Add<T> for Rec
where
    T: Atom,
{
    type Output = Rec;

    fn add(self, other: T) -> Rec {
        self + other.rec()
    }
}

impl BitOr for Rec {
    type Output = Rec;

    fn bitor(self, rhs: Rec) -> Rec {
        Rec(self.0 + "|" + &rhs.0).group()
    }
}

impl<T> BitOr<T> for Rec
where
    T: Atom,
{
    type Output = Rec;

    fn bitor(self, rhs: T) -> Rec {
        self | rhs.rec()
    }
}

impl fmt::Display for Rec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
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
    type Output = Rec;

    fn bitor(self, rhs: &'a str) -> Rec {
        self.rec() | rhs.rec()
    }
}

impl<'a> BitOr for ChCls<'a> {
    type Output = Rec;

    fn bitor(self, rhs: ChCls<'a>) -> Rec {
        self.rec() | rhs.rec()
    }
}

pub trait Atom {
    fn regex(&self) -> String;

    fn rec(&self) -> Rec {
        Rec(self.regex())
    }

    fn rpt(&self, quantifier: impl Quantifier) -> Rec {
        self.rec().quantify(quantifier)
    }
}

impl<'a> Atom for ChCls<'a> {
    fn regex(&self) -> String {
        match self {
            ChCls::AllBut(chars) => String::from("[^") + chars + "]",
            ChCls::Digit => String::from(r"\d"),
            ChCls::Any => String::from("."),
            ChCls::WhSpc => String::from(r"\s"),
            ChCls::End => String::from("$"),
        }
    }
}

pub trait Quantifier {
    fn regex(&self) -> &str;

    fn lazy(&self) -> Repeat {
        Repeat(String::from(self.regex()) + LAZY)
    }
}

pub struct ConstantQuantifier<'a>(&'a str);
pub struct Repeat(String);

impl<'a> Quantifier for ConstantQuantifier<'a> {
    fn regex(&self) -> &str {
        self.0
    }
}

impl Quantifier for Repeat {
    fn regex(&self) -> &str {
        self.0.as_str()
    }
}

impl<'a> Atom for &'a str {
    fn regex(&self) -> String {
        self.replace(".", r"\.").replace("+", r"\+")
    }
}

impl<'a> Add<Rec> for &'a str {
    type Output = Rec;

    fn add(self, other: Rec) -> Rec {
        self.rec() + other
    }
}
