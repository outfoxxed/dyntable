//! Parsing code for proc macro options

use proc_macro2::Span;
use syn::{Ident, parse::{Parse, ParseStream}, Token, punctuated::Punctuated};

#[derive(Debug)]
pub struct AttributeOptions {
	pub repr: StructRepr,
	pub abi: FunctionAbi,
	pub drop: Option<FunctionAbi>,
	pub vtable_name: Option<Ident>,
}

#[derive(Debug)]
pub enum StructRepr {
	Rust, // #[repr(Rust)] is not allowed
	Other(Ident),
}

#[derive(Debug)]
pub enum FunctionAbi {
	Rust,
	Other(Ident),
}

impl Parse for StructRepr {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		let repr = input.parse::<Ident>()?;

		Ok(match &repr.to_string() as &str {
			"Rust" => StructRepr::Rust,
			_ => StructRepr::Other(repr),
		})
	}
}

impl Parse for FunctionAbi {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		let abi = input.parse::<Ident>()?;

		Ok(match &abi.to_string() as &str {
			"Rust" => FunctionAbi::Rust,
			_ => FunctionAbi::Other(abi),
		})
	}
}

impl Parse for AttributeOptions {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		enum AttrOption {
			Repr(StructRepr),
			Abi(FunctionAbi),
			Drop(Option<FunctionAbi>),
			VTableName(Ident),
		}

		struct SpannedAttrOption(Span, AttrOption);

		impl Parse for SpannedAttrOption {
			fn parse(input: ParseStream) -> syn::Result<Self> {
				let option_name = input.parse::<Ident>()?;
				let _ = input.parse::<Token![=]>()?;

				Ok(SpannedAttrOption(option_name.span(), match &option_name.to_string() as &str {
					"repr" => AttrOption::Repr(input.parse::<StructRepr>()?),
					"abi" => AttrOption::Abi(input.parse::<FunctionAbi>()?),
					"drop" => AttrOption::Drop({
						let abi = input.parse::<Ident>()?;

						match &abi.to_string() as &str {
							"none" => None,
							"Rust" => Some(FunctionAbi::Rust),
							_ => Some(FunctionAbi::Other(abi)),
						}
					}),
					"vtable" => AttrOption::VTableName(input.parse::<Ident>()?),
					_ => return Err(syn::Error::new(
						option_name.span(),
						&format!("Unknown option '{}'", option_name.to_string()),
					)),
				}))
			}
		}

		let options = Punctuated::<SpannedAttrOption, Token![,]>::parse_terminated(input)?;

		struct OptionalOptions {
			repr: Option<StructRepr>,
			abi: Option<FunctionAbi>,
			drop: Option<Option<FunctionAbi>>,
			vtable_name: Option<Ident>,
		}

		let mut option_struct = OptionalOptions {
			repr: None,
			abi: None,
			drop: None,
			vtable_name: None,
		};

		for SpannedAttrOption(span, option) in options {
			let duplicate = match option {
				AttrOption::Repr(x) => matches!(option_struct.repr.replace(x), Some(_)),
				AttrOption::Abi(x) => matches!(option_struct.abi.replace(x), Some(_)),
				AttrOption::Drop(x) => matches!(option_struct.drop.replace(x), Some(_)),
				AttrOption::VTableName(x) => matches!(option_struct.vtable_name.replace(x), Some(_)),
			};

			if duplicate {
				return Err(syn::Error::new(
					span,
					"option can only be defined once",
				))
			}
		}

		Ok(AttributeOptions {
			repr: option_struct.repr.unwrap_or(StructRepr::Rust),
			abi: option_struct.abi.unwrap_or(FunctionAbi::Other(Ident::new("C", Span::call_site()))),
			drop: option_struct.drop.unwrap_or(Some(FunctionAbi::Other(Ident::new("C", Span::call_site())))),
			vtable_name: option_struct.vtable_name,
		})
	}
}
