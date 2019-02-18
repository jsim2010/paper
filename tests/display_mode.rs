mod mock;

use mock::MockUserInterface;
use pancurses::Input;
use paper::Paper;
use paper::ui::ENTER;
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

    assert_that!(mock_ui.apply.calls())
        .equals_iterator(&iter::once(&mock::display_sketch_edit(String::from(""))));
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

    assert_that!(mock_ui.apply.calls())
        .equals_iterator(&iter::once(&mock::display_sketch_edit(String::from("#"))));
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

    assert_that!(mock_ui.apply.calls())
        .equals_iterator(&iter::once(&mock::display_sketch_edit(String::from("/"))));
}

/// 'j' in Display mode should scroll view down 1/4 of file.
///
/// GIVEN the application is in Display mode and viewing a file with 20 lines,
/// WHEN the user enters 'j',
/// THEN the user interface should display lines 6-20.
#[test]
fn j_scrolls_down() {
    let mock_ui = MockUserInterface::new(vec![
        Some(Input::Character('s')),
        Some(Input::Character('e')),
        Some(Input::Character('e')),
        Some(Input::Character(' ')),
        Some(Input::Character('M')),
        Some(Input::Character('O')),
        Some(Input::Character('C')),
        Some(Input::Character('K')),
        Some(Input::Character(':')),
        Some(Input::Character('/')),
        Some(Input::Character('/')),
        Some(Input::Character('t')),
        Some(Input::Character('e')),
        Some(Input::Character('s')),
        Some(Input::Character('t')),
        Some(Input::Character(ENTER)),
        Some(Input::Character('j')),
    ]);
    let mut paper = Paper::new(&mock_ui);

    paper.run().unwrap();
}
