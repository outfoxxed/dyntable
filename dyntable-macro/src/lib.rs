use parse::{AttributeOptions, DynTraitBody};
use quote::ToTokens;
use syn::parse_macro_input;

mod codegen;
mod parse;

#[proc_macro_attribute]
pub fn dyntable(
	attr: proc_macro::TokenStream,
	item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	let attribute_options = parse_macro_input!(attr as AttributeOptions);
	let trait_body = parse_macro_input!(item as DynTraitBody);

	dbg!(attribute_options, &trait_body);

	let vtable = codegen::vtable::build_vtable(&trait_body).unwrap();
	dbg!(vtable.to_token_stream().to_string());

	todo!()
}
