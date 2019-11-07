// `app_from_crate` requires using all the macros that it calls.
use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version};
use paper::{ui::Terminal, Explorer, Paper};

fn main() {
    let _ = app_from_crate!().get_matches();

    if let Err(error) =
        Explorer::new().and_then(|local_explorer| Paper::new(Terminal::new(), local_explorer).run())
    {
        eprintln!("{}", error);
    }
}
