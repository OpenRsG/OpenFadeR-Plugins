# Contributing

Thanks for taking the time. Contributions of any size are welcome, including
typo fixes and questions that turn into documentation.

## Before you start

For anything more than a small fix, open an issue first and describe what you
want to change. It saves you from building something that turns out not to fit.

## Setting up

Plugins build against the SDK of the core repository, so check this repository out inside a core checkout:

```bash
git clone https://github.com/OpenRsG/OpenFadeR.git
cd OpenFadeR
git clone https://github.com/OpenRsG/OpenFadeR-Plugins.git extension/plugin
extension/plugin/build.sh
```

## Running the tests

```bash
cargo test --release --manifest-path extension/plugin/Cargo.toml
cargo test --release     # in the core checkout: the host-side tests of the bundled plugins
```

## Making a change

1. Branch off `main`.
2. Keep the change focused: one concern per pull request.
3. Match the style already in the file you are editing.
4. Update the README or docs if you changed behaviour anyone depends on.
5. Write a commit message that explains *why*, not just what.

## Pull requests

Describe what changed and how you verified it. Screenshots help for anything
visual. CI has to pass before review.

## Reporting bugs

Include what you did, what happened, what you expected, and the versions of
OpenFadeR-Plugins, your OS and your runtime. A minimal reproduction is worth more than a
long description.

## Code of conduct

By participating you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
