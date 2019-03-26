//! Defines helpful numerical functionality.
use std::fmt::{self, Display, Formatter};
use std::ops::Add;
use try_from::{TryFrom, TryFromIntError};

/// The internal value that represents a `Length::End`.
///
/// Value is specified by `pancurses`.
const END: i32 = -1;

/// Signifies an `i32` value that is not negative.
///
/// Useful for cases where an interface requires an i32 but the number cannot be negative.
///
/// # Guarantees
///
/// Given: `value: NonNegativeI32`
///
/// `i32::from(value) >= 0`
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NonNegativeI32(i32);

impl NonNegativeI32 {
    /// Create a `NonNegativeI32` without checking the value.
    ///
    /// # Safety
    ///
    /// `value <= i32::max_value()`
    // Follows the precedent set by `NonZeroUsize`.
    pub unsafe fn new_unchecked(value: u32) -> Self {
        Self(
            i32::try_from(value)
                .unwrap_or_else(|_| panic!("Creating `NonNegativeI32 from {}.", value)),
        )
    }

    /// Creates a new `NonNegativeI32` with a value of zero.
    pub fn zero() -> Self {
        Self(0)
    }

    /// Computes `self / rhs`, returning `None` if `rhs == 0` or the division results in overflow.
    #[inline]
    // Follows the format set by checked_div() from primitive types.
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        self.0.checked_div(rhs.0).map(Self)
    }

    /// Adds one, returning the result.
    #[inline]
    // Follows the nightly `Step` trait.
    pub fn add_one(self) -> Self {
        Self(self.0.add(1))
    }

    /// Subtracts one, returning the result.
    #[inline]
    // Follows the nightly `Step` trait.
    pub fn sub_one(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }
}

impl Display for NonNegativeI32 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<i32> for NonNegativeI32 {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: i32) -> Result<Self, Self::Err> {
        if value.is_negative() {
            Err(TryFromIntError::Underflow)
        } else {
            Ok(Self(value))
        }
    }
}

impl TryFrom<u64> for NonNegativeI32 {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: u64) -> Result<Self, Self::Err> {
        // Converted value will be >= 0.
        i32::try_from(value).map(Self)
    }
}

impl From<NonNegativeI32> for i32 {
    #[inline]
    fn from(value: NonNegativeI32) -> Self {
        value.0
    }
}

impl From<NonNegativeI32> for u64 {
    #[inline]
    fn from(value: NonNegativeI32) -> Self {
        Self::try_from(value.0).expect("Converting `NonNegativeI32` to u64.")
    }
}

impl TryFrom<NonNegativeI32> for usize {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: NonNegativeI32) -> Result<Self, Self::Err> {
        Self::try_from(value.0)
    }
}

/// Signifies a number of elements in a list.
#[derive(Clone, Copy, Eq, Debug, Hash, Ord, PartialEq, PartialOrd)]
pub enum Length {
    /// The number of elements.
    Value(NonNegativeI32),
    /// Represents the value equal to all elements from the current one to the end.
    End,
}

impl Length {
    /// Returns if `Length` is equal to 0.
    #[inline]
    pub fn is_zero(self) -> bool {
        match self {
            Length::Value(NonNegativeI32(0)) => true,
            _ => false,
        }
    }
}

impl Default for Length {
    #[inline]
    fn default() -> Self {
        Length::Value(NonNegativeI32::default())
    }
}

impl Display for Length {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Length::End => write!(f, "END"),
            length => write!(f, "{}", length),
        }
    }
}

impl From<u64> for Length {
    #[inline]
    fn from(value: u64) -> Self {
        match NonNegativeI32::try_from(value) {
            Ok(length) => Length::Value(length),
            Err(_) => Length::End,
        }
    }
}

impl From<Length> for i32 {
    #[inline]
    fn from(value: Length) -> Self {
        match value {
            Length::Value(x) => Self::from(x),
            Length::End => END,
        }
    }
}
