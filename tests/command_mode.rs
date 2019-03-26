mod mock;

use mock::Controller;
use pancurses::Input;
use paper::ui::{Index, BACKSPACE, ESC};

/// Entering characters in Command mode should add text to sketch and display.
///
/// GIVEN the application is in Command mode,
/// WHEN the user enters `abc`,
/// THEN the user interface should display the sketch `"abc"`.
#[test]
fn characters_are_displayed_as_sketch() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create(
        &controller,
        vec![
            Input::Character('.'),
            Input::Character('a'),
            Input::Character('b'),
        ],
    );
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('c')));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![mock::display_row_edit(7, String::from("abc"))]
    );
}

/// Entering BS in Command mode should remove text from sketch and display.
///
/// GIVEN the application is in Command mode and has the sketch `"abc"`,
/// WHEN the user enters `BS`,
/// THEN the user interface should display the sketch `"ab"`.
#[test]
fn backspace_removes_character_from_sketch() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create(
        &controller,
        vec![
            Input::Character('.'),
            Input::Character('a'),
            Input::Character('b'),
            Input::Character('c'),
        ],
    );
    controller
        .borrow_mut()
        .set_input(Some(Input::Character(BACKSPACE)));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![mock::display_row_edit(7, String::from("ab"))]
    );
}

/// Entering ESC in Command mode should return to Display mode.
///
/// GIVEN the application is in Command mode,
/// WHEN the user enters `ESC`,
/// THEN the user interface should display the current file.
#[test]
fn escape_returns_to_display_mode() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(5));
    let mut paper = mock::create_with_file(&controller, vec![Input::Character('.')], "a");
    controller
        .borrow_mut()
        .set_input(Some(Input::Character(ESC)));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![
            mock::display_clear_edit(),
            mock::display_row_edit(0, String::from("1 a")),
        ]
    );
}
