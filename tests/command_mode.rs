mod mock;

use mock::MockUserInterface;
use pancurses::Input;
use paper::ui::BACKSPACE;
use paper::Paper;
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
