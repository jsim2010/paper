//! Creates and runs the application using the arguments provided from the command.
use {
    // `app_from_crate` requires importing all the macros that it calls.
    // https://github.com/clap-rs/clap/issues/1478 states that fix has been added to be released in 3.0.0.
    clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg},
    fehler::throws,
    paper::{Failure, Paper},
};

#[throws(Failure)]
fn main() {
    // Forces compiler to rebuild when Cargo.toml file is changed, needed for app_from_crate.
    let _ = include_str!("../Cargo.toml");

    Paper::new(
        &(&app_from_crate!()
            .arg(
                Arg::with_name("log")
                    .long("log")
                    .value_name("COMPONENT")
                    .possible_values(&["starship"])
                    .help("Enables logs for components"),
            )
            .arg(
                Arg::with_name("file")
                    .value_name("FILE")
                    .help("The file to be viewed"),
            )
            .arg(
                Arg::with_name("verbose")
                    .short("v")
                    .multiple(true)
                    .help("Increases the logging verbosity - can be repeated upto 3 times"),
            )
            .get_matches())
            .into(),
    )?
    .run()?;
}
