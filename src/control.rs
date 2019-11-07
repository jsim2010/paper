//! Control
use core::{fmt::{self, Display, Formatter}, str::FromStr};
use lazy_static::lazy_static;
use rec::{tkn, Pattern, Class, prelude::*, opt, lazy_some, some};
use std::string::ToString;

/// Ability
#[derive(Clone, Debug)]
pub(crate) enum Ability {
    /// Command
    Command(Option<Command>),
    /// Pattern
    Pattern(Option<Pattern>),
    // TODO: Add Structure to highlight different items.
}

impl Default for Ability {
    fn default() -> Self {
        Ability::Command(None)
    }
}

/// An error while parsing [`Ability`].
#[derive(Clone, Copy, Debug)]
pub(crate) enum ParseAbilityError {
    /// InvalidAbility
    Ability(Option<char>),
}

impl FromStr for Ability {
    type Err = ParseAbilityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        lazy_static! {
            static ref ABILITY_PATTERN: Pattern = Pattern::new(
                Class::Start
                    + (tkn!("command_ability" => '.' + opt(tkn!("command" => lazy_some(Class::Any)) + opt(some(Class::Whitespace) + opt(tkn!("args" => some(Class::Any))))))
                        | tkn!("filter_ability" => '/' + opt(tkn!("filter" => some(Class::Any)))))
                    + Class::End
            );
        }

        let ability_tokens = ABILITY_PATTERN
            .tokenize(s)
            .ok_or_else(|| ParseAbilityError::Ability(s.chars().nth(0)))?;

        if ability_tokens.name("command_ability").is_some() {
            Ok(Ability::Command(ability_tokens.name_str("command").map(
                |command| match command {
                    "see" => Command::See(ability_tokens.name_str("args").map_or(String::new(), ToString::to_string)),
                    "put" => Command::Put,
                    "end" => Command::End,
                    _ => Command::Unknown(command.to_string()),
                },
            )))
        } else if ability_tokens.name("filter_ability").is_some() {
            Ok(Ability::Pattern(ability_tokens.name_parse::<Pattern>("filter").and_then(Result::ok)))
        } else {
            Err(ParseAbilityError::Ability(None))
        }
    }
}

/// Identifies an edge of a [`Range`].
#[derive(Clone, Copy)]
pub(crate) enum Edge {
    /// The start of the [`Range`].
    Start,
    /// The end of the [`Range`].
    End,
}

impl Display for Edge {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Edge::Start => write!(f, "Start"),
            Edge::End => write!(f, "End"),
        }
    }
}

/// Command
#[derive(Clone, Debug)]
pub(crate) enum Command {
    /// See
    See(String),
    /// Put
    Put,
    /// Unknown
    Unknown(String),
    /// End
    End,
}
