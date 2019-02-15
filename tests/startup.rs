mod mock;

use paper::Paper;
use mock::MockUserInterface;

/// Startup should initiailize the user interface.
///
/// WHEN the application starts running,
/// THEN the user interface should initialize.
#[test]
fn initializes_ui() {
    let mock_ui = MockUserInterface::new(Vec::with_capacity(1));
    let mut paper = Paper::new(&mock_ui);

    paper.run().unwrap();

    assert!(mock_ui.init.called());
}
