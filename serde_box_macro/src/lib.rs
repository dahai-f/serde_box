use proc_macro::TokenStream;
use std::collections::hash_map;
use std::hash::Hasher;

use proc_macro2::Span;
use quote::*;
use syn::spanned::Spanned;
use syn::*;

#[proc_macro_attribute]
pub fn serde_box(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as Item);
    match item {
        Item::Impl(item_impl) => {
            if let Some((None, trait_path, _for)) = &item_impl.trait_ {
                let struct_type = &item_impl.self_ty;
                let register_serde_box = Ident::new(
                    &format!(
                        "register_serde_box_{}",
                        hash(&(quote! {#trait_path #struct_type}).into())
                    ),
                    Span::call_site(),
                );

                let output = quote! {
                    #item_impl

                    #[ctor]
                    fn #register_serde_box() {
                        let trait_ptr = std::ptr::null::<#struct_type>() as *const dyn #trait_path;
                        let trait_obj: metatype::TraitObject = type_coerce(metatype::Type::meta(trait_ptr));
                        let vtable = trait_obj.vtable;
                        let registry = <(dyn #trait_path) as SerdeBoxRegistry>::get_registry();
                        registry.insert(std::any::type_name::<#struct_type>().to_owned(), vtable);
                    }
                };

                output.into()
            } else {
                TokenStream::from(
                    Error::new(
                        item_impl.into_token_stream().span(),
                        "serde_box expected `impl trait for struct`",
                    )
                    .to_compile_error(),
                )
            }
        }
        Item::Trait(item_trait) => {
            let trait_ident = &item_trait.ident;
            let output = quote! {
                #item_trait

                impl SerdeBoxRegistry for dyn #trait_ident {
                    fn get_registry() -> &'static Registry {
                        use std::lazy::SyncLazy;
                        static REGISTRY: SyncLazy<Registry> = SyncLazy::new(|| Registry {
                            type_name_to_vtable: Default::default(),
                        });
                        &REGISTRY
                    }
                }
            };
            output.into()
        }
        _ => TokenStream::from(
            Error::new(
                item.into_token_stream().span(),
                "serde_box expected `trait` or `impl for`",
            )
            .to_compile_error(),
        ),
    }
}

fn hash(tokens: &TokenStream) -> u64 {
    let mut hasher = hash_map::DefaultHasher::new();
    hasher.write(tokens.to_string().as_bytes());
    hasher.finish()
}
