use quote::ToTokens;

mod codegen;
mod parse;

#[proc_macro_attribute]
pub fn dyntable(
	attr: proc_macro::TokenStream,
	item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	let info = parse::DynTraitInfo::parse_trait(attr, item).unwrap();

	dbg!(&info);

	let vtable = codegen::vtable::build_vtable(&info).unwrap();
	dbg!(vtable.to_token_stream().to_string());

	todo!()
}
