use double::{mock_method};
use paper::ui::{Edit, Outcome, UserInterface};
use try_from::TryFromIntError;
use pancurses::Input;

#[derive(Debug, Clone)]
pub struct MockUserInterface {
    pub init: double::Mock<(), Outcome>,
    pub close: double::Mock<(), Outcome>,
    pub apply: double::Mock<(Edit), Outcome>,
    pub flash: double::Mock<(), Outcome>,
    pub grid_height: double::Mock<(), Result<usize, TryFromIntError>>,
    pub receive_input: double::Mock<(), Option<Input>>,
}

impl Default for MockUserInterface {
    fn default() -> Self {
        Self {
            init: double::Mock::new(Ok(())),
            close: double::Mock::new(Ok(())),
            apply: double::Mock::new(Ok(())),
            flash: double::Mock::new(Ok(())),
            grid_height: double::Mock::new(Ok(0)),
            receive_input: double::Mock::default(),
        }
    }
}

impl UserInterface for MockUserInterface {
    mock_method!(init(&self) -> Outcome);
    mock_method!(close(&self) -> Outcome);
    mock_method!(apply(&self, _edit: Edit) -> Outcome);
    mock_method!(flash(&self) -> Outcome);
    mock_method!(grid_height(&self) -> Result<usize, TryFromIntError>);
    mock_method!(receive_input(&self) -> Option<Input>);
}
