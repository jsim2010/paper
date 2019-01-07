use clap::{App, crate_version, crate_authors};
use paper::Paper;

fn main() {
    let _matches = App::new("paper")
        .version(crate_version!())
        .author(crate_authors!())
        .get_matches();

    let mut paper = Paper::new();

    paper.run();
}
