# Supported Features

This document tracks which features defined in the [Language Server Protocol
(LSP) specification][spec] are currently supported by `tower-lsp` and to what
degree. Please note: the matrices below are kept updated on a best-effort basis
and may be out of date on mainline at any given time.

[spec]: https://microsoft.github.io/language-server-protocol/specification

Feel free to open a new [discussion thread] if there are any questions. Drive-by
pull requests correcting this document are always welcome!

[discussion thread]: https://github.com/ebkalderon/tower-lsp/discussions

<details><summary>Click here to expand/collapse the icon legend.</summary>

#### Message Type

Symbol                      | Description                    | Location in API
----------------------------|--------------------------------|-----------------------
:leftwards_arrow_with_hook: | Request, client to server      | `LanguageServer` trait
:arrow_right_hook:          | Request, server to client      | `Client` struct
:arrow_right:               | Notification, client to server | `LanguageServer` trait
:arrow_left:                | Notification, server to client | `Client` struct

#### Support Status

Symbol          | Description
----------------|----------------------------------------
:green_circle:  | Supported
:yellow_circle: | Partial support (see footnote provided)
:red_circle:    | Unsupported

</details>

### Overall status: (77.5/90) _~86.1%_

## [3.17.0] - 2022-05-10

### Status: (9/16)

