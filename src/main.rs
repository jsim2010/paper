use clap::{crate_authors, crate_version, App};
use paper::ui::Terminal;
use paper::LocalExplorer;
use paper::Paper;

fn main() {
    let _matches = App::new("paper")
        .version(crate_version!())
        .author(crate_authors!())
        .get_matches();
    let mut paper = Paper::new(Terminal::new(), LocalExplorer::new());

    if let Err(s) = paper.run() {
        eprintln!("{}", s);
    }
}
