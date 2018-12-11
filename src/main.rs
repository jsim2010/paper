extern crate paper;

#[macro_use]
extern crate clap;

use clap::App;
use std::process;

fn main() {
    let matches = App::new("paper")
        .version(crate_version!())
        .author(crate_authors!())
        .get_matches();

    if let Err(e) = paper::run() {
        eprintln!("Error: {}", e);
        eprintln!("{}", matches.usage());
        
        process::exit(1);
    }
}
