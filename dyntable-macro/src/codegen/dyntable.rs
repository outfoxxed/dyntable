use proc_macro2::TokenStream;
use quote::format_ident;
use syn::{Signature, Token};

use crate::parse::{DynTraitInfo, VTableEntry};

pub fn impl_dyntable(dyntrait: &DynTraitInfo) -> TokenStream {
	let proxy_trait = format_ident!("__DynTable_{}", dyntrait.dyntrait.ident);
	let ident = &dyntrait.dyntrait.ident;
	let vtable_ident = &dyntrait.vtable.name;
	let (_, ty_generics, where_clause) = dyntrait.generics.split_for_impl();
	let impl_generics = dyntrait.generics.params.clone().into_iter()
    .collect::<Vec<_>>();
	let where_entries = where_clause
    .into_iter()
    .flat_map(|clause| &clause.predicates);

	let vtable_entries = dyntrait.entries.iter()
    .map(|entry| match entry {
			VTableEntry::Subtable(subtable) => {
				let entry_name = &subtable.ident;
				quote::quote! {
					#entry_name: <__DynTarget as ::dyntable::DynTable<
						<(dyn #ident #ty_generics + 'static) as ::dyntable::VTableRepr>::VTable,
					>>::VTABLE
				}
			},
			VTableEntry::Method(method) => {
				let Signature {
					unsafety,
					abi,
					fn_token,
					ident,
					inputs,
					variadic,
					output,
					..
				} = &method.sig;

				let inputs = inputs.iter()
					.map(|_| <Token![_]>::default());

				let output = match output {
					syn::ReturnType::Default => None,
					syn::ReturnType::Type(..) => Some(<Token![_]>::default()),
				}.into_iter();

				quote::quote! {
					#ident: unsafe {
						::core::intrinsics::transmute(
							__DynTarget::#ident as
								#unsafety #abi #fn_token (
									#(#inputs,)* #variadic
								) #( -> #output)*
						)
					}
				}
			},
		});

	quote::quote! {
		#[allow(non_camel_case_types)]
		unsafe trait #proxy_trait<'v, V: 'v + ::dyntable::VTable> {
			const VTABLE: V;
			const STATIC_VTABLE: &'v V;
		}

		unsafe impl<
			'__dyn_vtable,
			#(#impl_generics + '__dyn_vtable,)*
			__DynTarget,
		> ::dyntable::__private::DynTable2<'__dyn_vtable, #vtable_ident #ty_generics>
		for ::dyntable::__private::DynImplTarget<__DynTarget, #vtable_ident #ty_generics>
		where
			#(#where_entries,)*
			__DynTarget: #proxy_trait<'__dyn_vtable, #vtable_ident #ty_generics>,
		{
			const VTABLE: #vtable_ident #ty_generics = __DynTarget::VTABLE;
			const STATIC_VTABLE: &'__dyn_vtable #vtable_ident #ty_generics = __DynTarget::STATIC_VTABLE;
		}

		unsafe impl<
			'__dyn_vtable,
			#(#impl_generics + '__dyn_vtable,)*
			__DynTarget: #ident #ty_generics,
		> #proxy_trait<'__dyn_vtable, #vtable_ident #ty_generics>
		for __DynTarget
		#where_clause {
			const STATIC_VTABLE: &'__dyn_vtable #vtable_ident #ty_generics =
				&<Self as #proxy_trait<'__dyn_vtable, #vtable_ident #ty_generics>>::VTABLE;
			const VTABLE: #vtable_ident #ty_generics = #vtable_ident {
				#(#vtable_entries,)*
			};
		}
	}
}
