mod mock;

use mock::{MockExplorer, MockUserInterface};
use pancurses::Input;
use paper::ui::{BACKSPACE, ESC};
use paper::{File, Paper};
use spectral::prelude::*;

/// Entering characters in Command mode should add text to sketch and display.
///
/// GIVEN the application is in Command mode,
/// WHEN the user enters `abc`,
/// THEN the user interface should display the sketch `"abc"`.
#[test]
fn characters_are_displayed_as_sketch() {
    let mock_ui = MockUserInterface::new();
    let mut paper = Paper::new(&mock_ui);
    let setup_inputs = vec![
        Some(Input::Character('.')),
        Some(Input::Character('a')),
        Some(Input::Character('b')),
    ];

    for input in setup_inputs {
        mock_ui.receive_input.return_value(input);
        paper.step().unwrap();
    }

    mock_ui.apply.reset_calls();
    mock_ui
        .receive_input
        .return_value(Some(Input::Character('c')));
    paper.step().unwrap();

    assert!(mock_ui
        .apply
        .has_calls_exactly(vec![mock::display_sketch_edit(String::from("abc"))]));
}

/// Entering BS in Command mode should remove text from sketch and display.
///
/// GIVEN the application is in Command mode and has the sketch `"abc"`,
/// WHEN the user enters `BS`,
/// THEN the user interface should display the sketch `"ab"`.
#[test]
fn backspace_removes_character_from_sketch() {
    let mock_ui = MockUserInterface::new();
    let mut paper = Paper::new(&mock_ui);
    let setup_inputs = vec![
        Some(Input::Character('.')),
        Some(Input::Character('a')),
        Some(Input::Character('b')),
        Some(Input::Character('c')),
    ];

    for input in setup_inputs {
        mock_ui.receive_input.return_value(input);
        paper.step().unwrap();
    }

    mock_ui.apply.reset_calls();
    mock_ui
        .receive_input
        .return_value(Some(Input::Character(BACKSPACE)));
    paper.step().unwrap();

    assert_that!(mock_ui
        .apply
        .has_calls_exactly(vec![mock::display_sketch_edit(String::from("ab"))]));
}

/// Entering ESC in Command mode should return to Display mode.
///
/// GIVEN the application is in Command mode,
/// WHEN the user enters `ESC`,
/// THEN the user interface should display the current file.
#[test]
fn escape_returns_to_display_mode() {
    let mock_ui = MockUserInterface::new();
    let mock_explorer = MockExplorer::new();
    let file = File::new(&mock_explorer, String::from("mock"));
    mock_explorer.read.return_value(Ok(String::from("a")));
    mock_ui.grid_height.return_value(Ok(5));
    let mut paper = Paper::with_file(&mock_ui, file);
    let setup_inputs = vec![Some(Input::Character('.'))];

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
