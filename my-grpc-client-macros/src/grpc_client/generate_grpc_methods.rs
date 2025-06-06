use std::{collections::HashMap, str::FromStr};

use super::{fn_override::FnOverride, proto_file_reader::ProtoServiceDescription, ParamType};

pub fn generate_grpc_methods(
    proto_file: &ProtoServiceDescription,
    retries_amount: usize,
    overrides: &HashMap<String, FnOverride>,
    width_telemetry: bool,
) -> Vec<proc_macro2::TokenStream> {
    let mut result = Vec::new();

    for rpc in &proto_file.rpc {
        let fn_name = rpc.get_fn_name_as_token();

        let input_param = rpc.get_input_param();

        let output_param = rpc.get_output_param();

        let input_data_type = get_func_in_data_type(input_param.as_ref());

        let output_data_type = get_func_out_data_type(output_param.as_ref());

        let request_fn_name = get_request_fn_name(input_param.as_ref());
        let response_fn_name = get_response_fn_name(output_param.as_ref());

        let retries_amount = if let Some(value) = overrides.get(&rpc.name) {
            value.retries
        } else {
            retries_amount
        };

        let with_retries = if retries_amount > 0 {
            let amount = proc_macro2::Literal::usize_unsuffixed(retries_amount);
            quote::quote!(.with_retries(#amount))
        } else {
            quote::quote!()
        };

        let ctx_param = if width_telemetry {
            quote::quote!(ctx: &my_telemetry::MyTelemetryContext,)
        } else {
            quote::quote!()
        };

        let get_channel = if width_telemetry {
            quote::quote!(self.channel.get_channel(ctx))
        } else {
            quote::quote!(self.channel.get_channel())
        };

        //let log_fn_name = format!("{}::{}", struct_name, fn_name.to_string());

        let item = quote::quote! {
            pub async fn #fn_name(
                &self,
                input_data: #input_data_type,
                #ctx_param
            ) -> Result<#output_data_type, my_grpc_extensions::GrpcReadError> {
                let channel = #get_channel;

                let result = channel
                    .#request_fn_name(input_data)
                    #with_retries
                    .#response_fn_name;

                Ok(result)

            }
        };

        result.push(item);
    }

    result
}

fn get_request_fn_name(input_param: Option<&super::ParamType<'_>>) -> proc_macro2::TokenStream {
    match input_param {
        Some(input_param) => {
            if input_param.is_stream() {
                quote::quote! {start_request_with_input_prams_as_stream}
            } else {
                quote::quote! {start_request}
            }
        }
        None => {
            quote::quote! {start_request}
        }
    }
}

fn get_response_fn_name(input_param: Option<&super::ParamType<'_>>) -> proc_macro2::TokenStream {
    match input_param {
        Some(input_param) => {
            if input_param.is_stream() {
                quote::quote! {get_streamed_response(self).await?}
            } else {
                quote::quote! {get_response(self).await?}
            }
        }
        None => {
            quote::quote! {get_response(self).await?}
        }
    }
}

fn get_func_in_data_type(data_type: Option<&super::ParamType<'_>>) -> proc_macro2::TokenStream {
    match data_type {
        Some(input_param) => match input_param {
            ParamType::Single(name) => proc_macro2::TokenStream::from_str(name).unwrap(),
            ParamType::Stream(name) => {
                let param = proc_macro2::TokenStream::from_str(name).unwrap();
                quote::quote!(impl Into<my_grpc_extensions::StreamedRequest<#param>>)
            }
        },
        None => {
            quote::quote! {()}
        }
    }
}

fn get_func_out_data_type(data_type: Option<&super::ParamType<'_>>) -> proc_macro2::TokenStream {
    match data_type {
        Some(input_param) => match input_param {
            ParamType::Single(name) => proc_macro2::TokenStream::from_str(name).unwrap(),
            ParamType::Stream(name) => {
                let param = proc_macro2::TokenStream::from_str(name).unwrap();
                quote::quote!(my_grpc_extensions::StreamedResponse<#param>)
            }
        },
        None => {
            quote::quote! {()}
        }
    }
}
