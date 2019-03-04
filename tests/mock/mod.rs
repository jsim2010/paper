use double::mock_method;
use pancurses::Input;
use paper::num::Length;
use paper::{File, ui};
use paper::ui::{Address, Change, Edit, Index, Region, UserInterface};
use paper::{Explorer, Outcome};
use std::cell::RefCell;
use std::rc::Rc;
use try_from::TryFromIntError;

pub fn create_controller() -> Rc<RefCell<Controller>> {
    Rc::new(RefCell::new(Controller::default()))
}

pub fn create_file(controller: &Rc<RefCell<Controller>>) -> File {
    File::new(Rc::new(RefCell::new(MockExplorer::new(Rc::clone(controller)))), String::from("mock"))
}

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
    pub fn new() -> Self {
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
    mock_method!(init(&self) -> ui::Outcome);
    mock_method!(close(&self) -> ui::Outcome);
    mock_method!(apply(&self, _edit: Edit) -> ui::Outcome);
    mock_method!(flash(&self) -> ui::Outcome);
    mock_method!(grid_height(&self) -> Result<usize, TryFromIntError>);
    mock_method!(receive_input(&self) -> Option<Input>);
}

#[derive(Debug, Default)]
pub struct Controller {
    file: String,
}

impl Controller {
    pub fn set_file(&mut self, file: String) {
        self.file = file;
    }

    pub fn file(&self) -> &String {
        &self.file
    }
}

#[derive(Debug, Clone)]
pub struct MockExplorer {
    controller: Rc<RefCell<Controller>>,
}

impl MockExplorer {
    pub fn new(controller: Rc<RefCell<Controller>>) -> Self {
        Self { controller }
    }
}

impl Explorer for MockExplorer {
    fn start(&mut self) {
    }

    fn read(&self, path: &str) -> Outcome<String> {
        Ok(self.controller.borrow().file().to_string())
    }

    fn write(&self, path: &str, data: &str) -> Outcome<()> {
        Ok(())
    }
}

pub fn display_sketch_edit(sketch: String) -> Edit {
    display_row_edit(0, 0, sketch)
}

pub fn display_row_edit(row: u16, column: u16, line: String) -> Edit {
    Edit::new(
        Region::new(
            Address::new(Index::from(row), Index::from(column)),
            Length::End,
        ),
        Change::Row(line),
    )
}

pub fn display_clear_edit() -> Edit {
    Edit::new(
        Region::new(
            Address::new(Index::from(0), Index::from(0)),
            Length::Value(Index::from(0)),
        ),
        Change::Clear,
    )
}
