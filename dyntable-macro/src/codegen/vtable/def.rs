use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{punctuated::Punctuated, GenericParam, LifetimeDef, Token, TypeParam};

use crate::parse::{
	DynTraitInfo,
	MethodEntry,
	MethodParam,
	MethodReceiver,
	ReceiverReference,
	Subtable,
	SubtableEntry,
	VTableEntry,
	VTableInfo,
};

pub fn gen_vtable(
	dyntrait @ DynTraitInfo {
		vis,
		vtable: VTableInfo {
			repr,
			name: vtable_ident,
		},
		generics: trait_generics,
		drop: drop_abi,
		embed_layout,
		..
	}: &DynTraitInfo,
) -> TokenStream {
	let (impl_generics, _, where_clause) = trait_generics.split_for_impl();
	let repr = repr.as_repr();

	let drop_abi = drop_abi.as_ref().map(|abi| abi.as_abi()).into_iter();

	let entries = dyntrait
		.entries
		.iter()
		.map(|entry| gen_vtable_entry(dyntrait, entry));

	let embed_layout = match embed_layout {
		true => Some(TokenStream::new()),
		false => None,
	}
	.into_iter()
	.collect::<Vec<_>>();

	let vtable_phantom_generics = {
		let generics = trait_generics
			.params
			.iter()
			.filter_map(|entry| match entry {
				GenericParam::Lifetime(LifetimeDef { lifetime, .. }) => {
					Some(quote::quote! { &#lifetime () })
				},
				GenericParam::Type(TypeParam { ident, .. }) => Some(ident.to_token_stream()),
				GenericParam::Const(_) => None,
			})
			.collect::<Vec<_>>();

		match generics.len() {
			1 => quote::quote! { #(#generics),* },
			// 0 OR more than 1 (0 params = `()`)
			_ => quote::quote! { (#(#generics),*) },
		}
	};

	quote::quote! {
		#[allow(non_snake_case)]
		#repr
		#vis struct #vtable_ident #impl_generics
		#where_clause {
			#(#vis __drop: unsafe #drop_abi fn(*mut ::core::ffi::c_void),)*
			// embed_layout is a marker and generates no code
			#(#vis __layout: ::dyntable::alloc::MemoryLayout, #embed_layout)*
			#(#entries,)*
			#vis __generics: ::core::marker::PhantomData<#vtable_phantom_generics>,
		}
	}
}

fn gen_vtable_entry(dyntrait: &DynTraitInfo, entry: &VTableEntry) -> TokenStream {
	match entry {
		VTableEntry::Subtable(SubtableEntry {
			ident,
			subtable: Subtable { path, .. },
		}) => quote::quote! {
			#ident: <(dyn #path + 'static) as ::dyntable::VTableRepr>::VTable
		},
		VTableEntry::Method(method) => gen_vtable_method(dyntrait, method),
	}
}

fn gen_vtable_method(
	DynTraitInfo { vis, .. }: &DynTraitInfo,
	MethodEntry {
		unsafety,
		abi,
		fn_token,
		ident,
		generics,
		receiver,
		inputs,
		output,
	}: &MethodEntry,
) -> TokenStream {
	let inputs = inputs.iter().map(
		|MethodParam { ty, .. }| ty, /*strip_references(ty.clone())*/
	);

	let output = match output {
		syn::ReturnType::Default => None,
		syn::ReturnType::Type(_, ty) => Some(ty /*strip_references(ty.as_ref().clone())*/),
	}
	.into_iter();

	let self_ptr = match receiver {
		MethodReceiver::Reference(ReceiverReference {
			reference: (_, lt), ..
		}) => {
			let lt = lt.into_iter();
			quote::quote! { ::dyntable::DynSelf #(<#lt>)* }
		},
		MethodReceiver::Value(_) => {
			quote::quote! { *mut ::core::ffi::c_void }
		},
	};

	let declared_lifetimes = generics
		.params
		.iter()
		.filter_map(|param| match param {
			GenericParam::Type(_) => None,
			GenericParam::Const(_) => None,
			// Currently, lifetime bounds cannot be expressed with relation to
			// another bound in a for clause. This means it is not possible to
			// express bounds with relation to trait lifetime parameters as
			// generic parameters in the for clause. It is unclear if this is an
			// issue.
			GenericParam::Lifetime(lt) => Some(lt),
		})
		.collect::<Punctuated<_, Token![,]>>();

	let for_tok = match declared_lifetimes.is_empty() {
		true => TokenStream::new(),
		false => quote::quote! { for<#declared_lifetimes> },
	};

	quote::quote! {
		#vis #ident: #for_tok #unsafety #abi #fn_token (
			#self_ptr,
			#(#inputs),*
		) #( -> #output)*
	}
}
