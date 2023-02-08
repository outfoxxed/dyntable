//! Parsing code for proc macro options

use proc_macro2::Span;
use syn::{
	parse::{Parse, ParseStream},
	punctuated::Punctuated,
	Ident,
	LitBool,
	Token,
};

use super::Abi;

#[derive(Debug)]
pub struct AttributeOptions {
	pub repr: Abi,
	pub relax_abi: bool,
	pub drop: Option<Abi>,
	pub embed_layout: bool,
	pub vtable_name: Option<Ident>,
}

impl Parse for AttributeOptions {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		enum AttrOption {
			Repr(Abi),
			RelaxAbi(bool),
			Drop(Option<Abi>),
			EmbedLayout(bool),
			VTableName(Ident),
		}

		struct SpannedAttrOption(Span, AttrOption);

		impl Parse for SpannedAttrOption {
			fn parse(input: ParseStream) -> syn::Result<Self> {
				let option_name = input.parse::<Ident>()?;
				let _ = input.parse::<Token![=]>()?;

				Ok(Self(
					option_name.span(),
					match &option_name.to_string() as &str {
						"repr" => AttrOption::Repr(Abi::parse_struct_repr(input)?),
						"relax_abi" => AttrOption::RelaxAbi(input.parse::<LitBool>()?.value),
						"drop" => AttrOption::Drop({
							let abi = input.parse::<Ident>()?;

							match &abi.to_string() as &str {
								"none" => None,
								_ => Some(Abi::Explicit(abi)),
							}
						}),
						"embed_layout" => AttrOption::EmbedLayout(input.parse::<LitBool>()?.value),
						"vtable" => AttrOption::VTableName(input.parse::<Ident>()?),
						_ => {
							return Err(syn::Error::new(
								option_name.span(),
								&format!("Unknown option '{}'", option_name.to_string()),
							))
						},
					},
				))
			}
		}

		let options = Punctuated::<SpannedAttrOption, Token![,]>::parse_terminated(input)?;

		struct OptionalOptions {
			repr: Option<Abi>,
			relax_abi: Option<bool>,
			drop: Option<Option<Abi>>,
			embed_layout: Option<bool>,
			vtable_name: Option<Ident>,
		}

		let mut option_struct = OptionalOptions {
			repr: None,
			relax_abi: None,
			drop: None,
			embed_layout: None,
			vtable_name: None,
		};

		for SpannedAttrOption(span, option) in options {
			let duplicate = match option {
				AttrOption::Repr(x) => matches!(option_struct.repr.replace(x), Some(_)),
				AttrOption::RelaxAbi(x) => matches!(option_struct.relax_abi.replace(x), Some(_)),
				AttrOption::Drop(x) => matches!(option_struct.drop.replace(x), Some(_)),
				AttrOption::EmbedLayout(x) => matches!(option_struct.embed_layout.replace(x), Some(_)),
				AttrOption::VTableName(x) => {
					matches!(option_struct.vtable_name.replace(x), Some(_))
				},
			};

			if duplicate {
				return Err(syn::Error::new(span, "option can only be defined once"))
			}
		}

		Ok(Self {
			repr: option_struct.repr.unwrap_or(Abi::new_explicit_c()),
			relax_abi: option_struct.relax_abi.unwrap_or(false),
			drop: option_struct.drop.unwrap_or(Some(Abi::new_explicit_c())),
			embed_layout: option_struct.embed_layout.unwrap_or(true),
			vtable_name: option_struct.vtable_name,
		})
	}
}
