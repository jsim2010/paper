mod mock;

use mock::{Controller, MockExplorer, MockUserInterface};
use pancurses::Input;
use paper::{Explorer, File, Paper};
use spectral::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// `.` in Display mode should enter Command mode.
///
/// GIVEN the application is in Display mode,
/// WHEN the user enters `.`,
/// THEN the user interface should display an empty sketch.
#[test]
fn period_enters_command_mode() {
    let mock_ui = MockUserInterface::new();
    let mut paper = Paper::new(&mock_ui);

    mock_ui
        .receive_input
        .return_value(Some(Input::Character('.')));
    paper.step().unwrap();

    assert!(mock_ui
        .apply
        .has_calls_exactly(vec![mock::display_sketch_edit(String::from(""))]));
}

/// `#` in Display mode should enter Filter mode.
///
/// GIVEN the application is in Display mode,
/// WHEN the user enters `#`,
/// THEN the user interface should display a sketch with `"#"`.
#[test]
fn pound_sign_enters_filter_mode() {
    let mock_ui = MockUserInterface::new();
    let mut paper = Paper::new(&mock_ui);

    mock_ui
        .receive_input
        .return_value(Some(Input::Character('#')));
    paper.step().unwrap();

    assert!(mock_ui
        .apply
        .has_calls_exactly(vec![mock::display_sketch_edit(String::from("#"))]));
}

/// `/` in Display mode should enter Filter mode.
///
/// GIVEN the application is in Display mode,
/// WHEN the user enters `/`,
/// THEN the user interface should display a sketch with `"/"`.
#[test]
fn backslash_enters_filter_mode() {
    let mock_ui = MockUserInterface::new();
    let mut paper = Paper::new(&mock_ui);

    mock_ui
        .receive_input
        .return_value(Some(Input::Character('/')));
    paper.step().unwrap();

    assert!(mock_ui
        .apply
        .has_calls_exactly(vec![mock::display_sketch_edit(String::from("/"))]));
}

/// `j` in Display mode should scroll view down 1/4 of screen.
///
/// GIVEN the application is in Display mode and the screen is 8 lines,
/// WHEN the user enters 'j',
/// THEN the user interface should display lines starting at line 3.
#[test]
fn j_scrolls_down() {
    let mock_ui = MockUserInterface::new();
    let controller = mock::create_controller();
    let file = mock::create_file(&controller);
    controller.borrow_mut().set_file(String::from("a\nb\nc"));
    let mut paper = Paper::with_file(&mock_ui, file);

    mock_ui.grid_height.return_value(Ok(8));
    mock_ui
        .receive_input
        .return_value(Some(Input::Character('j')));
    paper.step().unwrap();

    assert!(mock_ui.apply.has_calls_exactly_in_order(vec![
        mock::display_clear_edit(),
        mock::display_row_edit(0, 2, String::from("3 c")),
    ]));
}

/// `j` in Display mode should not scroll past the last line.
///
/// GIVEN the application is in Display mode, the screen is 8 lines and the file is 2 lines,
/// WHEN the user enters `j`,
/// THEN the user interface should display only line 2.
#[test]
fn j_does_not_scroll_past_last_line() {
    let mock_ui = MockUserInterface::new();
    let controller = mock::create_controller();
    let file = mock::create_file(&controller);
    controller.borrow_mut().set_file(String::from("a\nb"));
    mock_ui.grid_height.return_value(Ok(8));
    let mut paper = Paper::with_file(&mock_ui, file);

    mock_ui
        .receive_input
        .return_value(Some(Input::Character('j')));
    paper.step().unwrap();

    assert!(mock_ui.apply.has_calls_exactly_in_order(vec![
        mock::display_clear_edit(),
        mock::display_row_edit(0, 2, String::from("2 b")),
    ]));
}

