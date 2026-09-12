# Agent contract

This product follows https://github.com/thanhson251189/engineering-practices

- One idea per change.
- Tests for new behavior land in the same change (`src/` `#[cfg(test)]`).
- Do not claim tests passed unless you ran them in this session.
- English source of truth for the practices repo; this file only points there.

Product is the Rust crate. Run:

    cargo test
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
