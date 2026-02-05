use std::str::FromStr;

use proto_file_reader::ProtoServiceDescription;
use types_reader::TokensObject;

pub fn generate(input: proc_macro2::TokenStream) -> Result<proc_macro::TokenStream, syn::Error> {
    let as_str = input.to_string();
    let params_list = TokensObject::new(input.into());

    let params_list = match params_list {
        Ok(params_list) => params_list,
        Err(err) => {
            panic!("Can not parse params '{}'. Err: {:?}", as_str, err);
        }
    };

    let proto_file = params_list.get_named_param("proto_file")?;
    let proto_file = proto_file.unwrap_any_value_as_str()?;
    let proto_file = proto_file.as_str()?;

    let service_description = ProtoServiceDescription::read_proto_file(proto_file);

    let service_name =
        proc_macro2::TokenStream::from_str(service_description.get_service_name()).unwrap();

    let grpc_struct_name = params_list.try_get_named_param("grpc_struct_name");

    let grpc_struct_name = match grpc_struct_name {
        Some(grpc_struct_name) => {
            let grpc_struct_name = grpc_struct_name.unwrap_any_value_as_str()?;
            let grpc_struct_name = grpc_struct_name.as_str()?;
            proc_macro2::TokenStream::from_str(grpc_struct_name).unwrap()
        }
        None => quote::quote! {super::SdkGrpcService},
    };

    let crate_ns = params_list.get_named_param("crate_ns")?;
    let crate_ns = crate_ns.unwrap_any_value_as_str()?;
    let crate_ns = crate_ns.as_str()?;

    let with_telemetry = params_list.try_get_named_param("with_telemetry");

    let with_telemetry = match with_telemetry {
        Some(with_telemetry) => {
            let with_telemetry = with_telemetry.unwrap_as_value()?;
            let with_telemetry = with_telemetry.unwrap_value()?;
            let with_telemetry = with_telemetry.as_bool()?;
            with_telemetry.get_value()
        }
        None => false,
    };

    let mut functions = Vec::new();

    for rpc in service_description.rpc.iter() {
        let fn_name_str = rpc.get_fn_name();

        let telemetry_injection = if with_telemetry {
            let with_telemetry = crate::consts::inject_telemetry_line(fn_name_str.as_str());

            Some(with_telemetry)
        } else {
            None
        };

        let fn_name =
            proc_macro2::TokenStream::from_str(fn_name_str.as_snake_case().as_str()).unwrap();

        let input_param = if let Some(input_param) = rpc.get_input_param() {
            match input_param {
                proto_file_reader::ParamType::Single(tp_name) => {
                    let tp_name = proc_macro2::TokenStream::from_str(tp_name).unwrap();
                    quote::quote! {tonic::Request<#tp_name>}
                }
                proto_file_reader::ParamType::Stream(tp_name) => {
                    let tp_name = proc_macro2::TokenStream::from_str(tp_name).unwrap();
                    quote::quote! {tonic::Request<tonic::Streaming<#tp_name>>}
                }
            }
        } else {
            quote::quote! {tonic::Request<()>}
        };

        let mut stream_description = quote::quote! {};

        let (out_type, result_conversion) = if let Some(out_param) = rpc.get_output_param() {
            match out_param {
                proto_file_reader::ParamType::Single(tp_name) => {
                    let tp_name = proc_macro2::TokenStream::from_str(tp_name).unwrap();
                    (
                        quote::quote! {tonic::Response<#tp_name>},
                        quote::quote! {Ok(result.into())},
                    )
                }
                proto_file_reader::ParamType::Stream(tp_name) => {
                    let fn_name_streamed = format!("{}Stream", fn_name_str.as_str());

                    stream_description = quote::quote! {
                         generate_server_stream!(stream_name: #fn_name_streamed, item_name: #tp_name);
                    };

                    let fn_name =
                        proc_macro2::TokenStream::from_str(fn_name_streamed.as_str()).unwrap();
                    (
                        quote::quote! {tonic::Response<Self::#fn_name>},
                        quote::quote! {result.get_result()},
                    )
                }
            }
        } else {
            (
                quote::quote! {tonic::Response<()>},
                quote::quote! {Ok(result.into())},
            )
        };

        if let Some(telemetry_injection) = telemetry_injection {
            functions.push(quote::quote! {

                #stream_description

                async fn #fn_name(&self, request:#input_param)->Result<#out_type, tonic::Status>{
                    #telemetry_injection
                    let request = request.into_inner();
                    let result = #fn_name(&self.app, request.into(),my_telemetry).await;
                    #result_conversion
                }
            });
        } else {
            functions.push(quote::quote! {
                #stream_description
                async fn #fn_name(&self, request:#input_param)->Result<#out_type, tonic::Status>{
                    let request = request.into_inner();
                    let result = #fn_name(&self.app, request.into()).await;
                    #result_conversion
                }
            });
        }
    }

    let service_name_lc = service_description.get_service_name().to_lowercase();

    let server_ns = proc_macro2::TokenStream::from_str(
        format!("{}::{}_server::*", crate_ns, service_name_lc).as_str(),
    )
    .unwrap();

    let crate_ns = proc_macro2::TokenStream::from_str(crate_ns).unwrap();

    let result = quote::quote! {

        use #server_ns;
        use #crate_ns::*;

        #[tonic::async_trait]
        impl #service_name for #grpc_struct_name{
            #(#functions)*

          async fn ping(&self, _: tonic::Request<()>) -> Result<tonic::Response<()>, tonic::Status> {
              Ok(tonic::Response::new(()))
          }
        }


    };

    Ok(result.into())
}
