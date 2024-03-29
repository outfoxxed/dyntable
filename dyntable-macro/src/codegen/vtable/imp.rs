use proc_macro2::{Span, TokenStream};
use quote::{format_ident, ToTokens};
use syn::{
	ConstParam,
	GenericParam,
	Lifetime,
	LifetimeParam,
	ReturnType,
	Token,
	TypeParam,
	TypeParamBound,
};

use super::def::{fix_vtable_associated_types, visit_type_paths};
use crate::parse::{
	DynTraitInfo,
	MethodEntry,
	MethodParam,
	MethodReceiver,
	Subtable,
	SubtableEntry,
	TopLevelSubtable,
	TraitInfo,
	VTableEntry,
	VTableInfo,
};

pub fn gen_impl(
	dyntrait @ DynTraitInfo {
		dyntrait: TraitInfo {
			ident,
			vtable_ty_generics: trait_ty_generics,
			..
		},
		vtable: VTableInfo {
			name: vtable_ident,
			generics: vtable_generics,
			..
		},
		drop: drop_abi,
		embed_layout,
		..
	}: &DynTraitInfo,
) -> TokenStream {
	let proxy_trait = format_ident!("__DynTrait_{}", dyntrait.dyntrait.ident);
	let (impl_generics, ty_generics, where_clause) = vtable_generics.split_for_impl();

	let impl_vt_generic_entries = vtable_generics
		.params
		.clone()
		.into_iter()
		.map(|mut param| {
			match &mut param {
				GenericParam::Lifetime(param) => {
					param.colon_token.get_or_insert_with(Default::default);
					param
						.bounds
						.insert(0, Lifetime::new("'__dyn_vtable", Span::call_site()));
				},
				GenericParam::Type(param) => {
					param.eq_token = None;
					param.default = None;
					param.colon_token.get_or_insert_with(Default::default);
					param.bounds.insert(
						0,
						TypeParamBound::Lifetime(Lifetime::new("'__dyn_vtable", Span::call_site())),
					);
				},
				_ => {},
			};
			param
		})
		.collect::<Vec<_>>();

	let where_predicates = where_clause
		.into_iter()
		.flat_map(|clause| &clause.predicates)
		.collect::<Vec<_>>();

	let entries = dyntrait.entries.iter().map(|entry| match entry {
		VTableEntry::Subtable(SubtableEntry {
			ident,
			subtable: TopLevelSubtable {
				ref_token,
				subtable: Subtable { path, .. },
			},
		}) => match ref_token {
			Some(_) => quote::quote! {
				#ident: <Self as ::dyntable::DynTrait<
					<(dyn #path + 'static) as ::dyntable::VTableRepr>::VTable,
				>>::STATIC_VTABLE
			},
			None => quote::quote! {
				#ident: <Self as ::dyntable::DynTrait<
					<(dyn #path + 'static) as ::dyntable::VTableRepr>::VTable,
				>>::VTABLE
			},
		},
		VTableEntry::Method(method) => gen_method_entry(dyntrait, method),
	});

	let (drop_marker, drop_abi) = match drop_abi.as_ref() {
		Some(drop_abi) => (
			Some(TokenStream::new()).into_iter(),
			Some(drop_abi.as_abi()).into_iter(),
		),
		None => (None.into_iter(), None.into_iter()),
	};

	let embed_layout = match embed_layout {
		true => Some(TokenStream::new()),
		false => None,
	}
	.into_iter()
	.collect::<Vec<_>>();

	quote::quote! {
		#[allow(non_camel_case_types)]
		unsafe impl<
			'__dyn_vtable,
			#(#impl_vt_generic_entries,)*
			__DynTarget,
		> #proxy_trait<'__dyn_vtable, #vtable_ident #ty_generics>
		for __DynTarget
		where
			#(#where_predicates,)*
			__DynTarget: #ident #trait_ty_generics,
		{
			const STATIC_VTABLE: &'__dyn_vtable #vtable_ident #ty_generics =
				&<Self as #proxy_trait<'__dyn_vtable, #vtable_ident #ty_generics>>::VTABLE;
			const VTABLE: #vtable_ident #ty_generics = #vtable_ident {
				#(__drop: {
					unsafe #drop_abi fn thunk<T>(ptr: *mut ::core::ffi::c_void) {
						::core::ptr::drop_in_place(ptr as *mut T)
					}

					thunk::<Self>
				},)*
				#(__layout: ::dyntable::alloc::MemoryLayout::new::<Self>(), #embed_layout)* // embed_layout is a marker
				#(#entries,)*
				__generics: ::core::marker::PhantomData,
			};
		}

		// drop implementation
		#(#drop_marker // marker, no code generated
			#[allow(non_camel_case_types)]
			unsafe impl #impl_generics ::dyntable::AssociatedDrop
			for #vtable_ident #ty_generics
			#where_clause {
				#[inline(always)]
				unsafe fn virtual_drop(&self, instance: *mut ::core::ffi::c_void) {
					(self.__drop)(instance)
				}
			}
		)*

		#(#embed_layout // marker, no code generated
			#[allow(non_camel_case_types)]
			unsafe impl #impl_generics ::dyntable::AssociatedLayout
			for #vtable_ident #ty_generics
			#where_clause {
				#[inline(always)]
				fn virtual_layout(&self) -> ::dyntable::alloc::MemoryLayout {
					self.__layout
				}
			}
		)*
	}
}

