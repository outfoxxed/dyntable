use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{GenericParam, LifetimeDef, Type, TypeParam, TypePtr, TypeReference};

use crate::parse::{
	DynTraitInfo,
	MethodEntry,
	MethodParam,
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
	let (_, ty_generics, where_clause) = trait_generics.split_for_impl();
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
		#vis struct #vtable_ident #ty_generics
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
		receiver,
		inputs,
		output,
		..
	}: &MethodEntry,
) -> TokenStream {
	let self_ptr_type = receiver.pointer_type();

	let inputs = inputs
		.iter()
		.map(|MethodParam { ty, .. }| strip_references(ty.clone()));

	let output = match output {
		syn::ReturnType::Default => None,
		syn::ReturnType::Type(_, ty) => Some(strip_references(ty.as_ref().clone())),
	}
	.into_iter();

	quote::quote! {
		#vis #ident: #unsafety #abi #fn_token (
			*#self_ptr_type ::core::ffi::c_void,
			#(#inputs),*
		) #( -> #output)*
	}
}

/// Replace toplevel references in a [`Type`] with raw pointers
// TODO: reassess how nessesary this is and if it could be a
// source of UB. At this point the main goal is to copy the old
// macro's functionality (toplevel references to pointers).
fn strip_references(ty: Type) -> Type {
	match ty {
		Type::Reference(TypeReference {
			mutability, elem, ..
		}) => Type::Ptr(TypePtr {
			star_token: Default::default(),
			const_token: match &mutability {
				Some(_) => None,
				None => Some(Default::default()),
			},
			mutability,
			// TODO: add tests to check if nested references need
			// to be removed (if they need to be removed at all, see above todo)
			elem,
		}),
		other => other,
	}
}
