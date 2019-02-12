# paper

A terminal-based editor with goals to maximize simplicity and efficiency.

This project is very much in an alpha state.

Its features include:
- Modal editing (keys implement different functionality depending on the current mode).
- Extensive but relatively simple filter grammar that allows user to select any text.

Future items on the Roadmap:
- Add more filter grammar.
- Implement suggestions for commands to improve user experience.
- Support Language Server Protocol.

## Development

Clone the repository and enter the directory:

```sh
git clone https://github.com/jsim2010/paper.git
cd paper
```

If `cargo-make` is not already installed on your system, install it:

```sh
cargo install --force cargo-make
```

Install all dependencies needed for development:

```sh
cargo make dev
```

Now you can run the following commands:
- Evaluate all checks, lints and tests: `cargo make eval`
- Fix stale README and formatting: `cargo make fix`

License: MIT
