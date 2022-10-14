use syn::parse::Parser;
use syn::{parse_quote, ItemStruct, Path};

use quote::quote;

use crate::{Crates, MacroArgs};

pub(crate) fn grpc_request(
    macro_args: MacroArgs,
    mut item_struct: ItemStruct,
) -> proc_macro2::TokenStream {
    let identity = macro_args.identity;
    let name = &item_struct.ident;

    let Crates { serde, std } = macro_args.crates;

    let (arg_and_type, arg_name): (Vec<_>, Vec<_>) = item_struct
        .fields
        .iter()
        .map(|f| {
            let name = &f.ident;
            let ty = &f.ty;
            let arg_and_type = quote! {
                #name: #ty
            };

            let arg_name = quote! {#name};
            (arg_and_type, arg_name)
        })
        .unzip();

    // add new func
    let new_fn = quote! {

        impl #name {

           pub fn new(#(#arg_and_type,)*) -> Self {
                #name {
                    #(#arg_name,)*
                    headers: #std::collections::HashMap::<String, String>::new(),
                    request_id: None,
                    module: None
                }
            }

            pub fn header(mut self, key: String, value: String) -> Self{
                self.headers.insert(key, value);
                self
            }

            pub fn get_headers(&self) -> #std::collections::HashMap<String, String> {

                self.headers.clone()
            }

            pub fn request_id(mut self, request_id: String) -> Self {
                self.request_id = Some(request_id);
                self
            }

            pub fn module(mut self, module: String) -> Self {
                self.module = Some(module);
                self
            }
        }
    };

    // add derive GrpcMessageBody
    let grpc_message_body = quote! {
        impl crate::naming::grpc::message::GrpcMessageBody for #name {
            fn identity<'a>() -> std::borrow::Cow<'a, str> {
                #identity.into()
            }
        }
    };

    // add field
    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        let headers_field = syn::Field::parse_named
            .parse2(quote! {
                headers: #std::collections::HashMap<String, String>
            })
            .unwrap();
        let request_id_field = syn::Field::parse_named
            .parse2(quote! {
                request_id: Option<String>
            })
            .unwrap();
        let module_field = syn::Field::parse_named
            .parse2(quote! {
                module: Option<String>
            })
            .unwrap();

        fields.named.push(headers_field);
        fields.named.push(request_id_field);
        fields.named.push(module_field);
    }

    let derive_paths: Vec<Path> = vec![
        syn::parse_quote! { #serde::Deserialize },
        syn::parse_quote! { #serde::Serialize },
        syn::parse_quote! { Clone },
        syn::parse_quote! { Debug },
    ];

    // add derive
    item_struct.attrs.push(parse_quote! {
        #[derive(#(#derive_paths),*)]

    });
    item_struct.attrs.push(parse_quote!(
        #[serde(rename_all = "camelCase")]
    ));

    quote! {
        #item_struct
        #new_fn
        #grpc_message_body
    }
    .into()
}
