//! Tests the functionality of closing the application.
mod mock;

use paper::Paper;
use mock::MockUserInterface;

/// A `Close` input should close the user interface.
///
/// WHEN the user sends a `Close` input,
/// THEN the user interface should close.
///
/// This functionality is primarily important for testing. Tests need to be able to use a mock
/// `UserInterface` to end the application when ending a test.
#[test]
fn close_input_closes_ui() {
    // Close input is added to the end of the inputs.
    let mock_ui = MockUserInterface::new(Vec::with_capacity(1));
    let mut paper = Paper::new(&mock_ui);

    paper.run().unwrap();

    assert!(mock_ui.close.called());
}
