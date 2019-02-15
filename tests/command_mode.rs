mod mock;

use paper::Paper;
use mock::MockUserInterface;
use pancurses::Input;
use spectral::prelude::*;
use paper::ui::BACKSPACE;

fn in_command_mode(mut inputs: Vec<Option<Input>>) -> Vec<Option<Input>> {
    let mut full_inputs = vec![Some(Input::Character('.'))];
    full_inputs.append(&mut inputs);
    full_inputs
}

/// Entering characters in Command mode should add text to sketch and display.
///
/// GIVEN the application is in Command mode,
/// WHEN the user enters `abc`,
/// THEN the user interface should display the sketch `"abc"`.
#[test]
fn characters_are_displayed_as_sketch() {
    let mock_ui = MockUserInterface::new(in_command_mode(vec![
        Some(Input::Character('a')),
        Some(Input::Character('b')),
        Some(Input::Character('c'))
    ]));
    let mut paper = Paper::new(&mock_ui);

    paper.run().unwrap();

    assert_that!(mock_ui.apply.calls().last()).is_some().is_equal_to(&mock::display_sketch_edit(String::from("abc")));
}

/// Entering BS in Command mode should remove text from sketch and display.
///
/// GIVEN the application is in Command mode and has the sketch `"abc"`,
/// WHEN the user enters `BS`,
/// THEN the user interface should display the sketch `"ab"`.
#[test]
fn backspace_removes_character_from_sketch() {
    let mock_ui = MockUserInterface::new(in_command_mode(vec![
        Some(Input::Character('a')),
        Some(Input::Character('b')),
        Some(Input::Character('c')),
        Some(Input::Character(BACKSPACE)),
    ]));
    let mut paper = Paper::new(&mock_ui);

    paper.run().unwrap();

    assert_that!(mock_ui.apply.calls().last()).is_some().is_equal_to(&mock::display_sketch_edit(String::from("ab")));
}
