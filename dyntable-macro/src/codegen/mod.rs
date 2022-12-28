//! Code generation and related utilities

use proc_macro2::Span;
use syn::{Ident, Path, PathSegment};

pub mod vtable;
pub mod vtable_impl;
pub mod dyntable;

/// Build an absolute path from a list of segments.
///
/// The path will be prefixed with ::
///
/// # Example
/// ```
/// absolute_path(["a", "b"])
/// ```
/// returns the path `::a::b`
fn absolute_path<const N: usize>(segments: [&str; N]) -> Path {
	Path {
		leading_colon: Some(Default::default()),
		segments: segments
			.map(|segment| PathSegment::from(Ident::new(segment, Span::call_site())))
			.into_iter()
			.collect(),
	}
}
