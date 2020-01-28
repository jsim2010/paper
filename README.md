# paper

A terminal-based text editor with goals to maximize simplicity and efficiency.

This project is very much in an alpha state.

## Development

Clone the repository and enter the directory:

```sh
git clone https://github.com/jsim2010/paper.git
cd paper
```

This project uses [`just`](https://github.com/casey/just) for running project-specific commands. If `just` is not already installed on your system, install it:

```sh
cargo install just
```

To see all available recipes for development, run:

```sh
just --list
```

Note that `just v` is run by the **validate** status check required for merging pull requests.

License: MIT
