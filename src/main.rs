extern crate paper;

#[macro_use]
extern crate clap;

use clap::App;
use paper::Paper;

fn main() {
    let _matches = App::new("paper")
        .version(crate_version!())
        .author(crate_authors!())
        .get_matches();

    let mut paper = Paper::new();

    paper.run();
}
