//! Defines helpful numerical functionality.
use core::{
    convert::TryFrom,
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

    /// Creates a `NonNegI32` with the smallest value that can be represented by `NonNegI32`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use paper::num::NonNegI32;
    ///
    /// assert_eq!(i32::from(NonNegI32::min_value()), 0)
    /// ```
    pub const fn min_value() -> Self {
        Self(0)
    }

    /// Creates a `NonNegI32` with the largest value that can be represented by `NonNegI32`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use paper::num::NonNegI32;
    ///
    /// assert_eq!(i32::from(NonNegI32::max_value()), 2147483647);
    /// ```
    pub const fn max_value() -> Self {
        Self(i32::max_value())
    }

    /// Converts `value` to a `NonNegI32`, saturating at `i32::max_value()`.
    ///
    /// # Examples
    ///
    /// Converting number less than `i32::max_value()`.
    /// ```
    /// # use paper::num::NonNegI32;
    ///
    /// assert_eq!(i32::from(NonNegI32::saturating_from_u64(1)), 1);
    /// ```
    ///
    /// Converting number greater than `i32::max_value()`.
    /// ```
    /// # use paper::num::NonNegI32;
    ///
    /// assert_eq!(i32::from(NonNegI32::saturating_from_u64(u64::max_value())),
    /// i32::max_value());
    /// ```
    pub fn saturating_from_u64(value: u64) -> Self {
        match Self::try_from(value) {
            Ok(index) => index,
            // Error occurs when value is larger than i32::max_value().
            Err(_) => Self::max_value(),
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
        // Try to convert to u32, thus we return TryFromIntError if value is negative.
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

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        i32::try_from(value).map(Self)
    }
}

impl From<NonNegI32> for i32 {
    fn from(value: NonNegI32) -> Self {
        value.0
    }
}

impl From<NonNegI32> for u64 {
    fn from(value: NonNegI32) -> Self {
        // It is guaranteed value.0 >= 0 so u64::from(value.0) should never fail.
        Self::try_from(value.0).expect("converting `NonNegI32` to u64")
    }
}

impl TryFrom<NonNegI32> for usize {
    type Error = TryFromIntError;

    fn try_from(value: NonNegI32) -> Result<Self, Self::Error> {
        Self::try_from(value.0)
    }
}
