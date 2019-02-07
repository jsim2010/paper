//! Defines the base types of [`ui`].

use crate::{fmt, Add, AddAssign, Borrow, Display, Formatter, Ordering, TryFrom, TryFromIntError};
use std::ops::Neg;

/// The type of the value stored in [`Index`].
///
/// This type is specified by [`pancurses`].
pub(crate) type IndexType = i32;

/// The [`Length`] that represents the number of characters until the end of the row.
pub(crate) const END: Length = Length(END_VALUE);

/// The internal value that represents the number of characters until the end of the row.
///
/// Value is specified by [`pancurses`].
const END_VALUE: IndexType = -1;

/// Signifies the index of a row or column in the grid.
///
/// An `Index` must be `>= 0` and `<= i32::max_value()`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Index(IndexType);

impl<T> Add<T> for Index
where
    T: Borrow<IndexType> + Display,
{
    type Output = Self;

    fn add(self, other: T) -> Self::Output {
        Index(
            self.0
                .checked_add(*other.borrow())
                .unwrap_or_else(|| panic!("{} + {} wrapped", self, other)),
        )
    }
}

impl<T> AddAssign<T> for Index
where
    T: Borrow<IndexType>,
{
    fn add_assign(&mut self, other: T) {
        self.0 += other.borrow();
    }
}

impl Borrow<IndexType> for Index {
    fn borrow(&self) -> &IndexType {
        &self.0
    }
}

impl Display for Index {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u8> for Index {
    fn from(value: u8) -> Self {
        Index(IndexType::from(value))
    }
}

impl Neg for Index {
    type Output = IndexType;

    #[allow(clippy::integer_arithmetic)] // self.0 >= 0
    fn neg(self) -> Self::Output {
        -self.0
    }
}

impl PartialEq<IndexType> for Index {
    fn eq(&self, other: &IndexType) -> bool {
        self.0.eq(other)
    }
}

impl PartialOrd<IndexType> for Index {
    fn partial_cmp(&self, other: &IndexType) -> Option<Ordering> {
        Some(self.0.cmp(other))
    }
}

impl TryFrom<i64> for Index {
    type Err = TryFromIntError;

    fn try_from(value: i64) -> Result<Self, Self::Err> {
        IndexType::try_from(value).map(Index)
    }
}

impl TryFrom<IndexType> for Index {
    type Err = TryFromIntError;

    fn try_from(value: IndexType) -> Result<Self, Self::Err> {
        if value.is_negative() {
            Err(TryFromIntError::Underflow)
        } else {
            Ok(Index(value))
        }
    }
}

impl TryFrom<Length> for Index {
    type Err = LengthEndError;

    #[inline]
    fn try_from(value: Length) -> Result<Self, Self::Err> {
        Self::try_from(value.0).map_err(|_| LengthEndError)
    }
}

impl TryFrom<u32> for Index {
    type Err = TryFromIntError;

    fn try_from(value: u32) -> Result<Self, Self::Err> {
        IndexType::try_from(value).map(Index)
    }
}

impl TryFrom<usize> for Index {
    type Err = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Err> {
        IndexType::try_from(value).map(Index)
    }
}

impl From<Index> for IndexType {
    #[inline]
    fn from(value: Index) -> Self {
        value.0
    }
}

impl TryFrom<Index> for usize {
    type Err = TryFromIntError;

    #[inline]
    fn try_from(value: Index) -> Result<Self, Self::Err> {
        Self::try_from(value.0)
    }
}

/// Signifies a number of adjacent [`Address`]es.
///
/// Generally this is an unsigned number. However, there is a special `Length` called [`END`] that
/// represents the number of [`Address`]es between a start [`Address`] and the end of that row.
///
/// [`Address`]: struct.Address.html
/// [`END`]: constant.END.html
#[derive(Clone, Copy, Eq, Debug, Default, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Length(IndexType);

impl Length {
    /// Returns the value of the `Length`.
    pub(crate) fn n(self) -> IndexType {
        self.0
    }
}

impl Display for Length {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.0 {
            END_VALUE => write!(f, "END"),
            length => write!(f, "{}", length),
        }
    }
}

impl From<u16> for Length {
    fn from(value: u16) -> Self {
        Length(IndexType::from(value))
    }
}

impl TryFrom<usize> for Length {
    type Err = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Err> {
        IndexType::try_from(value).map(Length)
    }
}

/// Signifies an [`Error`] that occurs when trying to convert [`END`] to an [`Index`].
#[derive(Clone, Copy, Debug)]
pub struct LengthEndError;

impl Display for LengthEndError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Length is end")
    }
}
