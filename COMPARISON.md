| Feature                     | dyntable                             | [thin_trait_object]              | [cglue]            | [abi_stable]                     | [vtable]           |
|-----------------------------|--------------------------------------|----------------------------------|--------------------|----------------------------------|--------------------|
| Version                     | 0.1.0                                | 1.1.2                            | 0.2.12             | 0.11.1                           | 0.1.9              |
| License                     | Apache-2.0                           | MIT/Apache-2.0                   | MIT                | MIT/Apache-2.0                   | GPLv3              |
| no-std support              | :heavy_check_mark:                   | :heavy_check_mark:[^thin-monly]  | :heavy_check_mark: | :x:                              | :heavy_check_mark: |
| Pointer Type                | Fat                                  | Thin                             | Fat                | Fat                              | Fat                |
| Supports References         | :heavy_check_mark:                   | :x:                              | :heavy_check_mark: | :heavy_check_mark:               | :heavy_check_mark: |
| Non-Box Trait Containers    | :x:                                  | :x:                              | :heavy_check_mark: | :heavy_check_mark:               | :heavy_check_mark: |
| Custom Allocators           | :heavy_check_mark:                   | :x:                              | :x:                | :x:                              | :x:                |
| Uses Thunks[^m-thunk]       | By-Value Fns                         | Never                            | Always             | Unknown                          | Always             |
| Trait Bounds / Supertraits  | :heavy_check_mark:[^dyntable-bounds] | :heavy_check_mark:[^thin-bounds] | :x:                | :heavy_check_mark:[^sabi-bounds] | :x:                |
| Trait Groups[^m-groups]     | :x:                                  | :x:                              | :heavy_check_mark: | :x:                              | :x:                |
| Trait Generics              | :heavy_check_mark:                   | :x:                              | :heavy_check_mark: | :heavy_check_mark:               | :x:                |
| Default Generics            | :heavy_check_mark:                   | :x:                              | :x:                | :heavy_check_mark:               | :x:                |
| Trait Lifetime Generics     | :heavy_check_mark:                   | :x:                              | :x:                | :heavy_check_mark:               | :x:                |
| Fn Generics                 | :x:                                  | :x:                              | Panics             | :x:                              | :x:                |
| Fn Lifetime Generics        | :heavy_check_mark:                   | :x:                              | :heavy_check_mark: | :heavy_check_mark:               | :x:                |
| Upcasting                   | :heavy_check_mark:                   | :x:                              | :x:                | :x:                              | :x:                |
| Downcasting                 | :x:                                  | :x:                              | :x:                | :heavy_check_mark:               | :heavy_check_mark: |
| Constants                   | :x:                                  | :x:                              | :x:                | :x:                              | :heavy_check_mark: |
| Associated Types            | :x:                                  | :x:                              | :heavy_check_mark: | :heavy_check_mark:               | :x:                |
| Field Offsets               | :x:                                  | :x:                              | :x:                | :x:                              | :heavy_check_mark: |
| Duplicate Trait Name        | :heavy_check_mark:                   | :heavy_check_mark:               | :heavy_check_mark: | :heavy_check_mark:               | :x:                |
| Layout Validation           | :x:                                  | :x:                              | :heavy_check_mark: | :heavy_check_mark:               | :x:                |
| Single Macro[^m-1-macro]    | :heavy_check_mark:                   | :heavy_check_mark:               | :x:                | :heavy_check_mark:               | :x:                |
| No Reborrowing[^m-reborrow] | :x:                                  | No Ref Types                     | :x:                | :x:                              | :x:                |

[thin_trait_object]: https://crates.io/crates/thin_trait_object
[cglue]: https://crates.io/crates/cglue
[abi_stable]: https://crates.io/crates/abi_stable
[vtable]: https://crates.io/crates/vtable

[^alternative-updates]: The listed alternative crates may have been updated to support unlisted features.
[^thin-monly]: Thin-trait-object has no runtime component.
[^dyntable-bounds]: Dyntable traits can only have bounds on other dyntable traits (without associated types) and Send or Sync.
[^thin-bounds]: Requires manually writing impls and thunks for bound methods.
[^sabi-bounds]: Abi-Stable traits can only have bounds on a specific selection of traits (see abi_stable docs for details),
[^m-groups]: Trait groups are non trait bound / supertrait based groupings of traits. See the cglue docs for details.
[^m-1-macro]: Only one macro is required, on the trait definition.
[^m-reborrow]: Reborrowing is when a reference is represented by an owned type, and needs to have a function called to borrow it.
[^m-thunk]: Intermediary functions used for type conversion that cannot be inlined.
