//! Defines smart pointer functionality.
use std::cell::RefCell;
use std::rc::Rc;

/// A Mutable Reference Counter.
///
/// This is just a `Rc` of a `RefCell`.
pub(crate) type Mrc<T> = Rc<RefCell<T>>;

/// Creates a new `Mrc`.
#[macro_export]
macro_rules! mrc {
    ($item:expr) => {
        std::rc::Rc::new(std::cell::RefCell::new($item))
    };
}
