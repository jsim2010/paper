mod mock;

use mock::{MockExplorer, MockUserInterface};
use pancurses::Input;
use paper::ui::ESC;
use paper::{File, Paper};

#[test]
fn escape_returns_to_display() {
    let mock_ui = MockUserInterface::new();
    let controller = mock::create_controller();
    let file = mock::create_file(&controller);
    controller.borrow_mut().set_file(String::from("a"));
    mock_ui.grid_height.return_value(Ok(5));
    let mut paper = Paper::with_file(&mock_ui, file);
    let setup_inputs = vec![Some(Input::Character('#'))];

    for input in setup_inputs {
        mock_ui.receive_input.return_value(input);
        paper.step().unwrap();
    }

    mock_ui.apply.reset_calls();
    mock_ui
        .receive_input
        .return_value(Some(Input::Character(ESC)));
    paper.step().unwrap();

    assert!(mock_ui.apply.has_calls_exactly_in_order(vec![
        mock::display_clear_edit(),
        mock::display_row_edit(0, 2, String::from("1 a")),
    ]));
}
