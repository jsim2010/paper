//! Creates and runs the application using the arguments provided from the command.
use {
    // `app_from_crate` requires importing all the macros that it calls.
    // https://github.com/clap-rs/clap/issues/1478 states that fix has been added to be released in 3.0.0.
    clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg},
    paper::{Failure, Paper},
};

fn main() -> Result<(), Failure> {
    // Forces compiler to rebuild when Cargo.toml file is changed, needed for app_from_crate.
    let _ = include_str!("../Cargo.toml");

    Paper::new(
        &(&app_from_crate!()
            .arg(Arg::with_name("file").help("the file to be viewed"))
            .get_matches())
            .into(),
    )?
    .run()?;
    Ok(())
}
