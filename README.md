# dyntable
[![crates.io](https://img.shields.io/crates/v/dyntable?style=for-the-badge&logo=rust)](https://crates.io/crates/dyntable)
[![docs.rs](https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs)](https://docs.rs/crate/dyntable/latest)

Fully featured, Idiomatic, and FFI Safe traits.

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

# Alternatives
A few other crates may suit your needs better than dyntable. There is a comparison in
[COMPARISON.md](COMPARISON.md)
