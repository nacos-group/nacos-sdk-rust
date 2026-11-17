use quote::quote;
use syn::ItemStruct;
use syn::parse::Parser;

use super::{MacroArgs, add_message_attrs, grpc_message_data_impl};

const SUCCESS_RESPONSE: (i32, &str) = (200, "Response ok");
const FAIL_RESPONSE: (i32, &str) = (500, "Response fail");

pub(crate) fn grpc_response(
    macro_args: MacroArgs,
    mut item_struct: ItemStruct,
) -> proc_macro2::TokenStream {
    let identity = macro_args.identity;
    let name = &item_struct.ident;
    let grpc_message_body = grpc_message_data_impl(name, &identity);

    let (success_code, success_message) = SUCCESS_RESPONSE;
    let (fail_code, fail_message) = FAIL_RESPONSE;

    let impl_message_response = quote! {

        impl #name {

            pub fn ok() -> Self {
                #name {
                    result_code: #success_code,
                    message: Some(#success_message.to_owned()),
                    ..Default::default()
                }
            }

            pub fn fail() -> Self {
                #name {
                    result_code: #fail_code,
                    message: Some(#fail_message.to_owned()),
                    ..Default::default()
                }
            }

        }

    };

    let grpc_message_response = quote! {

        impl crate::common::remote::grpc::message::GrpcResponseMessage for #name {
            fn request_id(&self) -> Option<&String> {
                self.request_id.as_ref()
            }

            fn result_code(&self) -> i32 {
                self.result_code
            }

            fn error_code(&self) -> i32 {
                self.error_code
            }

            fn message(&self) -> Option<&String> {
                self.message.as_ref()
            }

            fn is_success(&self) -> bool {
                self.result_code == #success_code
            }
        }
    };

    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        let result_code_field = syn::Field::parse_named
            .parse2(quote! {
                pub result_code: i32
            })
            .expect("Failed to parse response field");
        let error_code_field = syn::Field::parse_named
            .parse2(quote! {
                pub error_code: i32
            })
            .expect("Failed to parse response field");
        let message_field = syn::Field::parse_named
            .parse2(quote! {
                pub message: Option<String>
            })
            .expect("Failed to parse response field");
        let request_id_field = syn::Field::parse_named
            .parse2(quote! {
                pub request_id: Option<String>
            })
            .expect("Failed to parse response field");

        fields.named.push(result_code_field);
        fields.named.push(error_code_field);
        fields.named.push(message_field);
        fields.named.push(request_id_field);
    }

    add_message_attrs(&mut item_struct);

    quote! {
        #item_struct
        #grpc_message_response
        #grpc_message_body
        #impl_message_response
    }
}
