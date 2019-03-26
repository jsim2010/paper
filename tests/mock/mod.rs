use pancurses::Input;
use paper::mode::{Operation, Output};
use paper::storage::ProgressParams;
use paper::ui::{Address, Change, Edit, Index, UserInterface};
use paper::Explorer;
use paper::{ui, Paper};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use try_from::TryFromIntError;

pub fn create(controller: &Rc<RefCell<Controller>>, setup: Vec<Input>) -> Paper {
    let mut paper = Paper::new(
        MockUserInterface::new(&controller),
        MockExplorer::new(controller),
    );

    for input in setup {
        controller.borrow_mut().set_input(Some(input));
        paper.step().unwrap();
    }

    controller.borrow_mut().reset_apply_calls();
    paper
}

pub fn create_with_file(
    controller: &Rc<RefCell<Controller>>,
    setup: Vec<Input>,
    file: &str,
) -> Paper {
    controller.borrow_mut().set_file(String::from(file));
    let mut paper = Paper::new(
        MockUserInterface::new(controller),
        MockExplorer::new(controller),
    );

    // Sets the data in the view based on the file stored by controller.
    paper.operate(&Operation::display_file("mock")).unwrap();

    for input in setup {
        controller.borrow_mut().set_input(Some(input));
        paper.step().unwrap();
    }

    controller.borrow_mut().reset_apply_calls();
    paper
}

#[derive(Debug, Clone)]
pub struct MockUserInterface {
    controller: Rc<RefCell<Controller>>,
}

impl MockUserInterface {
    /// Creates a new `MockUserInterface`.
    pub fn new(controller: &Rc<RefCell<Controller>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            controller: Rc::clone(controller),
        }))
    }
}

impl UserInterface for MockUserInterface {
    fn init(&self) -> ui::Outcome {
        Ok(())
    }

    fn close(&self) -> ui::Outcome {
        Ok(())
    }

    fn apply(&self, edit: Edit) -> ui::Outcome {
        self.controller.borrow_mut().add_apply_call(edit);
        Ok(())
    }

    fn flash(&self) -> ui::Outcome {
        Ok(())
    }

    fn grid_height(&self) -> Result<Index, TryFromIntError> {
        *self.controller.borrow().grid_height()
    }

    fn receive_input(&self) -> Option<Input> {
        *self.controller.borrow().input()
    }
}

#[derive(Debug, Default)]
pub struct Controller {
    file: String,
    input: Option<Input>,
    apply_calls: Vec<Edit>,
    grid_height: GridHeight,
}

impl Controller {
    pub fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self::default()))
    }

    pub fn set_file(&mut self, file: String) {
        self.file = file;
    }

    pub fn file(&self) -> &String {
        &self.file
    }

    pub fn set_input(&mut self, input: Option<Input>) {
        self.input = input;
    }

    pub fn input(&self) -> &Option<Input> {
        &self.input
    }

    pub fn add_apply_call(&mut self, edit: Edit) {
        self.apply_calls.push(edit);
    }

    pub fn reset_apply_calls(&mut self) {
        self.apply_calls.clear();
    }

    pub fn apply_calls(&self) -> &Vec<Edit> {
        &self.apply_calls
    }

    pub fn set_grid_height(&mut self, grid_height: Result<Index, TryFromIntError>) {
        self.grid_height.0 = grid_height;
    }

    pub fn grid_height(&self) -> &Result<Index, TryFromIntError> {
        &self.grid_height.0
    }
}

#[derive(Debug)]
struct GridHeight(Result<Index, TryFromIntError>);

impl Default for GridHeight {
    fn default() -> Self {
        Self(Ok(Index::from(0_u8)))
    }
}

#[derive(Debug, Clone)]
pub struct MockExplorer {
    controller: Rc<RefCell<Controller>>,
}

impl MockExplorer {
    pub fn new(controller: &Rc<RefCell<Controller>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            controller: Rc::clone(controller),
        }))
    }
}

impl Explorer for MockExplorer {
    fn start(&mut self) -> Output<()> {
        Ok(())
    }

    fn read(&mut self, _path: &Path) -> Output<String> {
        Ok(self.controller.borrow().file().to_string())
    }

    fn write(&self, _path: &Path, _data: &str) -> Output<()> {
        Ok(())
    }

    fn receive_notification(&mut self) -> Option<ProgressParams> {
        None
    }
}

pub fn display_row_edit(row: u16, line: String) -> Edit {
    Edit::new(
        Some(Address::new(Index::from(row), Index::from(0_u8))),
        Change::Row(line),
    )
}

pub fn display_clear_edit() -> Edit {
    Edit::new(None, Change::Clear)
}
