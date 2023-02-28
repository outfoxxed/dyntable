use proc_macro2::TokenStream;
use quote::{format_ident, ToTokens};
use syn::{punctuated::Punctuated, GenericParam, LifetimeDef, Path, Token, Type, TypeParam};

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
			generics,
		},
		drop: drop_abi,
		embed_layout,
		..
	}: &DynTraitInfo,
) -> TokenStream {
	let (impl_generics, _, where_clause) = generics.split_for_impl();
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
		let generics = generics
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
		#[allow(non_snake_case, non_camel_case_types)]
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
	let inputs = inputs.iter().map(|MethodParam { ty, .. }| {
		let mut ty = ty.clone();
		visit_type_paths(&mut ty, &mut fix_vtable_associated_types);
		ty
	});

	let output = match output {
		syn::ReturnType::Default => syn::ReturnType::Default,
		syn::ReturnType::Type(arrow, ty) => syn::ReturnType::Type(*arrow, {
			let mut ty = ty.clone();
			visit_type_paths(&mut ty, &mut fix_vtable_associated_types);
			ty
		}),
	};

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
		) #output
	}
}

pub fn visit_type_paths(ty: &mut Type, visit: &mut impl FnMut(&mut Path)) {
	use syn::{
		ReturnType,
		TraitBound,
		TypeArray,
		TypeBareFn,
		TypeGroup,
		TypeImplTrait,
		TypeParamBound,
		TypeParen,
		TypePath,
		TypePtr,
		TypeReference,
		TypeSlice,
		TypeTraitObject,
		TypeTuple,
	};

	match ty {
		Type::Array(TypeArray { elem, .. }) => visit_type_paths(elem, visit),
		Type::BareFn(TypeBareFn { inputs, output, .. }) => {
			for input in inputs {
				visit_type_paths(&mut input.ty, visit)
			}

			if let ReturnType::Type(_, ty) = output {
				visit_type_paths(ty, visit)
			}
		},
		Type::Group(TypeGroup { elem, .. }) => visit_type_paths(elem, visit),
		Type::ImplTrait(TypeImplTrait { bounds, .. }) => {
			for bound in bounds {
				if let TypeParamBound::Trait(TraitBound { path, .. }) = bound {
					visit(path);
				}
			}
		},
		Type::Macro(_) => {}, // macro args are not checked
		Type::Paren(TypeParen { elem, .. }) => visit_type_paths(elem, visit),
		Type::Path(TypePath { path, .. }) => visit(path),
		Type::Ptr(TypePtr { elem, .. }) => visit_type_paths(elem, visit),
		Type::Reference(TypeReference { elem, .. }) => visit_type_paths(elem, visit),
		Type::Slice(TypeSlice { elem, .. }) => visit_type_paths(elem, visit),
		Type::TraitObject(TypeTraitObject { bounds, .. }) => {
			for bound in bounds {
				if let TypeParamBound::Trait(TraitBound { path, .. }) = bound {
					visit(path);
				}
			}
		},
		Type::Tuple(TypeTuple { elems, .. }) => {
			for elem in elems {
				visit_type_paths(elem, visit);
			}
		},
		_ => {},
	}
}

pub fn fix_vtable_associated_types(path: &mut Path) {
	if path.leading_colon.is_some() {
		return
	}

	let mut iter = path.segments.iter_mut();
	let Some(self_tok) = iter.next() else { return };
	if self_tok.ident != "Self" || !self_tok.arguments.is_empty() {
		return
	}

	let Some(associated) = iter.next() else { return };
	if !associated.arguments.is_empty() {
		return
	}

	*path = Path::from(format_ident!("__DynAssociated_{}", associated.ident));
}
