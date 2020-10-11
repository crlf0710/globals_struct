extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;
use syn::spanned::Spanned;

#[proc_macro_attribute]
pub fn globals_struct_field(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_attribute]
pub fn globals_struct_field_view(_attr: TokenStream, item: TokenStream) -> TokenStream {
    if syn::parse_macro_input::parse::<syn::ItemMod>(item).is_ok() {
        return syn::Error::new(
            syn::export::Span::call_site(),
            "Attribute `globals_struct_field_view` must occur after `globals_struct` on modules!",
        )
        .to_compile_error()
        .into();
    }
    TokenStream::new()
}

#[proc_macro_attribute]
pub fn globals_struct_use(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_attribute]
pub fn globals_struct(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let syn::ItemMod {
        attrs: mod_attrs,
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
    let target_views = match globals_struct_attr_multiple_targets_and_values(
        &mod_attrs,
        syn::export::Span::call_site(),
        "globals_struct_field_view",
    ) {
        Err(e) => {
            return e.to_compile_error().into();
        }
        Ok(v) => v,
    };
    let mut field_vises = vec![];
    let mut field_names = vec![];
    let mut field_tys = vec![];
    let mut field_exprs = vec![];
    let mut field_views = vec![];
    let mut use_vises = vec![];
    let mut use_leading_colons = vec![];
    let mut use_usetrees = vec![];
    if let Err(e) = recursive_process_items(
        &mod_name,
        &mod_items,
        (
            &mut field_vises,
            &mut field_names,
            &mut field_tys,
            &mut field_exprs,
            &mut field_views,
        ),
        (&mut use_vises, &mut use_leading_colons, &mut use_usetrees),
    ) {
        return e.to_compile_error().into();
    }

    let mut ts = quote! {
        #mod_vis struct #mod_name {
            #(#field_vises #field_names : #field_tys ,)*
        }

        impl ::core::default::Default for #mod_name {
            fn default() -> Self {
                #mod_name {
                    #(#field_names : #field_exprs ,)*
                }
            }
        }

        #(#use_vises use #use_leading_colons #use_usetrees;)*
    };
    let field_count = field_names.len();
    for (view_name, ctor_name) in target_views {
        let mut view_field_vises = vec![];
        let mut view_field_names = vec![];
        let mut view_field_tys = vec![];
        for field_idx in 0..field_count {
            if field_views[field_idx].iter().all(|view| *view != view_name) {
                continue;
            }
            view_field_vises.push(field_vises[field_idx].clone());
            view_field_names.push(field_names[field_idx].clone());
            view_field_tys.push(field_tys[field_idx].clone());
        }
        ts.extend(Some(quote! {
            #mod_vis struct #view_name<'view> {
                #(#view_field_vises #view_field_names : &'view mut #view_field_tys ,)*
                #[doc(hidden)]
                pub __dummy__ : ::core::marker::PhantomData<&'view mut ()>
            }
            macro_rules! #ctor_name {
                ($globals:expr) => {
                    #view_name {
                        #(#view_field_names : &mut $globals . #view_field_names ,)*
                        __dummy__ : ::core::marker::PhantomData
                    }
                }
            }
        }));
    }
    ts.into()
}

fn recursive_process_items(
    mod_name: &syn::Ident,
    mod_items: &[syn::Item],
    (field_vises, field_names, field_tys, field_exprs, field_views): (
        &mut Vec<syn::Visibility>,
        &mut Vec<syn::Ident>,
        &mut Vec<Box<syn::Type>>,
        &mut Vec<Box<syn::Expr>>,
        &mut Vec<Vec<syn::Ident>>,
    ),
    (use_vises, use_leading_colons, use_usetrees): (
        &mut Vec<syn::Visibility>,
        &mut Vec<Option<syn::Token![::]>>,
        &mut Vec<syn::UseTree>,
    ),
) -> syn::Result<()> {
    for item in mod_items {
        let item_span = item.span();
        if let syn::Item::Static(syn::ItemStatic {
            vis: static_vis,
            attrs: static_attrs,
            ident: static_name,
            ty: static_ty,
            expr: static_initializer,
            ..
        }) = item
        {
            let target =
                globals_struct_attr_target(&static_attrs, item_span, "globals_struct_field")?;
            if let Some(target) = target {
                if target == *mod_name {
                    let target_views = globals_struct_attr_multiple_targets(
                        &static_attrs,
                        item_span,
                        "globals_struct_field_view",
                    )?;

                    field_vises.push(static_vis.clone());
                    field_names.push(static_name.clone());
                    field_tys.push(static_ty.clone());
                    field_exprs.push(static_initializer.clone());
                    field_views.push(target_views);
                }
            }
        } else if let syn::Item::Use(syn::ItemUse {
            vis: use_vis,
            attrs: use_attrs,
            leading_colon: use_leading_colon,
            tree: use_usetree,
            ..
        }) = item
        {
            let target = globals_struct_attr_target(&use_attrs, item_span, "globals_struct_use")?;
            if let Some(target) = target {
                if target == *mod_name {
                    if use_usetrees.iter().all(|existing| existing != use_usetree) {
                        use_vises.push(use_vis.clone());
                        use_leading_colons.push(use_leading_colon.clone());
                        use_usetrees.push(use_usetree.clone());
                    }
                }
            }
        } else if let syn::Item::Macro(syn::ItemMacro { mac, .. }) = item {
            let mac_span = mac.span();
            if mac
                .path
                .get_ident()
                .map(|v| *v == "include")
                .unwrap_or(false)
            {
                let filepath = mac.parse_body::<syn::LitStr>()?;
                let file_content = std::fs::read_to_string(filepath.value())
                    .map_err(|e| syn::Error::new(mac_span, e))?;
                let file = syn::parse_file(&file_content)?;
                recursive_process_items(
                    mod_name,
                    &file.items,
                    (
                        field_vises,
                        field_names,
                        field_tys,
                        field_exprs,
                        field_views,
                    ),
                    (use_vises, use_leading_colons, use_usetrees),
                )?;
            }
        }
    }
    Ok(())
}

