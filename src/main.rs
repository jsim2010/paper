use {
    // `app_from_crate` requires using all the macros that it calls.
    clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg},
    paper::{Paper, Settings},
    std::process,
};

fn main() {
    // Forces compiler to rebuild when Cargo.toml file is changed, needed for app_from_crate.
    let _ = include_str!("../Cargo.toml");
    let app = app_from_crate!().arg(Arg::with_name("file").help("the file to be viewed"));

    if let Err(error) = Paper::new().run(Settings::from(app.get_matches())) {
        eprintln!("ERROR: {}", error);

        process::exit(1);
    }
}
