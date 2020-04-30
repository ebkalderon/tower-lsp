# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.0] - 2020-04-30

### Changed

* Update `lsp-types` crate from 0.73 to 0.74 (PR #178). This introduces breaking
  changes to the following `LanguageServer` trait method signatures:
  * `hover()`
  * `signatureHelp()`
  * `goto_declaration()`
  * `goto_definition()`
  * `goto_type_definition()`
  * `goto_implementation()`
  * `document_highlight()`
* Make `LanguageServer::initialize()` handler `async fn` (PR #182).
* Accept `stdin` and `stdout` handles that are not `Send + 'static`. This
  permits the use of `std::io::Cursor` or `Vec<u8>` as mock stdio sources for
  tests, and passing in `&mut` handles is now supported as well (PR #184).

### Fixed

* Fix broken bidirectional request/response routing (PR #184). The original
  implementation introduced in [0.9.0](#090---2020-03-04) would deadlock under
  certain conditions.

## [0.10.1] - 2020-04-29

### Changed

* Implement `Clone` for `Client` so it can be safely passed to functions
  expecting `'static` values.
* Mark `MessageStream` as `#[must_use]`.

## [0.10.0] - 2020-03-19

### Added

* Implement support for the following client-to-server messages:
  * `textDocument/willSaveWaitUntil`
  * `textDocument/selectionRange`
* Re-export useful `jsonrpc-core` types in a new `jsonrpc` module (PR #169).

### Changed

* Update `lsp-types` crate from 0.70 to 0.73 (PR #162).

### Fixed

* Fix JSON-RPC delegate for `textDocument/foldingRange` (PR #167).

## [0.9.1] - 2020-03-07

### Added

* Implement support for the following client-to-server messages:
  * `textDocument/documentColor`
  * `textDocument/colorPresentation`
  * `textDocument/rangeFormatting`
  * `textDocument/onTypeFormatting`
  * `textDocument/foldingRange`

### Changed

* Server will accept the `initialize` request from the client only once and will
  respond with JSON-RPC error code `-32600` if sent again (PR #160).

### Fixed

* Fix broken links and improve documentation (PRs #152 #157 #158).

## [0.9.0] - 2020-03-04

### Added

* Add `info!()` message when server initializes to be consistent with the
  existing `info!()` message that is emitted when the server exits.
* Implement support for the following client-to-server messages:
  * `textDocument/references`
  * `textDocument/documentLink`
  * `documentLink/resolve`
  * `textDocument/rename`
  * `textDocument/prepareRename`
* Implement support for the following server-to-client messages:
  * `window/showMessageRequest`
  * `workspace/workspaceFolders`
  * `workspace/configuration`

### Changed

* Improve LSP message encoding efficiency (PR #126).
* Reduce chattiness of `trace!()` logs (PR #130).
* Change all notification trait methods to `async fn` (PR #131).
* Implement proper server-to-client request routing (PRs #134 #135).
* Rename `Printer` to `Client`.
* Change `Client::apply_edit()` to return `Result<ApplyWorkspaceEditResponse>`.
* Change `Client::register_capability()` to return `Result<()>`.
* Change `Client::unregister_capability()` to return `Result<()>`.

### Removed

* Remove redundant serialization steps from `Client` (PR #129).

## [0.8.0] - 2020-02-28

### Added

* Implement support for the following client-to-server messages:
  * `textDocument/willSave`
  * `completionItem/resolve`
  * `textDocument/documentSymbol`
  * `textDocument/codeAction`
  * `textDocument/codeLens`
  * `codeLens/resolve`
  * `textDocument/formatting`

### Changed

* `LspService::call()` stops serving requests after `exit` notification,
  meaning there is no longer a need for `ExitReceiver::run_until_exit` and the
  `Server::serve()` async method can now be awaited directly (PR #117).
* Return `Option<String>` as service response type (PR #116).
* Disable unused `nom` features for a hopefully lighter build (PR #112).
* Link to current version of LSP specification in doc comments (PR #122).

### Fixed

* Correctly handle backpressure using `Service::poll_ready()` (PR #117).

### Removed

* Remove `ExitReceiver` type and `LspService::close_handle()` method (PR #117).

## [0.7.0] - 2020-02-24

### Added

* Add default implementations to all non-required `LanguageServer` methods.
* Add support for emitting custom notifications to clients (PR #99).
* Implement support for the following client-to-server messages:
  * `textDocument/signatureHelp`
  * `textDocument/implementation`

### Changed

* Bump minimum supported Rust version to 1.39.0.
* Convert to `std::future` and async/await (PR #101).
* Update `futures` crate from 0.1.28 to 0.3.
* Update `lsp-types` crate from 0.68.0 to 0.70.
* Update `tokio` crate from 0.1.12 to 0.2.
* Update `tower-service` crate from 0.2.0 to 0.3.

### Fixed

* Fix some incorrect links in doc comments.

## [0.6.0] - 2020-01-07

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

[Unreleased]: https://github.com/ebkalderon/tower-lsp/compare/v0.11.0...HEAD
[0.11.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.10.1...v0.11.0
[0.10.1]: https://github.com/ebkalderon/tower-lsp/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.9.1...v0.10.0
[0.9.1]: https://github.com/ebkalderon/tower-lsp/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/ebkalderon/tower-lsp/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/ebkalderon/tower-lsp/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ebkalderon/tower-lsp/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ebkalderon/tower-lsp/releases/tag/v0.1.0
