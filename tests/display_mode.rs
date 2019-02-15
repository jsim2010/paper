mod mock;

use paper::Paper;
use mock::MockUserInterface;
use pancurses::Input;
use spectral::prelude::*;
use std::iter;

/// `.` in Display mode should enter Command mode.
///
/// GIVEN the application is in Display mode,
/// WHEN the user enters `.`,
/// THEN the user interface should display an empty sketch.
#[test]
fn period_enters_command_mode() {
    let mock_ui = MockUserInterface::new(vec![Some(Input::Character('.'))]);
    let mut paper = Paper::new(&mock_ui);

    paper.run().unwrap();

    assert_that!(mock_ui.apply.calls()).equals_iterator(&iter::once(&mock::display_sketch_edit(String::from(""))));
}

/// `#` in Display mode should enter Filter mode.
///
/// GIVEN the application is in Display mode,
/// WHEN the user enters `#`,
/// THEN the user interface should display a sketch with `"#"`.
#[test]
fn pound_sign_enters_filter_mode() {
    let mock_ui = MockUserInterface::new(vec![Some(Input::Character('#'))]);
    let mut paper = Paper::new(&mock_ui);

    paper.run().unwrap();

    assert_that!(mock_ui.apply.calls()).equals_iterator(&iter::once(&mock::display_sketch_edit(String::from("#"))));
}

/// `/` in Display mode should enter Filter mode.
///
/// GIVEN the application is in Display mode,
/// WHEN the user enters `/`,
/// THEN the user interface should display a sketch with `"/"`.
#[test]
fn backslash_enters_filter_mode() {
    let mock_ui = MockUserInterface::new(vec![Some(Input::Character('/'))]);
    let mut paper = Paper::new(&mock_ui);

    paper.run().unwrap();

    assert_that!(mock_ui.apply.calls()).equals_iterator(&iter::once(&mock::display_sketch_edit(String::from("/"))));
}
