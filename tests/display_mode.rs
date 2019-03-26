mod mock;

use mock::Controller;
use pancurses::Input;
use paper::ui::Index;

/// `.` in Display mode should enter Command mode.
///
/// GIVEN the application is in Display mode,
/// WHEN the user enters `.`,
/// THEN the user interface should display an empty sketch.
#[test]
fn period_enters_command_mode() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create(&controller, vec![]);
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('.')));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![mock::display_row_edit(7, String::from(""))]
    );
}

/// `#` in Display mode should enter Filter mode.
///
/// GIVEN the application is in Display mode,
/// WHEN the user enters `#`,
/// THEN the user interface should display a sketch with `"#"`.
#[test]
fn pound_sign_enters_filter_mode() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create(&controller, vec![]);
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('#')));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![mock::display_row_edit(7, String::from("#"))]
    );
}

/// `/` in Display mode should enter Filter mode.
///
/// GIVEN the application is in Display mode,
/// WHEN the user enters `/`,
/// THEN the user interface should display a sketch with `"/"`.
#[test]
fn backslash_enters_filter_mode() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create(&controller, vec![]);
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('/')));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![mock::display_row_edit(7, String::from("/"))]
    );
}

/// `j` in Display mode should scroll view down 1/4 of screen.
///
/// GIVEN the application is in Display mode and the screen is 8 lines,
/// WHEN the user enters 'j',
/// THEN the user interface should display lines starting at line 3.
#[test]
fn j_scrolls_down() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create_with_file(&controller, vec![], "a\nb\nc");
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('j')));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![
            mock::display_clear_edit(),
            mock::display_row_edit(0, String::from("3 c")),
        ]
    );
}

/// `j` in Display mode should not scroll past the last line.
///
/// GIVEN the application is in Display mode, the screen is 8 lines and the file is 2 lines,
/// WHEN the user enters `j`,
/// THEN the user interface should display only line 2.
#[test]
fn j_does_not_scroll_past_last_line() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create_with_file(&controller, vec![], "a\nb");
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('j')));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![
            mock::display_clear_edit(),
            mock::display_row_edit(0, String::from("2 b")),
        ]
    );
}

/// `j` in Display mode should do nothing if already on the last line.
///
/// GIVEN the application is in Display mode, the screen is 8 lines and the file is 1 line,
/// WHEN the user enters `j`,
/// THEN the user interface should do nothing.
#[test]
fn j_at_end_does_nothing() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create_with_file(&controller, vec![], "a");
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('j')));

    paper.step().unwrap();

    assert_eq!(controller.borrow().apply_calls(), &vec![]);
}

/// `k` in Display mode should scroll up 1/4 of screen.
///
/// GIVEN the application is in Display mode, the screen is 8 lines and the first line is line 5,
/// WHEN the user enters 'k',
/// THEN the user interface should display lines 3-8.
#[test]
fn k_scrolls_up() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create_with_file(
        &controller,
        vec![Input::Character('j'), Input::Character('j')],
        "a\nb\nc\nd\ne",
    );
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('k')));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![
            mock::display_clear_edit(),
            mock::display_row_edit(0, String::from("3 c")),
            mock::display_row_edit(1, String::from("4 d")),
            mock::display_row_edit(2, String::from("5 e")),
        ]
    );
}

/// `k` in Display mode should not scroll past first line.
///
/// GIVEN the application is in Display mode, the screen is 8 lines, the file is 2 lines and the
/// first line is line 2,
/// WHEN the user enters 'k',
/// THEN the user interface should display lines 1-2.
#[test]
fn k_does_not_scroll_past_first_line() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create_with_file(&controller, vec![Input::Character('j')], "a\nb");
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('k')));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![
            mock::display_clear_edit(),
            mock::display_row_edit(0, String::from("1 a")),
            mock::display_row_edit(1, String::from("2 b")),
        ]
    );
}

/// `k` in Display mode should do nothing if already on first line
///
/// GIVEN the application is in Display mode, the screen is 8 lines, the file is 1 line,
/// WHEN the user enters 'k',
/// THEN the user interface should do nothing.
#[test]
fn k_at_first_line_does_nothing() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(8));
    let mut paper = mock::create_with_file(&controller, vec![], "a");
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('k')));

    paper.step().unwrap();

    assert_eq!(controller.borrow().apply_calls(), &vec![]);
}
