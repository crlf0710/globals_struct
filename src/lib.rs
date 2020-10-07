extern crate proc_macro;
use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::spanned::Spanned;
use quote::quote;

#[proc_macro_attribute]
pub fn globals_struct_field(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_attribute]
pub fn globals_struct(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let syn::ItemMod {
        vis: mod_vis,
        ident: mod_name,
        content: mod_content,
        ..
    } = parse_macro_input!(item as syn::ItemMod);
    let mod_items = match mod_content {
        None => {
            return syn::Error::new(
                syn::export::Span::call_site(),
                "Module content should be provided.",
            )
            .to_compile_error()
            .into();
        }
        Some((_, contents)) => contents,
    };
    let mut field_names = vec![];
    let mut field_tys = vec![];
    let mut field_exprs = vec![];
    for item in mod_items {
        let item_span = item.span();
        if let syn::Item::Static(syn::ItemStatic {
            attrs: static_attrs,
            ident: static_name,
            ty: static_ty,
            expr: static_initializer,
            ..
        }) = item
        {
            let target = match globals_struct_field_attr_target(&static_attrs, item_span) {
                Ok(v) => v,
                Err(e) => {
                    return e.to_compile_error().into();
                }
            };
            if let Some(target) = target {
                if target == mod_name {
                    field_names.push(static_name);
                    field_tys.push(static_ty);
                    field_exprs.push(static_initializer);
                }
            }
        }
    }

    let ts = quote!{
        #mod_vis struct #mod_name {
            #(#field_names : #field_tys ,)*
        }

        impl ::core::default::Default for #mod_name {
            fn default() -> Self {
                #mod_name {
                    #(#field_names : #field_exprs ,)*
                }
            }
        }
    };
    ts.into()
}

fn globals_struct_field_attr_target(
    attrs: &[syn::Attribute],
    span: syn::export::Span,
) -> syn::Result<Option<syn::Ident>> {
    let mut found_attr = None;
    for attr in attrs {
        if let Some(attr_ident) = get_path_last_ident(&attr.path) {
            if *attr_ident == "globals_struct_field" {
                if found_attr.is_some() {
                    return Err(syn::Error::new(
                        span,
                        "Attribute `globals_struct_field` should not be specified more than once on an item.",
                    ));
                }
                found_attr = Some(attr);
            }
        }
    }
    let found_attr = if let Some(attr) = found_attr {
        attr
    } else {
        return Ok(None);
    };
    let meta = found_attr.parse_meta()?;
    let target_path = match get_meta_sole_path(&meta) {
        Some(path) => path,
        None => {
            return Err(syn::Error::new(
                span,
                "Attribute `globals_struct_field` should be specified in the form #[globals_struct_field(Globals)].",
            ));
        }
    };
    Ok(get_path_last_ident(target_path).cloned())
}

fn get_meta_sole_path(meta: &syn::Meta) -> Option<&syn::Path> {
    let list = if let syn::Meta::List(list) = meta {
        list
    } else {
        return None;
    };
    if list.nested.len() != 1 {
        return None;
    }
    if let syn::NestedMeta::Meta(syn::Meta::Path(inner_path)) = &list.nested[0] {
        Some(inner_path)
    } else {
        None
    }
}

fn get_path_last_ident(path: &syn::Path) -> Option<&syn::Ident> {
    let last_segment = path.segments.last()?;
    if let syn::PathArguments::None = last_segment.arguments {
        Some(&last_segment.ident)
    } else {
        None
    }
}