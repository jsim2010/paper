//! Creates and runs the application using the arguments provided from the command.
use {
    fehler::throws,
    paper::{Arguments, Failure, Paper},
    structopt::StructOpt,
};

#[throws(Failure)]
fn main() {
    Paper::new(Arguments::from_args())?.run()?;
}
