use proc_macro::TokenStream;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use quote::*;
use syn::parse::{Parse, ParseBuffer};
use syn::spanned::Spanned;
use syn::*;

struct RegisterInput {
    trait_path: Path,
    struct_path: Path,
}

impl Parse for RegisterInput {
    fn parse(input: &ParseBuffer) -> Result<Self> {
        Ok(Self {
            trait_path: input.parse()?,
            struct_path: {
                input.parse::<Token![,]>()?;
                input.parse()?
            },
        })
    }
}

#[proc_macro]
pub fn register_serde_box(tokens: TokenStream) -> TokenStream {
    let mut hasher = DefaultHasher::new();
    tokens.to_string().hash(&mut hasher);
    let hash = hasher.finish();
    let register_fn = format!("fn register_serde_box_{}()", hash);
    let register_fn: Signature = parse_str(&register_fn).unwrap();
    let RegisterInput {
        trait_path,
        struct_path,
    } = parse_macro_input!(tokens as RegisterInput);

    let output = quote! {
        #[ctor]
        #register_fn {
            let trait_ptr = std::ptr::null::<#struct_path>() as *const dyn #trait_path;
            let trait_obj: metatype::TraitObject = metatype::type_coerce(metatype::Type::meta(trait_ptr));
            let vtable = trait_obj.vtable;
            let registry = <(dyn #trait_path) as SerdeBoxRegistry>::get_registry();
            registry.insert(std::any::type_name::<#struct_path>().to_owned(), vtable);
        }
    };
    output.into()
}

#[proc_macro_attribute]
pub fn serde_box(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as Item);
    match item {
        Item::Impl(item_impl) => {
            if let Some((None, trait_path, _for)) = &item_impl.trait_ {
                let register = if item_impl.generics.params.is_empty() {
                    let struct_type = &item_impl.self_ty;
                    quote! {
                        register_serde_box!(#trait_path, #struct_type);
                    }
                } else {
                    quote! {}
                };
                let output = quote! {
                    #item_impl
                    #register
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
