# ratelimit - a token bucket ratelimiter for rust

ratelimit provides a token bucket ratelimiter which can be used by a single thread, or shared across threads by using a channel to push tokens to the ratelimiter

The API documentation of this library can be found at
[docs.rs/ratelimit](https://docs.rs/ratelimit/).

[![conduct-badge][]][conduct] [![travis-badge][]][travis] [![downloads-badge][] ![release-badge][]][crate] [![license-badge][]](#license)

[conduct-badge]: https://img.shields.io/badge/%E2%9D%A4-code%20of%20conduct-blue.svg
[travis-badge]: https://img.shields.io/travis/brayniac/ratelimit/master.svg
[downloads-badge]: https://img.shields.io/crates/d/ratelimit.svg
[release-badge]: https://img.shields.io/crates/v/ratelimit.svg
[license-badge]: https://img.shields.io/crates/l/ratelimit.svg
[conduct]: https://brayniac.github.io/conduct
[travis]: https://travis-ci.org/brayniac/ratelimit
[crate]: https://crates.io/crates/ratelimit
[Cargo]: https://github.com/rust-lang/cargo

## Code of Conduct

**NOTE**: All conversations and contributions to this project shall adhere to the [Code of Conduct][conduct]

## Usage

To use `ratelimit`, first add this to your `Cargo.toml`:

```toml
[dependencies]
ratelimit = "*"
```

Then, add this to your crate root:

```rust
extern crate ratelimit;
```

## Features

* token bucket ratelimiter
* single or multi threaded uses

## Future work

* additional ratelimiter models

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
