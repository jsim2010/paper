use clap::{crate_authors, crate_version, App};
use paper::ui::Terminal;
use paper::Paper;
use std::rc::Rc;

fn main() {
    let _matches = App::new("paper")
        .version(crate_version!())
        .author(crate_authors!())
        .get_matches();
    let ui = Rc::new(Terminal::new());
    let mut paper = Paper::new(ui);

    if let Err(s) = paper.run() {
        eprintln!("{}", s);
    }
}
