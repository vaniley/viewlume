# Contributing to Viewlume

Thanks for helping improve Viewlume.

## Development setup

1. Install the stable Rust toolchain.
2. Fork and clone the repository.
3. Create a focused branch from `main`.
4. Make the change and add tests where practical.
5. Run the required checks:

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

6. Open a pull request describing the problem, the solution, and how it was verified.

Keep pull requests small and avoid unrelated formatting or refactoring. UI changes should include a screenshot or short recording when possible.

By contributing, you agree that your contribution is licensed under the project's MIT License.
