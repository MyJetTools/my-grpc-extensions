use std::str::FromStr;

use proc_macro2::Ident;

use super::{proto_file_reader::ProtoServiceDescription, ParamType};

pub fn generate_interfaces_implementations(
    struct_name: &Ident,
    proto_file: &ProtoServiceDescription,
) -> Vec<proc_macro2::TokenStream> {
    let mut result = Vec::new();

    for rpc in &proto_file.rpc {
        if let Some(input_param_type) = &rpc.get_input_param() {
            if let Some(output_param_type) = &rpc.get_output_param() {
                let input_param_type_token = get_name_fn_param_type_token(&input_param_type);

                let output_param_type_token = output_param_type.get_output_param_type_token();

                let input_param_name_token = input_param_type.get_name_as_token();
                let output_param_name_token = output_param_type.get_name_as_token();

                let interface_name = get_interface_name(&input_param_type, &output_param_type);

                let fn_name = rpc.get_fn_name_as_token();

                let input_param_invoke = input_param_type.get_input_param_invoke_token();

                let quote = quote::quote! {
                    #[async_trait::async_trait]
                    impl
                        #interface_name<
                            TGrpcService,
                            #input_param_name_token,
                            #output_param_name_token,
                        > for #struct_name
                    {
                        async fn execute(
                            &self,
                            mut service: TGrpcService,
                            input_data: #input_param_type_token,
                        ) -> Result<#output_param_type_token, tonic::Status> {
                            let result = service.#fn_name(#input_param_invoke).await?;
                            Ok(result.into_inner())
                        }
                    }
                };

                result.push(quote);
            } else {
                let input_param_type_token = get_name_fn_param_type_token(&input_param_type);

                let output_param_type_token = quote::quote! {()};

                let input_param_name_token = input_param_type.get_name_as_token();

                let interface_name = get_interface_name_with_input_param_only(&input_param_type);

                let fn_name = rpc.get_fn_name_as_token();

                let input_param_invoke = input_param_type.get_input_param_invoke_token();

                let quote = quote::quote! {
                    #[async_trait::async_trait]
                    impl
                        #interface_name<
                            TGrpcService,
                            #input_param_name_token,
                            #output_param_type_token,
                        > for #struct_name
                    {
                        async fn execute(
                            &self,
                            mut service: TGrpcService,
                            input_data: #input_param_type_token,
                        ) -> Result<#output_param_type_token, tonic::Status> {
                            let result = service.#fn_name(#input_param_invoke).await?;
                            Ok(result.into_inner())
                        }
                    }
                };

                result.push(quote);
            }
        } else {
            if let Some(output_param_type) = &rpc.get_output_param() {
                let input_param_type_token = quote::quote! {()};

                let output_param_type_token = output_param_type.get_output_param_type_token();

                let output_param_name_token = output_param_type.get_name_as_token();

                let interface_name = get_interface_name_with_output_param_only(&output_param_type);

                let fn_name = rpc.get_fn_name_as_token();

                let quote = quote::quote! {
                    #[async_trait::async_trait]
                    impl
                        #interface_name<
                            TGrpcService,
                            #input_param_type_token,
                            #output_param_name_token,
                        > for #struct_name
                    {
                        async fn execute(
                            &self,
                            mut service: TGrpcService,
                            input_data: #input_param_type_token,
                        ) -> Result<#output_param_type_token, tonic::Status> {
                            let result = service.#fn_name(()).await?;
                            Ok(result.into_inner())
                        }
                    }
                };

                result.push(quote);
            } else {
                panic!(
                    "RPC {} Not supported if input_param and output_param are both empty",
                    rpc.name
                )
            }
        }
    }

    result
}

fn get_interface_name(
    input_param: &super::ParamType<'_>,
    output_param: &super::ParamType<'_>,
) -> proc_macro2::TokenStream {
    if input_param.is_stream() {
        if output_param.is_stream() {
            quote::quote!(
                my_grpc_extensions::RequestWithInputAsStreamWithResponseAsStreamGrpcExecutor
            )
        } else {
            quote::quote!(my_grpc_extensions::RequestWithInputAsStreamGrpcExecutor)
        }
    } else {
        if output_param.is_stream() {
            quote::quote!(my_grpc_extensions::RequestWithResponseAsStreamGrpcExecutor)
        } else {
            quote::quote!(my_grpc_extensions::RequestResponseGrpcExecutor)
        }
    }
}

fn get_interface_name_with_input_param_only(
    input_param: &super::ParamType<'_>,
) -> proc_macro2::TokenStream {
    if input_param.is_stream() {
        quote::quote!(my_grpc_extensions::RequestWithInputAsStreamGrpcExecutor)
    } else {
        quote::quote!(my_grpc_extensions::RequestResponseGrpcExecutor)
    }
}

fn get_interface_name_with_output_param_only(
    output_param: &super::ParamType<'_>,
) -> proc_macro2::TokenStream {
    if output_param.is_stream() {
        quote::quote!(my_grpc_extensions::RequestWithResponseAsStreamGrpcExecutor)
    } else {
        quote::quote!(my_grpc_extensions::RequestResponseGrpcExecutor)
    }
}

fn get_name_fn_param_type_token(src: &ParamType<'_>) -> proc_macro2::TokenStream {
    match src {
        ParamType::Single(name) => proc_macro2::TokenStream::from_str(name).unwrap(),
        ParamType::Stream(name) => {
            let name = proc_macro2::TokenStream::from_str(name).unwrap();
            quote::quote!(Vec<#name>)
        }
    }
}
