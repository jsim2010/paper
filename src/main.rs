//! Creates and runs the application using the arguments provided from the command.
use {
    fehler::throws,
    paper::{Arguments, Failure, Paper},
    // Implements from_args().
    structopt::StructOpt,
};

fn main() {
    if let Err(error) = exec() {
        eprint!("{}", error);
    }
}

/// Executes the application functionality.
#[throws(Failure)]
fn exec() {
    Paper::new(Arguments::from_args())?.run()?
}
