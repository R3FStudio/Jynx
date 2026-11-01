# Architecture

The public packages separate model execution from terminal presentation and client APIs. The [repository guide](docs/development/repository.md) explains their boundaries. The [source map](docs/reference/source-map.md) identifies the files that own each behavior.

The CLI consumes shared Rust crates. The harness owns tools, permissions, context, and state. HTTP SDKs and the subprocess client serve different integration needs. These boundaries allow independent tests and builds where source dependencies permit it.