/// `j` in Display mode should do nothing if already on the last line.
///
/// GIVEN the application is in Display mode, the screen is 8 lines and the file is 1 line,
/// WHEN the user enters `j`,
/// THEN the user interface should do nothing.
#[test]
fn j_at_end_does_nothing() {
    let mock_ui = MockUserInterface::new();
    let controller = mock::create_controller();
    let file = mock::create_file(&controller);
    controller.borrow_mut().set_file(String::from("a"));
    mock_ui.grid_height.return_value(Ok(8));
    let mut paper = Paper::with_file(&mock_ui, file);

    mock_ui
        .receive_input
        .return_value(Some(Input::Character('j')));
    paper.step().unwrap();

    assert!(mock_ui.apply.num_calls() == 0);
}

/// `k` in Display mode should scroll up 1/4 of screen.
///
/// GIVEN the application is in Display mode, the screen is 8 lines and the first line is line 5,
/// WHEN the user enters 'k',
/// THEN the user interface should display lines 3-8.
#[test]
fn k_scrolls_up() {
    let mock_ui = MockUserInterface::new();
    let controller = mock::create_controller();
    let file = mock::create_file(&controller);
    controller
        .borrow_mut()
        .set_file(String::from("a\nb\nc\nd\ne"));
    mock_ui.grid_height.return_value(Ok(8));
    let mut paper = Paper::with_file(&mock_ui, file);
    let setup_inputs = vec![Some(Input::Character('j')), Some(Input::Character('j'))];

    for input in setup_inputs {
        mock_ui.receive_input.return_value(input);
        paper.step().unwrap();
    }

    mock_ui.apply.reset_calls();
    mock_ui
        .receive_input
        .return_value(Some(Input::Character('k')));
    paper.step().unwrap();

    assert!(mock_ui.apply.has_calls_exactly_in_order(vec![
        mock::display_clear_edit(),
        mock::display_row_edit(0, 2, String::from("3 c")),
        mock::display_row_edit(1, 2, String::from("4 d")),
        mock::display_row_edit(2, 2, String::from("5 e")),
    ]));
}

/// `k` in Display mode should not scroll past first line.
///
/// GIVEN the application is in Display mode, the screen is 8 lines, the file is 2 lines and the
/// first line is line 2,
/// WHEN the user enters 'k',
/// THEN the user interface should display lines 1-2.
#[test]
fn k_does_not_scroll_past_first_line() {
    let mock_ui = MockUserInterface::new();
    let controller = mock::create_controller();
    let file = mock::create_file(&controller);
    controller.borrow_mut().set_file(String::from("a\nb"));
    mock_ui.grid_height.return_value(Ok(8));
    let mut paper = Paper::with_file(&mock_ui, file);
    let setup_inputs = vec![Some(Input::Character('j'))];

    for input in setup_inputs {
        mock_ui.receive_input.return_value(input);
        paper.step().unwrap();
    }

    mock_ui.apply.reset_calls();
    mock_ui
        .receive_input
        .return_value(Some(Input::Character('k')));
    paper.step().unwrap();

    assert!(mock_ui.apply.has_calls_exactly_in_order(vec![
        mock::display_clear_edit(),
        mock::display_row_edit(0, 2, String::from("1 a")),
        mock::display_row_edit(1, 2, String::from("2 b")),
    ]));
}

/// `k` in Display mode should do nothing if already on first line
///
/// GIVEN the application is in Display mode, the screen is 8 lines, the file is 1 line,
/// WHEN the user enters 'k',
/// THEN the user interface should do nothing.
#[test]
fn k_at_first_line_does_nothing() {
    let mock_ui = MockUserInterface::new();
    let controller = mock::create_controller();
    let file = mock::create_file(&controller);
    controller.borrow_mut().set_file(String::from("a"));
    mock_ui.grid_height.return_value(Ok(8));
    let mut paper = Paper::with_file(&mock_ui, file);

    mock_ui
        .receive_input
        .return_value(Some(Input::Character('k')));
    paper.step().unwrap();

    assert_that!(mock_ui.apply.num_calls()).is_equal_to(0);
}