Method Name                           | Message Type                | Supported      | Tracking Issue(s)
--------------------------------------|:---------------------------:|:--------------:|------------------
[`notebookDocument/didOpen`]          | :arrow_right:               | :red_circle:   |
[`notebookDocument/didChange`]        | :arrow_right:               | :red_circle:   |
[`notebookDocument/didSave`]          | :arrow_right:               | :red_circle:   |
[`notebookDocument/didClose`]         | :arrow_right:               | :red_circle:   |
[`textDocument/prepareTypeHierarchy`] | :leftwards_arrow_with_hook: | :green_circle: |
[`typeHierarchy/supertypes`]          | :leftwards_arrow_with_hook: | :green_circle: |
[`typeHierarchy/subtypes`]            | :leftwards_arrow_with_hook: | :green_circle: |
[`textDocument/inlayHint`]            | :leftwards_arrow_with_hook: | :green_circle: | ~[#352]~
[`inlayHint/resolve`]                 | :leftwards_arrow_with_hook: | :green_circle: | ~[#352]~
[`workspace/inlayHint/refresh`]       | :arrow_right_hook:          | :green_circle: | ~[#352]~
[`textDocument/inlineValue`]          | :leftwards_arrow_with_hook: | :green_circle: | ~[#352]~
[`workspace/inlineValue/refresh`]     | :arrow_right_hook:          | :green_circle: | ~[#352]~
[`textDocument/diagnostic`]           | :leftwards_arrow_with_hook: | :red_circle:   | [#374]
[`workspace/diagnostic`]              | :leftwards_arrow_with_hook: | :red_circle:   | [#374]
[`workspace/diagnostic/refresh`]      | :arrow_right_hook:          | :red_circle:   | [#374]
[`workspaceSymbol/resolve`]           | :leftwards_arrow_with_hook: | :green_circle: |

[`notebookDocument/didOpen`]: https://microsoft.github.io/language-server-protocol/specification#notebookDocument_didOpen
[`notebookDocument/didChange`]: https://microsoft.github.io/language-server-protocol/specification#notebookDocument_didChange
[`notebookDocument/didSave`]: https://microsoft.github.io/language-server-protocol/specification#notebookDocument_didSave
[`notebookDocument/didClose`]: https://microsoft.github.io/language-server-protocol/specification#notebookDocument_didClose
[`textDocument/prepareTypeHierarchy`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_prepareTypeHierarchy
[`typeHierarchy/supertypes`]: https://microsoft.github.io/language-server-protocol/specification#typeHierarchy_supertypes
[`typeHierarchy/subtypes`]: https://microsoft.github.io/language-server-protocol/specification#typeHierarchy_subtypes
[`textDocument/inlayHint`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_inlayHint
[`inlayHint/resolve`]: https://microsoft.github.io/language-server-protocol/specification#inlayHint_resolve
[`workspace/inlayHint/refresh`]: https://microsoft.github.io/language-server-protocol/specification#workspace_inlayHint_refresh
[`textDocument/inlineValue`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_inlineValue
[`workspace/inlineValue/refresh`]: https://microsoft.github.io/language-server-protocol/specification#workspace_inlineValue_refresh
[`textDocument/diagnostic`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_diagnostic
[`workspace/diagnostic`]: https://microsoft.github.io/language-server-protocol/specification#workspace_diagnostic
[`workspace/diagnostic/refresh`]: https://microsoft.github.io/language-server-protocol/specification#workspace_diagnostic_refresh
[`workspaceSymbol/resolve`]: https://microsoft.github.io/language-server-protocol/specification#workspace_symbolResolve

[#352]: https://github.com/ebkalderon/tower-lsp/issues/352
[#374]: https://github.com/ebkalderon/tower-lsp/issues/374

## [3.16.0] - 2020-12-14

### Status: (18/20)

Method Name                                | Message Type                | Supported      | Tracking Issue(s)
-------------------------------------------|:---------------------------:|:--------------:|------------------
[`$/setTrace`]                             | :arrow_right:               | :red_circle:   |
[`$/logTrace`]                             | :arrow_right:               | :red_circle:   |
[`textDocument/prepareCallHierarchy`]      | :leftwards_arrow_with_hook: | :green_circle: |
[`callHierarchy/incomingCalls`]            | :leftwards_arrow_with_hook: | :green_circle: |
[`callHierarchy/outgoingCalls`]            | :leftwards_arrow_with_hook: | :green_circle: |
[`workspace/codeLens/refresh`]             | :arrow_right_hook:          | :green_circle: |
[`textDocument/semanticTokens/full`]       | :leftwards_arrow_with_hook: | :green_circle: | ~[#146]~
[`textDocument/semanticTokens/full/delta`] | :leftwards_arrow_with_hook: | :green_circle: | ~[#146]~
[`textDocument/semanticTokens/range`]      | :leftwards_arrow_with_hook: | :green_circle: | ~[#146]~
[`workspace/semanticTokens/refresh`]       | :arrow_right_hook:          | :green_circle: | ~[#146]~
[`textDocument/moniker`]                   | :leftwards_arrow_with_hook: | :green_circle: |
[`codeAction/resolve`]                     | :arrow_right_hook:          | :green_circle: |
[`textDocument/linkedEditingRange`]        | :leftwards_arrow_with_hook: | :green_circle: |
[`workspace/willCreateFiles`]              | :leftwards_arrow_with_hook: | :green_circle: |
[`workspace/didCreateFiles`]               | :arrow_right:               | :green_circle: |
[`workspace/willRenameFiles`]              | :leftwards_arrow_with_hook: | :green_circle: |
[`workspace/didRenameFiles`]               | :arrow_right:               | :green_circle: |
[`workspace/willDeleteFiles`]              | :leftwards_arrow_with_hook: | :green_circle: |
[`workspace/didDeleteFiles`]               | :arrow_right:               | :green_circle: |
[`window/showDocument`]                    | :arrow_right_hook:          | :green_circle: |

[`$/setTrace`]: https://microsoft.github.io/language-server-protocol/specification#setTrace
[`$/logTrace`]: https://microsoft.github.io/language-server-protocol/specification#logTrace
[`textDocument/prepareCallHierarchy`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_prepareCallHierarchy
[`callHierarchy/incomingCalls`]: https://microsoft.github.io/language-server-protocol/specification#callHierarchy_incomingCalls
[`callHierarchy/outgoingCalls`]: https://microsoft.github.io/language-server-protocol/specification#callHierarchy_outgoingCalls
[`workspace/codeLens/refresh`]: https://microsoft.github.io/language-server-protocol/specification#codeLens_refresh
[`textDocument/semanticTokens/full`]: https://microsoft.github.io/language-server-protocol/specification#semanticTokens_fullRequest
[`textDocument/semanticTokens/full/delta`]: https://microsoft.github.io/language-server-protocol/specification#semanticTokens_deltaRequest
[`textDocument/semanticTokens/range`]: https://microsoft.github.io/language-server-protocol/specification#semanticTokens_rangeRequest
[`workspace/semanticTokens/refresh`]: https://microsoft.github.io/language-server-protocol/specification#semanticTokens_refreshRequest
[`textDocument/moniker`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_moniker
[`codeAction/resolve`]: https://microsoft.github.io/language-server-protocol/specification#codeAction_resolve
[`textDocument/linkedEditingRange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_linkedEditingRange
[`workspace/willCreateFiles`]: https://microsoft.github.io/language-server-protocol/specification#workspace_willCreateFiles
[`workspace/didCreateFiles`]: https://microsoft.github.io/language-server-protocol/specification#workspace_didCreateFiles
[`workspace/willRenameFiles`]: https://microsoft.github.io/language-server-protocol/specification#workspace_willRenameFiles
[`workspace/didRenameFiles`]: https://microsoft.github.io/language-server-protocol/specification#workspace_didRenameFiles
[`workspace/willDeleteFiles`]: https://microsoft.github.io/language-server-protocol/specification#workspace_willDeleteFiles
[`workspace/didDeleteFiles`]: https://microsoft.github.io/language-server-protocol/specification#workspace_didDeleteFiles
[`window/showDocument`]: https://microsoft.github.io/language-server-protocol/specification#window_showDocument

[#146]: https://github.com/ebkalderon/tower-lsp/issues/146

## [3.15.0] - 2020-01-14

### Status: (1/4)

Method Name                        | Message Type                | Supported      | Tracking Issue(s)
-----------------------------------|:---------------------------:|:--------------:|------------------
[`$/progress`]                     | :arrow_right: :arrow_left:  | :red_circle:   | ~[#176]~, [#380], [#381]
[`window/workDoneProgress/create`] | :arrow_right_hook:          | :red_circle:   | [#381]
[`window/workDoneProgress/cancel`] | :arrow_right:               | :red_circle:   | [#381]
[`textDocument/selectionRange`]    | :leftwards_arrow_with_hook: | :green_circle: | ~[#10]~

[`$/progress`]: https://microsoft.github.io/language-server-protocol/specification#progress
[`window/workDoneProgress/create`]: https://microsoft.github.io/language-server-protocol/specification#window_workDoneProgress_create
[`window/workDoneProgress/cancel`]: https://microsoft.github.io/language-server-protocol/specification#window_workDoneProgress_cancel
[`textDocument/selectionRange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_selectionRange

[#10]: https://github.com/ebkalderon/tower-lsp/issues/10
[#176]: https://github.com/ebkalderon/tower-lsp/issues/176
[#380]: https://github.com/ebkalderon/tower-lsp/issues/380
[#381]: https://github.com/ebkalderon/tower-lsp/issues/381

## [3.14.0] - 2018-12-13

### Status: (1/1)

Method Name                  | Message Type                | Supported      | Tracking Issue(s)
-----------------------------|:---------------------------:|:--------------:|------------------
[`textDocument/declaration`] | :leftwards_arrow_with_hook: | :green_circle: | ~[#10]~

[`textDocument/declaration`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_declaration

[#10]: https://github.com/ebkalderon/tower-lsp/issues/10

## [3.12.0] - 2018-08-23

### Status: (1/1)

Method Name                    | Message Type                | Supported      | Tracking Issue(s)
-------------------------------|:---------------------------:|:--------------:|------------------
[`textDocument/prepareRename`] | :leftwards_arrow_with_hook: | :green_circle: | ~[#10]~

[`textDocument/prepareRename`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_prepareRename

[#10]: https://github.com/ebkalderon/tower-lsp/issues/10

## [3.10.0] - 2018-07-23

### Status: (1/1)

Method Name                   | Message Type                | Supported      | Tracking Issue(s)
------------------------------|:---------------------------:|:--------------:|------------------
[`textDocument/foldingRange`] | :leftwards_arrow_with_hook: | :green_circle: | ~[#10]~

[`textDocument/foldingRange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_foldingRange

[#10]: https://github.com/ebkalderon/tower-lsp/issues/10

## [3.6.0] - 2018-02-22

### Status: (7/7)

Method Name                             | Message Type                | Supported      | Tracking Issue(s)
----------------------------------------|:---------------------------:|:--------------:|------------------
[`workspace/workspaceFolders`]          | :arrow_right_hook:          | :green_circle: | ~[#13]~
[`workspace/didChangeWorkspaceFolders`] | :arrow_right:               | :green_circle: | ~[#8]~
[`workspace/configuration`]             | :arrow_right_hook:          | :green_circle: | ~[#13]~
[`textDocument/typeDefinition`]         | :leftwards_arrow_with_hook: | :green_circle: | ~[#10]~
[`textDocument/implementation`]         | :leftwards_arrow_with_hook: | :green_circle: | ~[#10]~
[`textDocument/documentColor`]          | :leftwards_arrow_with_hook: | :green_circle: | ~[#10]~
[`textDocument/colorPresentation`]      | :leftwards_arrow_with_hook: | :green_circle: | ~[#10]~

[`workspace/workspaceFolders`]: https://microsoft.github.io/language-server-protocol/specification#workspace_workspaceFolders
[`workspace/didChangeWorkspaceFolders`]: https://microsoft.github.io/language-server-protocol/specification#workspace_didChangeWorkspaceFolders
[`workspace/configuration`]: https://microsoft.github.io/language-server-protocol/specification#workspace_configuration
[`textDocument/typeDefinition`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_typeDefinition
[`textDocument/implementation`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_implementation
[`textDocument/documentColor`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentColor
[`textDocument/colorPresentation`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_colorPresentation

[#8]: https://github.com/ebkalderon/tower-lsp/issues/8
[#10]: https://github.com/ebkalderon/tower-lsp/issues/10
[#13]: https://github.com/ebkalderon/tower-lsp/issues/13

## [3.0.0] - 2017-02-08

### Status: (39.5/40)

Method Name                          | Message Type                | Supported           | Tracking Issue(s)
-------------------------------------|:---------------------------:|:-------------------:|------------------
[`initialize`]                       | :leftwards_arrow_with_hook: | :green_circle:      |
[`initialized`]                      | :arrow_right:               | :green_circle:      |
[`client/registerCapability`]        | :arrow_right_hook:          | :green_circle:      | ~[#13]~
[`client/unregisterCapability`]      | :arrow_right_hook:          | :green_circle:      | ~[#13]~
[`shutdown`]                         | :leftwards_arrow_with_hook: | :green_circle:      |
[`exit`]                             | :arrow_right:               | :green_circle:      |
[`textDocument/didOpen`]             | :arrow_right:               | :green_circle:      |
[`textDocument/didChange`]           | :arrow_right:               | :green_circle:      |
[`textDocument/willSave`]            | :arrow_right:               | :green_circle:      | ~[#118]~
[`textDocument/willSaveWaitUntil`]   | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`textDocument/didSave`]             | :arrow_right:               | :green_circle:      |
[`textDocument/didClose`]            | :arrow_right:               | :green_circle:      |
[`textDocument/definition`]          | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`textDocument/references`]          | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`textDocument/documentHighlight`]   | :leftwards_arrow_with_hook: | :green_circle:      |
[`textDocument/documentLink`]        | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`documentLink/resolve`]             | :leftwards_arrow_with_hook: | :green_circle:      | ~[#12]~
[`textDocument/hover`]               | :leftwards_arrow_with_hook: | :green_circle:      |
[`textDocument/codeLens`]            | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`codeLens/resolve`]                 | :leftwards_arrow_with_hook: | :green_circle:      | ~[#11]~
[`textDocument/documentSymbol`]      | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`textDocument/completion`]          | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`completionItem/resolve`]           | :leftwards_arrow_with_hook: | :green_circle:      | ~[#9]~
[`textDocument/publishDiagnostics`]  | :arrow_left:                | :green_circle:      |
[`textDocument/signatureHelp`]       | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`textDocument/codeAction`]          | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`textDocument/formatting`]          | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`textDocument/rangeFormatting`]     | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`textDocument/onTypeFormatting`]    | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`textDocument/rename`]              | :leftwards_arrow_with_hook: | :green_circle:      | ~[#10]~
[`workspace/symbol`]                 | :leftwards_arrow_with_hook: | :green_circle:      | ~[#8]~
[`workspace/didChangeConfiguration`] | :arrow_right:               | :green_circle:      | ~[#8]~
[`workspace/didChangeWatchedFiles`]  | :arrow_right:               | :green_circle:      | ~[#8]~
[`workspace/executeCommand`]         | :leftwards_arrow_with_hook: | :green_circle:      | ~[#8]~
[`workspace/applyEdit`]              | :arrow_right_hook:          | :green_circle:      | ~[#13]~
[`window/showMessage`]               | :arrow_left:                | :green_circle:      |
[`window/showMessageRequest`]        | :arrow_right_hook:          | :green_circle:      | ~[#13]~
[`window/logMessage`]                | :arrow_left:                | :green_circle:      |
[`telemetry/event`]                  | :arrow_left:                | :green_circle:      |
[`$/cancelRequest`]                  | :arrow_right: :arrow_left:  | :yellow_circle:[^1] | ~[#145]~, [#231]

[`initialize`]: https://microsoft.github.io/language-server-protocol/specification#initialize
[`initialized`]: https://microsoft.github.io/language-server-protocol/specification#initialized
[`client/registerCapability`]: https://microsoft.github.io/language-server-protocol/specification#client_registerCapability
[`client/unregisterCapability`]: https://microsoft.github.io/language-server-protocol/specification#client_unregisterCapability
[`shutdown`]: https://microsoft.github.io/language-server-protocol/specification#shutdown
[`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
[`textDocument/didOpen`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didOpen
[`textDocument/didChange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didChange
[`textDocument/willSave`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_willSave
[`textDocument/willSaveWaitUntil`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_willSaveWaitUntil
[`textDocument/didSave`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didOpen
[`textDocument/didClose`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didClose
[`textDocument/definition`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_definition
[`textDocument/references`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_references
[`textDocument/documentHighlight`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentHighlight
[`textDocument/documentLink`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentLink
[`documentLink/resolve`]: https://microsoft.github.io/language-server-protocol/specification#documentLink_resolve
[`textDocument/hover`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_hover
[`textDocument/codeLens`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_codeLens
[`codeLens/resolve`]: https://microsoft.github.io/language-server-protocol/specification#codeLens_resolve
[`textDocument/documentSymbol`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentSymbol
[`textDocument/completion`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_completion
[`completionItem/resolve`]: https://microsoft.github.io/language-server-protocol/specification#completionItem_resolve
[`textDocument/publishDiagnostics`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_publishDiagnostics
[`textDocument/signatureHelp`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_signatureHelp
[`textDocument/codeAction`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_codeAction
[`textDocument/formatting`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_formatting
[`textDocument/rangeFormatting`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_rangeFormatting
[`textDocument/onTypeFormatting`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_onTypeFormatting
[`textDocument/rename`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_rename
[`workspace/symbol`]: https://microsoft.github.io/language-server-protocol/specification#workspace_symbol
[`workspace/didChangeConfiguration`]: https://microsoft.github.io/language-server-protocol/specification#workspace_didChangeConfiguration
[`workspace/didChangeWatchedFiles`]: https://microsoft.github.io/language-server-protocol/specification#workspace_didChangeWatchedFiles
[`workspace/executeCommand`]: https://microsoft.github.io/language-server-protocol/specification#workspace_executeCommand
[`workspace/applyEdit`]: https://microsoft.github.io/language-server-protocol/specification#workspace_applyEdit
[`window/showMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessage
[`window/showMessageRequest`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessageRequest
[`window/logMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_logMessage
[`telemetry/event`]: https://microsoft.github.io/language-server-protocol/specification#telemetry_event
[`$/cancelRequest`]: https://microsoft.github.io/language-server-protocol/specification#cancelRequest

[^1]: Server-to-client `$/cancelRequest` support is not yet implemented. However, the raw message can be emitted manually using `Client::send_notification()`.
      Client-to-server support is implemented via async/await task cancellation.

[#8]: https://github.com/ebkalderon/tower-lsp/issues/8
[#9]: https://github.com/ebkalderon/tower-lsp/issues/9
[#10]: https://github.com/ebkalderon/tower-lsp/issues/10
[#11]: https://github.com/ebkalderon/tower-lsp/issues/11
[#12]: https://github.com/ebkalderon/tower-lsp/issues/12
[#13]: https://github.com/ebkalderon/tower-lsp/issues/13
[#118]: https://github.com/ebkalderon/tower-lsp/issues/118
[#145]: https://github.com/ebkalderon/tower-lsp/issues/145
[#231]: https://github.com/ebkalderon/tower-lsp/issues/231

[3.17.0]: https://microsoft.github.io/language-server-protocol/specification#version_3_17_0
[3.16.0]: https://microsoft.github.io/language-server-protocol/specification#version_3_16_0
[3.15.0]: https://microsoft.github.io/language-server-protocol/specification#version_3_15_0
[3.14.0]: https://microsoft.github.io/language-server-protocol/specification#version_3_14_0
[3.12.0]: https://microsoft.github.io/language-server-protocol/specification#version_3_12_0
[3.10.0]: https://microsoft.github.io/language-server-protocol/specification#version_3_10_0
[3.6.0]: https://microsoft.github.io/language-server-protocol/specification#version_3_6_0
[3.0.0]: https://microsoft.github.io/language-server-protocol/specification#version_3_0_0
