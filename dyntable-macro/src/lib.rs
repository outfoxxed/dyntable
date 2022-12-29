mod codegen;
mod parse;

#[proc_macro_attribute]
pub fn dyntable(
	attr: proc_macro::TokenStream,
	item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	match parse::DynTraitInfo::parse_trait(attr, item) {
		Ok(info) => codegen::codegen(&info),
		Err(err) => err.into_compile_error(),
	}
	.into()
}
