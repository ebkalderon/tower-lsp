# Contributing to tower-lsp

`tower-lsp` is an open-source project that values community contribution. We
could always use a helping hand! What would you like to do?

1. [I want to submit issues or request features.](#submitting-issues)
2. [I want to contribute code.](#contributing-code)
3. [I want to write documentation.](#writing-documentation)

## Submitting issues

One way you can help `tower-lsp` is to report bugs or request features on our
GitHub issue trackers. We can't fix problems we don't know about, so please
report early and often!

When reporting a bug or asking for help, please include enough details so that
the people helping you can reproduce the behavior you are seeing. For some tips
on how to approach this, read about how to produce a
[Minimal, Complete, and Verifiable example][mcve].

[mcve]: https://stackoverflow.com/help/mcve

When making a feature request, please make it clear what problem you intend to
solve with the feature, any ideas for how `tower-lsp` could support solving that
problem, any possible alternatives, and any disadvantages.

## Contributing code

The general workflow for contributing code to the `tower-lsp` repository is as
follows:

1. Fork this repository to your own GitHub account.
2. `git clone` the forked repository to your local machine.
3. Check out the branch you wish to make changes against.
4. Make your changes and push them up to the forked repository.
5. [Open a pull request] against this repository.

[Open a pull request]: https://github.com/ebkalderon/tower-lsp/compare

Before submitting your pull request, please make sure the following conditions
are satisfied:

1. The pull request is based on an up-to-date version of your respective branch.
2. If your pull request introduces new functionality, you have written test
   cases for them.
   * Unit tests are placed at the bottom of the same `.rs` file in a submodule
     called `tests`. See [this example] for reference.
   * Integration tests are placed in a separate `.rs` file in the `tests`
     subdirectory.
3. The codebase has been processed with `cargo fmt`.
4. All of the following commands completed without errors:
   * `cargo build`
   * `cargo test --all`
   * `cargo run --example server`

[this example]: ./src/codec.rs#L129-L157

We encourage you to check that the test suite passes locally before submitting a
pull request with your changes. If anything does not pass, typically it will be
easier to iterate and fix it locally than waiting for the CI servers to run
tests for you.

Thank you very much for your contribution! Now `tower-lsp` will be a little
faster, more ergonomic, and more efficient.

## Writing documentation

Documentation improvements are always welcome!

As with most Rust projects, API documentation is generated directly from doc
comments, denoted by either `///` or `//!`, using a tool called Rustdoc. See
[the official Rust book's chapter on Rustdoc][rd] for more information on how
this works.

[rd]: https://doc.rust-lang.org/book/ch14-02-publishing-to-crates-io.html#making-useful-documentation-comments

Documentation of any kind should adhere to the following standard:

1. Lines must not extend beyond 80 characters in length.
2. To enhance readability in text editors and terminals, use only *reference
   style* Markdown links, as shown in the example below. However, if the link
   points to an anchor that exists on the same page, the *inline style* should
   be used instead.

```markdown
Here is some [example text] with a link in it. While we are at it, here is yet
yet [another link][al], but shorter. If we are linking to [an anchor](#anchor)
on the same page, we can do this inline.

[example text]: https://some.url/
[al]: https://another.url/
```

When submitting your pull requests, please follow the same workflow described in
the [Contributing Code](#contributing-code) section above.

## Code of conduct

Please note that this project is released with a [Contributor Code of Conduct].
By participating in this project, you agree to abide by its terms.

[Contributor Code of Conduct]: ./CODE_OF_CONDUCT.md
