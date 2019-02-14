mod mock;

use paper::Paper;
use pancurses::Input;
use mock::MockUserInterface;

#[test]
fn close_input_closes_ui() {
    let mock_ui = MockUserInterface::default();
    mock_ui.receive_input.return_value(Some(Input::KeyClose));
    let mut paper = Paper::new(&mock_ui);

    paper.run().unwrap();

    assert!(mock_ui.close.called());
}
