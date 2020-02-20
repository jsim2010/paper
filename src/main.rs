use {
    // `app_from_crate` requires using all the macros that it calls.
    clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg},
    core::convert::TryInto,
    paper::{Failure, Paper},
};

fn main() -> Result<(), Failure> {
    // Forces compiler to rebuild when Cargo.toml file is changed, needed for app_from_crate.
    let _ = include_str!("../Cargo.toml");

    Paper::new(
        app_from_crate!()
            .arg(Arg::with_name("file").help("the file to be viewed"))
            .get_matches()
            .try_into()?,
    )?
    .run()?;
    Ok(())
}