fn globals_struct_attr_target(
    attrs: &[syn::Attribute],
    span: syn::export::Span,
    expected_attr_name: &'static str,
) -> syn::Result<Option<syn::Ident>> {
    let mut found_attr = None;
    for attr in attrs {
        if let Some(attr_ident) = get_path_last_ident(&attr.path) {
            if *attr_ident != expected_attr_name {
                continue;
            }
            if found_attr.is_some() {
                return Err(syn::Error::new(
                    span,
                    "Attribute `globals_struct_field` or `globals_struct_use` should not be specified more than once on an item.",
                ));
            }
            found_attr = Some(attr);
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
                "Attribute `globals_struct_field` or `globals_struct_use` should be specified in the form #[globals_struct_field(Globals)] or #[globals_struct_use(Globals)].",
            ));
        }
    };
    Ok(get_path_last_ident(target_path).cloned())
}

fn globals_struct_attr_multiple_targets(
    attrs: &[syn::Attribute],
    span: syn::export::Span,
    expected_attr_name: &'static str,
) -> syn::Result<Vec<syn::Ident>> {
    let mut found_targets = vec![];
    for attr in attrs {
        if let Some(attr_ident) = get_path_last_ident(&attr.path) {
            if *attr_ident != expected_attr_name {
                continue;
            }
            let meta = attr.parse_meta()?;
            let target_path = match get_meta_sole_path(&meta) {
                Some(path) => path,
                None => {
                    return Err(syn::Error::new(
                        span,
                        "Attribute `globals_struct_field_view` should be specified in the form #[globals_struct_field_view(GlobalsView)].",
                    ));
                }
            };
            if let Some(target_path) = get_path_last_ident(target_path) {
                found_targets.push(target_path.clone());
            }
        }
    }
    Ok(found_targets)
}

fn globals_struct_attr_multiple_targets_and_values(
    attrs: &[syn::Attribute],
    span: syn::export::Span,
    expected_attr_name: &'static str,
) -> syn::Result<Vec<(syn::Ident, syn::Ident)>> {
    let mut found_targets = vec![];
    for attr in attrs {
        if let Some(attr_ident) = get_path_last_ident(&attr.path) {
            if *attr_ident != expected_attr_name {
                continue;
            }
            let meta = attr.parse_meta()?;
            let (target_path, target_value) = match get_meta_sole_path_and_value(&meta) {
                Some(path_and_value) => path_and_value,
                None => {
                    return Err(syn::Error::new(
                        span,
                        "Attribute `globals_struct_field_view` should be specified in the form #[globals_struct_field_view(GlobalsView, make_globals_view)].",
                    ));
                }
            };
            if let Some(target_path) = get_path_last_ident(target_path) {
                if let Some(target_value) = get_path_last_ident(target_value) {
                    found_targets.push((target_path.clone(), target_value.clone()));
                }
            }
        }
    }
    Ok(found_targets)
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

fn get_meta_sole_path_and_value(meta: &syn::Meta) -> Option<(&syn::Path, &syn::Path)> {
    let list = if let syn::Meta::List(list) = meta {
        list
    } else {
        return None;
    };
    if list.nested.len() != 2 {
        return None;
    }
    if let syn::NestedMeta::Meta(syn::Meta::Path(inner_path)) = &list.nested[0] {
        if let syn::NestedMeta::Meta(syn::Meta::Path(inner_path2)) = &list.nested[1] {
            Some((inner_path, inner_path2))
        } else {
            None
        }
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
