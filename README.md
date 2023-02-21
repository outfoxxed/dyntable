# dyntable
[![crates.io](https://img.shields.io/crates/v/dyntable?style=for-the-badge&logo=rust)](https://crates.io/crates/dyntable)
[![docs.rs](https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs)](https://docs.rs/crate/dyntable/latest)

FFI Safe traits implemented as fat pointers.

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
A few other crates may suit your needs better than dyntable.
Here's a simple feature matrix[^alternative-updates]

| Feature                         | dyntable           | [thin_trait_object]   | [cglue]            | [abi_stable]      | [vtable]           |
|---------------------------------|--------------------|-----------------------|--------------------|-------------------|--------------------|
| License                         | Apache-2.0         | MIT or Apache-2.0     | MIT                | MIT or Apache-2.0 | GPLv3              |
| C FFI                           | :heavy_check_mark: | :heavy_check_mark:    | :heavy_check_mark: | :x:               | :heavy_check_mark: |
| Rust FFI                        | :heavy_check_mark: | :heavy_check_mark:    | :heavy_check_mark: | :heavy_check_mark:| :heavy_check_mark: |
| Pointer Type                    | Fat                | Thin                  | Fat                | Fat               | Fat                |
| Trait Bounds / Supertraits      | :heavy_check_mark: | Partial               | :x:                | Predefined Only   | :x:                |
| Trait Generics                  | :heavy_check_mark: | :x:                   | :heavy_check_mark: | :heavy_check_mark:| :x:                |
| Trait Lifetime Generics         | :heavy_check_mark: | :x:                   | :x:                | :heavy_check_mark:| :x:                |
| Function Generics               | :x:                | :x:                   | Panics             | :x:               | :x:                |
| Function Lifetime Generics      | :heavy_check_mark: | :x:                   | :heavy_check_mark: | :heavy_check_mark:| :x:                |
| Upcasting                       | :heavy_check_mark: | :x:                   | :x:                | :x:               | :x:                |
| Downcasting                     | :x:                | :x:                   | :x:                | :heavy_check_mark:| :heavy_check_mark: |
| Constants                       | :x:                | :x:                   | :x:                | :x:               | :heavy_check_mark: |
| Associated Types                | :x:                | :x:                   | :x:                | :heavy_check_mark:| :x:                |
| Field Offsets                   | :x:                | :x:                   | :x:                | :x:               | :heavy_check_mark: |
| Duplicate Trait Name (In Crate) | :heavy_check_mark: | :heavy_check_mark:    | :heavy_check_mark: | :heavy_check_mark:| :x:                |
| Layout Validation               | :x:                | :x:                   | Optional           | :heavy_check_mark:| :x:                |

[thin_trait_object]: https://crates.io/crates/thin_trait_object
[cglue]: https://crates.io/crates/cglue
[abi_stable]: https://crates.io/crates/abi_stable
[vtable]: https://crates.io/crates/vtable

[^alternative-updates]: The listed alternative crates may have been updated to support unlisted features.
