use clap::{crate_authors, crate_version, App};
use paper::{ui::Terminal, LocalExplorer, Paper, Flag};

fn main() {
    let _matches = App::new("paper")
        .version(crate_version!())
        .author(crate_authors!())
        .get_matches();
    let error = match LocalExplorer::current_dir_url() {
        Ok(current_dir) => Paper::new(Terminal::new(), LocalExplorer::new(current_dir)).run().err(),
        Err(e) => Some(Flag::from(e)),
    };

    if let Some(e) = error {
        eprintln!("{}", e);
    }
}
