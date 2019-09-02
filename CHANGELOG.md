# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2019-09-03

### Added

* Add `ExitedError` for when calling `LspService` after it has already exited.

### Changed

* Language server now returns server error code `-32002` if any method is called
  before `initialize` request is received, [as per the spec][init].
* `LspService` sets `Service::Error` to `ExitedError`.
* `Server` can now accept any service where `Service::Error` is convertible to
  `Box<dyn Error + Send + Sync>`. This enables compatibility with most Tower
  middleware.
* Retain error or success from future in `ExitReceiver::run_until_exit()`.
* Remove `'static` bounds on some `Server` and `ExitReceiver` methods.

[init]: https://microsoft.github.io/language-server-protocol/specification#initialize

## [0.1.0] - 2019-09-02

### Added

* Initial crate release.
* Implement support for the following message types:
  * `initialize`
  * `initialized`
  * `shutdown`
  * `exit`
  * `window/showMessage`
  * `window/logMessage`
  * `textDocument/publishDiagnostics`
  * `textDocument/didOpen`
  * `textDocument/didChange`
  * `textDocument/didSave`
  * `textDocument/didClose`
  * `textDocument/hover`
  * `textDocument/documentHighlight`

[Unreleased]: https://github.com/ebkalderon/tower-lsp/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ebkalderon/tower-lsp/releases/tag/v0.1.0
