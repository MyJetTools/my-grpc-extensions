use my_telemetry::MyTelemetryContext;
use tonic::service::Interceptor;

pub struct GrpcClientInterceptor {
    process_id: i64,
}

impl GrpcClientInterceptor {
    pub fn new(telemetry_context: &MyTelemetryContext) -> Self {
        Self {
            process_id: telemetry_context.process_id,
        }
    }
}

impl Interceptor for GrpcClientInterceptor {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        request
            .metadata_mut()
            .insert("process-id", self.process_id.to_string().parse().unwrap());
        Ok(request)
    }
}
