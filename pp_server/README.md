# pp_server

A TCP poker server. Host a poker server with:

```bash
RUST_LOG=info pp_server --bind $host
```

Poker clients can connect with [pp_client][2].

## Related artifacts

- [Library crate][1]
- [Client crate][2]
- [Bots crate][3]
- [All-in-one Docker image (recommended)][4]

[1]: https://crates.io/crates/private_poker
[2]: https://crates.io/crates/pp_client
[3]: https://crates.io/crates/pp_bots
[4]: https://hub.docker.com/r/ognf/poker
