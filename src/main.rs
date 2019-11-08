// `app_from_crate` requires using all the macros that it calls.
use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version};
use paper::Paper;

fn main() {
    let _ = app_from_crate!().get_matches();

    if let Err(error) = Paper::new().and_then(|mut paper| paper.run()) {
        eprintln!("{}", error);
    }
}
