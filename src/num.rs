//! Defines helpful numerical functionality.
use std::{
    fmt::{self, Display, Formatter},
    ops::{Add, Sub},
};
use try_from::{TryFrom, TryFromIntError};

/// Signifies an `i32` value that is not negative.
///
/// Useful for cases where an interface requires an i32 but the number should not be negative.
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

    /// Returns the maximum value of `NonNegativeI32`.
    pub fn max_value() -> Self {
        Self(i32::max_value())
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
        Self(self.0.sub(1))
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

impl TryFrom<usize> for NonNegativeI32 {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: usize) -> Result<Self, Self::Err> {
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
