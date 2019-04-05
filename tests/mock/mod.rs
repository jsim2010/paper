use pancurses::Input;
use paper::file;
use paper::lsp::ProgressParams;
use paper::mode::Operation;
use paper::ui::{self, Address, Change, Index, Span};
use paper::{Explorer, Paper, UserInterface};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
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
    fn init(&self) -> ui::Effect {
        Ok(())
    }

    fn close(&self) -> ui::Effect {
        Ok(())
    }

    fn apply(&self, change: Change) -> ui::Effect {
        self.controller.borrow_mut().add_apply_call(change);
        Ok(())
    }

    fn flash(&self) -> ui::Effect {
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
    apply_calls: Vec<Change>,
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

    pub fn add_apply_call(&mut self, change: Change) {
        self.apply_calls.push(change);
    }

    pub fn reset_apply_calls(&mut self) {
        self.apply_calls.clear();
    }

    pub fn apply_calls(&self) -> &Vec<Change> {
        &self.apply_calls
    }

    pub fn set_grid_height(&mut self, grid_height: Result<u32, TryFromIntError>) {
        self.grid_height.0 = match grid_height {
            Ok(height) => Ok(unsafe { Index::new_unchecked(height) }),
            Err(error) => Err(error),
        };
    }

    pub fn grid_height(&self) -> &Result<Index, TryFromIntError> {
        &self.grid_height.0
    }
}

#[derive(Debug)]
struct GridHeight(Result<Index, TryFromIntError>);

impl Default for GridHeight {
    fn default() -> Self {
        Self(Ok(Index::zero()))
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
    fn start(&mut self) -> file::Effect<()> {
        Ok(())
    }

    fn read(&mut self, _path: &PathBuf) -> file::Effect<String> {
        Ok(self.controller.borrow().file().to_string())
    }

    fn write(&self, _path: &Path, _data: &str) -> file::Effect<()> {
        Ok(())
    }

    fn receive_notification(&mut self) -> Option<ProgressParams> {
        None
    }
}

pub fn row_change(row: u32, line: String) -> Change {
    let row_index = unsafe { Index::new_unchecked(row) };
    Change::Text(
        Span::new(
            Address::new(row_index, Index::zero()),
            Address::new(row_index, Index::max_value()),
        ),
        line,
    )
}

pub fn clear_change() -> Change {
    Change::Clear
}
