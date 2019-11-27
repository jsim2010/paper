// `app_from_crate` requires using all the macros that it calls.
use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg};
use paper::Paper;

fn main() {
    // Forces compiler to rebuild when Cargo.toml file is changed.
    let _ = include_str!("../Cargo.toml");

    let args = app_from_crate!()
        .arg(Arg::with_name("file").help("the file to be viewed"))
        .get_matches();

    if let Err(error) = Paper::default().run(&args) {
        eprintln!("{}", error);
    }
}
