mod mock;

use mock::Controller;
use pancurses::Input;
use paper::ui::{Color, ESC};

#[test]
fn escape_returns_to_display() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(5));
    let mut paper = mock::create_with_file(&controller, vec![Input::Character('#')], "a");
    controller
        .borrow_mut()
        .set_input(Some(Input::Character(ESC)));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![
            mock::clear_change(),
            mock::row_change(0, String::from("1 a")),
        ]
    );
}

#[test]
fn filter_single_digit_line_number() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(5));
    let mut paper = mock::create_with_file(&controller, vec![Input::Character('#')], "a\nb\nc");
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('2')));

    paper.step().unwrap();

    let calls = controller.borrow().apply_calls().clone();
    let changes = vec![
        mock::format_line_change(1, 2, Color::Red),
        mock::format_line_change(0, 2, Color::Blue),
        mock::format_line_change(2, 2, Color::Blue),
    ];

    for change in changes {
        assert!(calls.contains(&change), "\n{} not in  {:#?}", change, calls);
    }
}

#[test]
fn filter_double_digit_line_number() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(11));
    let mut paper = mock::create_with_file(
        &controller,
        vec![Input::Character('#'), Input::Character('1')],
        "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk",
    );
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('0')));

    paper.step().unwrap();

    let calls = controller.borrow().apply_calls().clone();
    let changes = vec![
        mock::format_line_change(0, 3, Color::Blue),
        mock::format_line_change(1, 3, Color::Blue),
        mock::format_line_change(2, 3, Color::Blue),
        mock::format_line_change(3, 3, Color::Blue),
        mock::format_line_change(4, 3, Color::Blue),
        mock::format_line_change(5, 3, Color::Blue),
        mock::format_line_change(6, 3, Color::Blue),
        mock::format_line_change(7, 3, Color::Blue),
        mock::format_line_change(8, 3, Color::Blue),
        mock::format_line_change(9, 3, Color::Red),
        mock::format_line_change(10, 3, Color::Blue),
    ];

    for change in changes {
        assert!(calls.contains(&change), "\n{} not in  {:#?}", change, calls);
    }
}

/// When the line number filter ends with the range operator ('.'), it should be the same as if the
/// range operator was not there.
#[test]
fn filter_line_number_range_operator() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(5));
    let mut paper = mock::create_with_file(
        &controller,
        vec![Input::Character('#'), Input::Character('2')],
        "a\nb\nc\nd\ne",
    );
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('.')));

    paper.step().unwrap();

    assert_eq!(
        controller.borrow().apply_calls(),
        &vec![
            mock::row_change(4, "#2.".into()),
            mock::format_line_change(0, 0, Color::Default),
            mock::format_line_change(1, 0, Color::Default),
            mock::format_line_change(2, 0, Color::Default),
            mock::format_line_change(3, 0, Color::Default),
            mock::format_line_change(4, 0, Color::Default),
            mock::format_line_change(0, 2, Color::Blue),
            mock::format_line_change(1, 2, Color::Blue),
            mock::format_line_change(2, 2, Color::Blue),
            mock::format_line_change(3, 2, Color::Blue),
            mock::format_line_change(4, 2, Color::Blue),
            mock::format_line_change(1, 2, Color::Red),
        ]
    );
}

#[test]
fn filter_line_number_range() {
    let controller = Controller::new();
    controller.borrow_mut().set_grid_height(Ok(5));
    let mut paper = mock::create_with_file(
        &controller,
        vec![
            Input::Character('#'),
            Input::Character('2'),
            Input::Character('.'),
        ],
        "a\nb\nc\nd\ne",
    );
    controller
        .borrow_mut()
        .set_input(Some(Input::Character('4')));

    paper.step().unwrap();

    let calls = controller.borrow().apply_calls().clone();
    let changes = vec![
        mock::format_line_change(0, 2, Color::Blue),
        mock::format_line_change(1, 2, Color::Red),
        mock::format_line_change(2, 2, Color::Red),
        mock::format_line_change(3, 2, Color::Red),
        mock::format_line_change(4, 2, Color::Blue),
    ];

    for change in changes {
        assert!(calls.contains(&change), "\n{} not in {:#?}", change, calls);
    }
}
