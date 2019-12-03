// `app_from_crate` requires using all the macros that it calls.
use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg};
use std::process;
use paper::{Failure, Paper, Settings};

fn main() {
    // Forces compiler to rebuild when Cargo.toml file is changed, needed for app_from_crate.
    let _ = include_str!("../Cargo.toml");
    let args = app_from_crate!().arg(Arg::with_name("file").help("the file to be viewed"));

    if let Err(error) = run(Settings::from(args.get_matches())) {
        eprintln!("ERROR: {}", error);

        process::exit(1);
    }
}

fn run(settings: Settings) -> Result<(), Failure> {
    let mut app = Paper::new()?;

    app.init()?;
    app.configure(settings)?;
    app.run()?;
    Ok(())
}
