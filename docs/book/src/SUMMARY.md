# Summary

[Introduction](./introduction.md)
- [What is ToS?](./introduction.md)
- [Quickstart](./quickstart.md)
- [Design goals](./design-goals.md)

[Architecture](./architecture.md)
- [Layered stack](./architecture.md)
- [Workspace layout](./workspace.md)
- [The TosAdapter contract](./adapter-contract.md)

[Schema (SDL)](./03-sdl.md)
- [Type system](./03-sdl.md#types)
- [Writing a schema](./03-sdl.md#writing-a-schema)
- [Pull / push / infer / diff / validate](./03-sdl.md#commands)

[Wire protocol](./04-protocol.md)
- [Framing](./04-protocol.md#framing)
- [Handshake](./04-protocol.md#handshake)
- [Streams and batches](./04-protocol.md#streams)
- [Watch & topology](./04-protocol.md#watch)
- [Live hexdump example](./04-protocol.md#live-capture)

[Adapters](./05-adapters.md)
- [Supported backends](./05-adapters.md)
- [URI scheme reference](./05-adapters.md#uri-schemes)
- [Type mapping per backend](./05-adapters.md#type-mappings)

[CLI](./06-cli.md)
- [Subcommands](./06-cli.md)
- [push](./06-cli.md#push)
- [sync](./06-cli.md#sync)
- [schema](./06-cli.md#schema)
- [topology / node / status / log](./06-cli.md#daemon)
- [TOML topology file](./06-cli.md#topology)

[Security](./07-security.md)
- [Threat model](./07-security.md#threat-model)
- [Handshake authenticity](./07-security.md#authenticity)
- [Optional payload encryption](./07-security.md#encryption)
- [Audit pipeline](./07-security.md#audit)

[Operations](./08-ops.md)
- [Building](./08-ops.md#building)
- [Cross-compile (musl)](./08-ops.md#cross)
- [Packaging (.deb)](./08-ops.md#packaging)
- [CI matrix](./08-ops.md#ci)
- [mdBook](./08-ops.md#book)

[Reference](./reference.md)
- [Glossary](./reference.md#glossary)
- [Changelog](./reference.md#changelog)
