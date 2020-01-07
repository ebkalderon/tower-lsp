# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

* Implement support for the following client-to-server messages:
  * `textDocument/declaration`
  * `textDocument/definition`
  * `textDocument/typeDefinition`

### Changed

* Update `lsp-types` crate from 0.63.1 to 0.68.0.

## [0.5.0] - 2019-12-12

### Added

* Add support for Language Server Protocol 3.15.

### Changed

* Update `lsp-types` crate from 0.61.0 to 0.63.1.

## [0.4.1] - 2019-12-09

### Changed

* Update `jsonrpc-core` crate from 14.0 to 14.0.5.
* Update `jsonrpc-derive` crate from 14.0 to 14.0.5.
* Update `log` crate from 0.4.7 to 0.4.8.
* Update `serde` crate from 1.0.99 to 1.0.103.
* Update `tokio-executor` crate from 0.1.8 to 0.1.9.
* Update `env_logger` crate from 0.7.0 to 0.7.1.

### Fixed

* Correctly handle LSP requests containing incomplete UTF-8 (PR #66).

## [0.4.0] - 2019-10-02

### Added

* Implement support for `textDocument/completion` request.

### Changed

* Expose `Printer` in `LanguageServer::initialize()`.
* Update `env_logger` crate from 0.6.2 to 0.7.0.
* Update `lsp-types` crate from 0.60.0 to 0.61.0.

### Fixed

* Allow `window/logMessage`, `window/showMessage`, and `telemetry/event`
  server-to-client notifications in `initialize` request (PR #48).
* Update links to the LSP specification website to point to the new URL.

## [0.3.1] - 2019-09-08

### Changed

* Use more descriptive message in not initialized JSON-RPC error.
* Initialize example server with available features so it can be used as a
  working mock language server.

### Fixed

* Allow JSON data for `telemetry/event` notification to be null.

## [0.3.0] - 2019-09-05

### Added

* Add support for decoding the optional `Content-Type` field in messages.
* Implement support for the following client-to-server messages:
  * `workspace/didChangeWorkspaceFolders`
  * `workspace/didChangeConfiguration`
  * `workspace/didChangeWatchedFiles`
  * `workspace/symbol`
  * `workspace/executeCommand`
* Implement support for the following server-to-client messages:
  * `telemetry/event`
  * `client/registerCapability`
  * `client/unregisterCapability`
  * `workspace/applyEdit`

### Changed

* Bump minimum Rust version to 1.34.0.
* Rename `highlight()` to `document_highlight()` to better match the
  specification.
* Make all notification methods into provided methods (PR #34).
* Change `LspService` request type from `String` to `Incoming` (PR #28).
* Update `Server` to spawn services with `Incoming` request type.
* Use `env_logger` to print log messages in examples.

### Fixed

* Fix broken doc link to `textDocument/didChange` in `LanguageServer` trait.

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

[init]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-14/#initialize

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

[Unreleased]: https://github.com/ebkalderon/tower-lsp/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/ebkalderon/tower-lsp/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/ebkalderon/tower-lsp/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ebkalderon/tower-lsp/releases/tag/v0.1.0
