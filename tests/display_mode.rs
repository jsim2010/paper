mod mock;

use mock::{MockExplorer, MockUserInterface};
use pancurses::Input;
use paper::{File, Paper};
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

/// `j` in Display mode should scroll view down 1/4 of screen.
///
/// GIVEN the application is in Display mode and the screen is 8 lines,
/// WHEN the user enters 'j',
/// THEN the user interface should display lines starting at line 3.
#[test]
fn j_scrolls_down() {
    let mock_ui = MockUserInterface::new(vec![Some(Input::Character('j'))]);
    let mock_explorer = MockExplorer::new();
    let file = File::new(&mock_explorer, String::from("mock"));
    mock_explorer
        .read
        .return_value(Ok(String::from("a\nb\nc\nd\ne\nf\ng\nh")));

    let mut paper = Paper::with_file(&mock_ui, file);

    mock_ui.grid_height.return_value(Ok(8));
    paper.run().unwrap();

    assert_that!(mock_ui.apply.calls()[0]).is_equal_to(&mock::display_clear_edit());
    assert_that!(mock_ui.apply.calls()[1]).is_equal_to(&mock::display_row_edit(
        0,
        2,
        String::from("3 c"),
    ));
    assert_that!(mock_ui.apply.calls().len()).is_equal_to(7);
}

/// `j` in Display mode should not scroll past the last line.
///
/// GIVEN the application is in Display mode, the screen is 8 lines, the file is 4 lines and the
/// first line is line 3,
/// WHEN the user enters `j`,
/// THEN the user interface should display only line 4.
#[test]
fn j_does_not_scroll_past_last_line() {
    let mock_ui = MockUserInterface::new(vec![
        // First j moves line 3 to the top of the screen.
        Some(Input::Character('j')),
        Some(Input::Character('j')),
    ]);
    let mock_explorer = MockExplorer::new();
    let file = File::new(&mock_explorer, String::from("mock"));
    mock_explorer
        .read
        .return_value(Ok(String::from("a\nb\nc\nd")));
    mock_ui.grid_height.return_value(Ok(8));
    let mut paper = Paper::with_file(&mock_ui, file);

    paper.run().unwrap();

    // Skip all the edits from the first j.
    let mut edits = mock_ui.apply.calls().into_iter();
    let last_edit = mock::display_row_edit(1, 2, String::from("4 d"));

    loop {
        match edits.next() {
            Some(edit) => {
                if edit == last_edit {
                    break;
                }
            }
            None => panic!("Unable to find the last edit from setup"),
        }
    }

    assert_that!(edits.next())
        .is_some()
        .is_equal_to(&mock::display_clear_edit());
    assert_that!(edits.next())
        .is_some()
        .is_equal_to(&mock::display_row_edit(0, 2, String::from("4 d")));
    // That is the end of the edits.
    assert_that!(edits.next()).is_none();
}

/// `j` in Display mode should do nothing if already on the last line.
///
/// GIVEN the application is in Display mode, the screen is 8 lines, the file is 3 lines and the
/// first line is line 3,
/// WHEN the user enters `j`,
/// THEN the user interface should do nothing.
#[test]
fn j_at_end_does_nothing() {
    let mock_ui = MockUserInterface::new(vec![
        // First j moves line 3 to top of the screen.
        Some(Input::Character('j')),
        Some(Input::Character('j')),
    ]);
    let mock_explorer = MockExplorer::new();
    let file = File::new(&mock_explorer, String::from("mock"));
    mock_explorer.read.return_value(Ok(String::from("a\nb\nc")));
    mock_ui.grid_height.return_value(Ok(8));
    let mut paper = Paper::with_file(&mock_ui, file);

    paper.run().unwrap();

    // Skip all the edits from the first j.
    let mut edits = mock_ui.apply.calls().into_iter();
    let last_edit = mock::display_row_edit(0, 2, String::from("3 c"));

    loop {
        match edits.next() {
            Some(edit) => {
                if edit == last_edit {
                    break;
                }
            }
            None => panic!(
                "Unable to find the last edit from setup in {:?}",
                mock_ui.apply.calls()
            ),
        }
    }

    assert_that!(edits.next()).is_none();
}
