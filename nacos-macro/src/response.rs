use syn::parse::Parser;
use syn::{parse_quote, ItemStruct, Path};

use quote::quote;

use crate::{Crates, MacroArgs};

const SUCCESS_RESPONSE_RESULT_CODE: i32 = 200;
const SUCCESS_RESPONSE_ERROR_CODE: i32 = 0;

pub(crate) fn grpc_response(
    macro_args: MacroArgs,
    mut item_struct: ItemStruct,
) -> proc_macro2::TokenStream {
    let identity = macro_args.identity;
    let name = &item_struct.ident;

    let Crates { serde, .. } = macro_args.crates;

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
                    result_code: #SUCCESS_RESPONSE_RESULT_CODE,
                    error_code: #SUCCESS_RESPONSE_ERROR_CODE,
                    message: None,
                    request_id: None
                }
            }

            pub fn request_id(&self) -> &Option<String> {
                &self.request_id
            }

            pub fn result_code(&self) -> i32 {
                self.result_code
            }

            pub fn error_code(&self) -> i32 {
                self.error_code
            }

            pub fn message(&self) -> Option<String> {
                self.message.as_ref().map(|msg|{msg.to_string()})
            }

            pub fn is_success(&self) -> bool {
                self.result_code == #SUCCESS_RESPONSE_RESULT_CODE
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

    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        let result_code_field = syn::Field::parse_named
            .parse2(quote! {
                result_code: i32
            })
            .unwrap();
        let error_code_field = syn::Field::parse_named
            .parse2(quote! {
                error_code: i32
            })
            .unwrap();
        let message_field = syn::Field::parse_named
            .parse2(quote! {
                message: Option<String>
            })
            .unwrap();
        let request_id_field = syn::Field::parse_named
            .parse2(quote! {
                request_id: Option<String>
            })
            .unwrap();

        fields.named.push(result_code_field);
        fields.named.push(error_code_field);
        fields.named.push(message_field);
        fields.named.push(request_id_field);
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
