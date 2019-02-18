use double::mock_method;
use pancurses::Input;
use paper::num::Length;
use paper::ui::{Address, Change, Edit, Index, Region, UserInterface};
use paper::ui;
use paper::{Explorer, Outcome};
use try_from::TryFromIntError;

#[derive(Debug, Clone)]
pub struct MockUserInterface {
    pub init: double::Mock<(), ui::Outcome>,
    pub close: double::Mock<(), ui::Outcome>,
    pub apply: double::Mock<(Edit), ui::Outcome>,
    pub flash: double::Mock<(), ui::Outcome>,
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
    mock_method!(init(&self) -> ui::Outcome);
    mock_method!(close(&self) -> ui::Outcome);
    mock_method!(apply(&self, _edit: Edit) -> ui::Outcome);
    mock_method!(flash(&self) -> ui::Outcome);
    mock_method!(grid_height(&self) -> Result<usize, TryFromIntError>);
    mock_method!(receive_input(&self) -> Option<Input>);
}

#[derive(Debug, Clone)]
pub struct MockExplorer {
    pub read: double::Mock<(String), Outcome<String>>,
    pub write: double::Mock<(String, String), Outcome<()>>,
}

impl MockExplorer {
    pub fn new() -> Self {
        Self {
            read: double::Mock::new(Ok(String::new())),
            write: double::Mock::new(Ok(())),
        }
    }
}

impl Explorer for MockExplorer {
    mock_method!(read(&self, path: &String) -> Outcome<String>);
    mock_method!(write(&self, path: &String, data: &String) -> Outcome<()>);
}

pub fn display_sketch_edit(sketch: String) -> Edit {
    display_row_edit(0, 0, sketch)
}

pub fn display_row_edit(row: u16, column: u16, line: String) -> Edit {
    Edit::new(Region::new(Address::new(Index::from(row), Index::from(column)), Length::End), Change::Row(line))
}

pub fn display_clear_edit() -> Edit {
    Edit::new(Region::new(Address::new(Index::from(0), Index::from(0)), Length::Value(Index::from(0))), Change::Clear)
}
