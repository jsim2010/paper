mod mock;

use mock::Controller;
use pancurses::Input;
use paper::ui::ESC;

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
            mock::display_clear_edit(),
            mock::display_row_edit(0, 2, String::from("1 a")),
        ]
    );
}
