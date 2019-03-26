//! Defines helpful numerical functionality.

use crate::{fmt, Add, AddAssign, Borrow, Display, Formatter, Ordering, TryFrom, TryFromIntError};

/// The internal value that represents the number of characters until the end of the row.
///
/// Value is specified by [`pancurses`].
const END: i32 = -1;

/// Signifies an `i32` value that is not negative.
///
/// Useful for cases where an interface requires a signed number but the number should not be
/// negative.
///
/// # Guarantees
///
/// Given: `value: NonNegativeI32`
///
/// `i32::from(value) >= 0`
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NonNegativeI32(i32);

impl NonNegativeI32 {
    /// Divides `NonNegativeI32` by given value, returning None if self is 0.
    #[inline]
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        self.0.checked_div(rhs.0).map(Self)
    }

    /// Adds 1.
    #[inline]
    pub fn add_one(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Subtracts 1.
    #[inline]
    pub fn sub_one(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }
}

impl<T> Add<T> for NonNegativeI32
where
    T: Borrow<i32>,
{
    type Output = Self;

    #[inline]
    fn add(self, other: T) -> Self::Output {
        Self(self.0 + other.borrow())
    }
}

impl<T> AddAssign<T> for NonNegativeI32
where
    T: Borrow<i32>,
{
    #[inline]
    fn add_assign(&mut self, other: T) {
        self.0 += other.borrow();
    }
}

impl Borrow<i32> for NonNegativeI32 {
    #[inline]
    fn borrow(&self) -> &i32 {
        &self.0
    }
}

impl Display for NonNegativeI32 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u8> for NonNegativeI32 {
    #[inline]
    fn from(value: u8) -> Self {
        Self(i32::from(value))
    }
}

impl From<u16> for NonNegativeI32 {
    #[inline]
    fn from(value: u16) -> Self {
        Self(i32::from(value))
    }
}

impl From<NonNegativeI32> for u64 {
    #[allow(clippy::result_unwrap_used)] // converting NonNegativeI32 to u64 will never fail
    #[inline]
    fn from(value: NonNegativeI32) -> Self {
        Self::try_from(value.0).unwrap()
    }
}

impl PartialEq<i32> for NonNegativeI32 {
    #[inline]
    fn eq(&self, other: &i32) -> bool {
        self.0.eq(other)
    }
}

impl PartialOrd<i32> for NonNegativeI32 {
    #[inline]
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        Some(self.0.cmp(other))
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

impl TryFrom<i64> for NonNegativeI32 {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: i64) -> Result<Self, Self::Err> {
        i32::try_from(value).map(Self)
    }
}

impl TryFrom<Length> for NonNegativeI32 {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: Length) -> Result<Self, Self::Err> {
        if let Length::Value(length) = value {
            Ok(length)
        } else {
            Err(TryFromIntError::Underflow)
        }
    }
}

impl TryFrom<usize> for NonNegativeI32 {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: usize) -> Result<Self, Self::Err> {
        i32::try_from(value).map(Self)
    }
}

impl TryFrom<u64> for NonNegativeI32 {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: u64) -> Result<Self, Self::Err> {
        i32::try_from(value).map(Self)
    }
}

impl From<NonNegativeI32> for i32 {
    #[inline]
    fn from(value: NonNegativeI32) -> Self {
        value.0
    }
}

impl TryFrom<NonNegativeI32> for usize {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: NonNegativeI32) -> Result<Self, Self::Err> {
        Self::try_from(value.0)
    }
}

/// Signifies the number of elements in a list.
#[derive(Clone, Copy, Eq, Debug, Hash, Ord, PartialEq, PartialOrd)]
pub enum Length {
    /// The value that covers all indexes.
    Value(NonNegativeI32),
    /// The value needed to cover all elements from the current one to the end.
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

impl TryFrom<Length> for u64 {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: Length) -> Result<Self, Self::Err> {
        match value {
            Length::Value(x) => Self::try_from(x.0),
            Length::End => Err(TryFromIntError::Underflow),
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

impl From<u16> for Length {
    #[inline]
    fn from(value: u16) -> Self {
        Length::Value(NonNegativeI32::from(value))
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

impl TryFrom<usize> for Length {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: usize) -> Result<Self, Self::Err> {
        NonNegativeI32::try_from(value).map(Length::Value)
    }
}
