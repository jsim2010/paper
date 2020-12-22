//! Describes orientations of a direction.

/// A direction along an axis.
#[derive(Clone, Copy, Debug)]
pub(crate) enum AxialDirection {
    /// Towards the primary direction of the axis.
    Positive,
    /// Opposite the primary direction of the axis.
    Negative,
}

/// An axis along a plane.
#[derive(Clone, Copy, Debug)]
enum PlaneAxis {
    /// The first axis.
    First,
    /// The second axis.
    Second,
}

/// Describes a direction on a planar surface.
#[derive(Clone, Copy, Debug)]
struct PlanarDirection {
    /// Describes the axis on which the direction lies.
    axis: PlaneAxis,
    /// Describes the direction along `axis`.
    direction: AxialDirection,
}

/// Describes a direction on a planar surface that is facing the user.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ScreenDirection {
    /// Towards the top of the screen.
    Up,
    /// Towards the bottom of the screen.
    Down,
    /// Towards the left side of the screen.
    Left,
    /// Towards the right side of the screen.
    Right,
}

impl ScreenDirection {
    /// Returns the direction of `self` along the vertical axis.
    #[inline]
    #[must_use]
    pub(crate) const fn vertical_direction(self) -> Option<AxialDirection> {
        match self {
            Self::Up => Some(AxialDirection::Negative),
            Self::Down => Some(AxialDirection::Positive),
            Self::Left | Self::Right => None,
        }
    }
}

impl From<PlanarDirection> for ScreenDirection {
    #[inline]
    fn from(planar_direction: PlanarDirection) -> Self {
        match planar_direction.axis {
            PlaneAxis::First => match planar_direction.direction {
                AxialDirection::Positive => Self::Down,
                AxialDirection::Negative => Self::Up,
            },
            PlaneAxis::Second => match planar_direction.direction {
                AxialDirection::Positive => Self::Right,
                AxialDirection::Negative => Self::Left,
            },
        }
    }
}

impl From<ScreenDirection> for PlanarDirection {
    #[inline]
    fn from(screen_direction: ScreenDirection) -> Self {
        match screen_direction {
            ScreenDirection::Up => Self {
                axis: PlaneAxis::First,
                direction: AxialDirection::Positive,
            },
            ScreenDirection::Down => Self {
                axis: PlaneAxis::First,
                direction: AxialDirection::Negative,
            },
            ScreenDirection::Left => Self {
                axis: PlaneAxis::Second,
                direction: AxialDirection::Negative,
            },
            ScreenDirection::Right => Self {
                axis: PlaneAxis::Second,
                direction: AxialDirection::Positive,
            },
        }
    }
}
