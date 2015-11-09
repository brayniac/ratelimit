# ratelimit - a token bucket ratelimiter for rust

ratelimit provides a token bucket ratelimiter which can be used by a single thread, or shared across threads by using a channel to push tokens to the ratelimiter

[![Build Status](https://travis-ci.org/brayniac/ratelimit.svg?branch=master)](https://travis-ci.org/brayniac/ratelimit)
[![crates.io](http://meritbadge.herokuapp.com/ratelimit)](https://crates.io/crates/ratelimit)
[![License](http://img.shields.io/:license-mit-blue.svg)](http://doge.mit-license.org)

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

## Documentation

View the docs here: [http://brayniac.github.io/ratelimit/](http://brayniac.github.io/ratelimit/)

## Features

* token bucket ratelimiter
* single or multi threaded uses

## Future work

* additional ratelimiter models