/// Generate a vtable entry for a solid base type
fn gen_method_entry(
	DynTraitInfo {
		dyntrait:
			TraitInfo {
				ident,
				generics: trait_generics,
				vtable_ty_generics: trait_vt_generics,
				..
			},
		vtable: VTableInfo {
			generics: vtable_generics,
			..
		},
		..
	}: &DynTraitInfo,
	MethodEntry {
		unsafety,
		abi,
		fn_token,
		ident: fn_ident,
		generics,
		receiver,
		inputs,
		output,
	}: &MethodEntry,
) -> TokenStream {
	let output = match output {
		ReturnType::Default => ReturnType::Default,
		ReturnType::Type(arrow, ty) => ReturnType::Type(*arrow, {
			let mut ty = ty.clone();
			visit_type_paths(&mut ty, &mut fix_vtable_associated_types);
			ty
		}),
	};

	let inputs = inputs
		.iter()
		.map(
			|MethodParam {
			     ident,
			     colon_token,
			     ty,
			 }| {
				let mut ty = ty.clone();
				visit_type_paths(&mut ty, &mut fix_vtable_associated_types);

				MethodParam {
					ident: ident.clone(),
					colon_token: colon_token.clone(),
					ty,
				}
			},
		)
		.collect::<Vec<_>>();

	let fn_path = match receiver {
		MethodReceiver::Reference(_) => quote::quote! { Self::#fn_ident },
		MethodReceiver::Value(_) => {
			// functions that take self by value need a proxy thunk to
			// convert from a pointer to an owned Self

			let call_generics = vtable_generics
				.params
				.clone()
				.into_iter()
				.map(|param| match param {
					GenericParam::Type(TypeParam { ident, .. }) => ident.to_token_stream(),
					GenericParam::Lifetime(LifetimeParam { lifetime, .. }) => {
						lifetime.to_token_stream()
					},
					GenericParam::Const(ConstParam { ident, .. }) => ident.to_token_stream(),
				})
				.collect::<Vec<_>>();

			let (_, ty_generics, _) = trait_generics.split_for_impl();
			let (_, _, fn_where_clause) = generics.split_for_impl();
			let param_list = MethodParam::params_safe(inputs.iter());
			let arg_list = MethodParam::idents_safe(inputs.iter());

			let impl_generic_entries = vtable_generics
				.params
				.clone()
				.into_iter()
				.collect::<Vec<_>>();

			quote::quote! {{
				#unsafety #abi #fn_token thunk <
					#(#impl_generic_entries,)*
					__DynSelf: #ident #trait_vt_generics,
				> (__dyn_self: *mut __DynSelf, #(#param_list),*) #output
				#fn_where_clause {
					<__DynSelf as #ident #ty_generics>::#fn_ident(
						unsafe { __dyn_self.read() },
						#(#arg_list),*
					)
				}

				thunk::<#(#call_generics,)* Self>
			}}
		},
	};

	let inputs = inputs.iter().map(|_| <Token![_]>::default());

	let output = match output {
		syn::ReturnType::Default => None,
		syn::ReturnType::Type(..) => Some(<Token![_]>::default()),
	}
	.into_iter();

	quote::quote! {
		#fn_ident: unsafe {
			::core::mem::transmute(
				#fn_path as
					#unsafety #abi #fn_token (
						_,
						#(#inputs),*
					) #( -> #output)*
			)
		}
	}
}
