# dyntable
[![crates.io](https://img.shields.io/crates/v/dyntable?style=for-the-badge&logo=rust)](https://crates.io/crates/dyntable)
[![docs.rs](https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs)](https://docs.rs/dyntable/latest/dyntable)

(Almost) fully featured, Idiomatic, and FFI Safe traits.

# Overview
This crate is an alternative implementation of Rust trait objects that
aims to get around one main limitation: The ABI of trait objects is
unspecified.

Usually this limitation is not an issue because the majority of rust code
is statically linked, but it means trait objects are largely useless for
a variety of situations such as:
- Implementing a plugin system
- Interacting with C, or any other language
- Dynamic linking
- On the fly codegen

This crate implements idiomatic trait objects, implemented using fat pointers
similar to native rust traits, with support for trait bounds (inheritance) and
upcasting. Implementing dyntable traits works exactly the same as normal rust traits.

## Notable features
- Uses fat pointers internally
- Trait bounds and trait upcasting
- Custom allocator support
- Uses existing `dyn` syntax
- Only one annotation macro required per trait
- (Limited) associated type support

# Examples
Below is a simple example from the docs

```rust
use dyntable::*;

#[dyntable]
trait MessageBuilder {
    // Note that String is not FFI safe, but is used here for simplicity.
    extern "C" fn build(&self) -> String;
}

struct Greeter(&'static str);

impl MessageBuilder for Greeter {
    extern "C" fn build(&self) -> String {
        format!("Hello {}!", self.0)
    }
}

let greeter = Greeter("World");

// move the greeter into a DynBox of MessageBuilder. This box can hold any
// object safe MessageBuilder implementation.
let greeter_box = DynBox::<dyn MessageBuilder>::new(greeter);

// methods implemented on a dyntrait are callable directly from the DynBox.
assert_eq!(greeter_box.build(), "Hello World!");
```

# Alternatives
A few other crates may suit your needs better than dyntable. There is a comparison in
[COMPARISON.md](COMPARISON.md)
