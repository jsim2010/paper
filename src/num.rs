//! Defines helpful numerical functionality.
use core::convert::TryFrom;
use std::{
    fmt::{self, Display, Formatter},
    num::TryFromIntError,
    ops::{Add, Sub},
};

/// Signifies an `i32` value that is not negative.
///
/// Useful for cases where an interface requires an i32 but the number should not be negative.
///
/// # Guarantees
///
/// Given: `value: NonNegI32`
///
/// `i32::from(value) >= 0`
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
// Follows precedent of [`NonZeroI32`].
pub struct NonNegI32(i32);

impl NonNegI32 {
    /// Create a `NonNegI32` without checking the value.
    ///
    /// # Safety
    ///
    /// `value >= 0`
    pub unsafe fn new_unchecked(value: i32) -> Self {
        Self(value)
    }

    /// Creates a new `NonNegI32` with a value of zero.
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Returns the maximum value of `NonNegI32`.
    pub const fn max_value() -> Self {
        Self(i32::max_value())
    }

    /// Converts `value` to a `NonNegI32`, saturating at `i32::max_value()`.
    pub fn saturating_from_u64(value: u64) -> Self {
        match Self::try_from(value) {
            Ok(index) => index,
            // Error can only occur when value is larger than i32::max_value().
            Err(_) => Self(i32::max_value()),
        }
    }

    /// Computes `self / rhs`, returning `None` if `rhs == 0` or the division results in overflow.
    // Follows precedent of [`i32::checked_div`].
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        self.0.checked_div(rhs.0).map(Self)
    }

    /// Adds one, returning the result.
    // Follows precedent of nightly [`Step::add_one`].
    pub fn add_one(self) -> Self {
        Self(self.0.add(1))
    }

    /// Subtracts one, returning the result.
    // Follows precedent of nightly [`Step::sub_one`].
    pub fn sub_one(self) -> Self {
        Self(self.0.sub(1))
    }
}

impl Display for NonNegI32 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<i32> for NonNegI32 {
    type Error = TryFromIntError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        u32::try_from(value).map(|_| Self(value))
    }
}

impl TryFrom<u64> for NonNegI32 {
    type Error = TryFromIntError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        i32::try_from(value).map(Self)
    }
}

impl TryFrom<usize> for NonNegI32 {
    type Error = TryFromIntError;

    #[inline]
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        // Converted value will be >= 0.
        i32::try_from(value).map(Self)
    }
}

impl From<NonNegI32> for i32 {
    #[inline]
    fn from(value: NonNegI32) -> Self {
        value.0
    }
}

impl From<NonNegI32> for u64 {
    #[inline]
    fn from(value: NonNegI32) -> Self {
        Self::try_from(value.0).expect("converting `NonNegI32` to u64")
    }
}

impl TryFrom<NonNegI32> for usize {
    type Error = TryFromIntError;

    #[inline]
    fn try_from(value: NonNegI32) -> Result<Self, Self::Error> {
        Self::try_from(value.0)
    }
}
