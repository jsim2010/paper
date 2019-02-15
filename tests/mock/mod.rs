use double::mock_method;
use pancurses::Input;
use paper::num::Length;
use paper::ui::{Address, Change, Edit, Index, Outcome, Region, UserInterface};
use try_from::TryFromIntError;

#[derive(Debug, Clone)]
pub struct MockUserInterface {
    pub init: double::Mock<(), Outcome>,
    pub close: double::Mock<(), Outcome>,
    pub apply: double::Mock<(Edit), Outcome>,
    pub flash: double::Mock<(), Outcome>,
    pub grid_height: double::Mock<(), Result<usize, TryFromIntError>>,
    pub receive_input: double::Mock<(), Option<Input>>,
}

impl MockUserInterface {
    /// Creates a new `MockUserInterface`.
    ///
    /// Adds `KeyClose` to end of inputs so that test stops running.
    pub fn new(mut inputs: Vec<Option<Input>>) -> Self {
        let mock_ui = Self {
            init: double::Mock::new(Ok(())),
            close: double::Mock::new(Ok(())),
            apply: double::Mock::new(Ok(())),
            flash: double::Mock::new(Ok(())),
            grid_height: double::Mock::new(Ok(0)),
            receive_input: double::Mock::default(),
        };
        inputs.push(Some(Input::KeyClose));
        mock_ui.receive_input.return_values(inputs);
        mock_ui
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

pub fn display_sketch_edit(sketch: String) -> Edit {
    Edit::new(
        Region::new(Address::new(Index::from(0), Index::from(0)), Length::End),
        Change::Row(sketch),
    )
}
