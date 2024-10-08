use my_telemetry::MyTelemetryContext;
use tonic::service::Interceptor;

pub struct GrpcClientInterceptor {
    ctx: MyTelemetryContext,
}

impl GrpcClientInterceptor {
    pub fn new(ctx: MyTelemetryContext) -> Self {
        Self { ctx }
    }

    pub fn to_string(&self) -> Option<String> {
        match &self.ctx {
            MyTelemetryContext::Single(process_id) => Some(process_id.to_string()),
            MyTelemetryContext::Multiple(ids) => {
                let mut result = String::new();
                let mut index = 0;
                for id in ids {
                    if index > 0 {
                        result.push(',')
                    }

                    result.push_str(&id.to_string());

                    index += 1;
                }

                Some(result)
            }
            MyTelemetryContext::Empty => None,
        }
    }
}

impl Interceptor for GrpcClientInterceptor {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(process_id) = self.to_string() {
            request
                .metadata_mut()
                .insert("process-id", process_id.parse().unwrap());
        }

        Ok(request)
    }
}
