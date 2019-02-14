use paper::Paper;
use std::rc::Rc;
use double::{mock_trait_no_default, __private_mock_trait_new_impl, mock_method};
use paper::ui::{Edit, Outcome, UserInterface};
use try_from::TryFromIntError;

mock_trait_no_default!(
    pub MockUserInterface,
    init() -> Outcome,
    close() -> Outcome,
    apply(Edit) -> Outcome,
    flash() -> Outcome,
    grid_height() -> Result<usize, TryFromIntError>,
    receive_input() -> Option<char>);

impl UserInterface for MockUserInterface {
    mock_method!(init(&self) -> Outcome);
    mock_method!(close(&self) -> Outcome);
    mock_method!(apply(&self, _edit: Edit) -> Outcome);
    mock_method!(flash(&self) -> Outcome);
    mock_method!(grid_height(&self) -> Result<usize, TryFromIntError>);
    mock_method!(receive_input(&self) -> Option<char>);
}

#[test]
fn initializes_ui() {
    let mock_ui = MockUserInterface::new(Ok(()), Ok(()), Ok(()), Ok(()), Ok(0), None);
    let paper = Paper::with_ui(Rc::new(mock_ui));
}
