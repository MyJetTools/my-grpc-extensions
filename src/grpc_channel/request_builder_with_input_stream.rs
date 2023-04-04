use crate::{GrpcReadError, RentedChannel, RequestWithInputAsStreamGrpcExecutor};

pub struct RequestBuilderWithInputStream<
    TService: Send + Sync + 'static,
    TRequest: Send + Sync + 'static,
> {
    input_contract: Vec<TRequest>,
    channel: RentedChannel<TService>,
}

impl<TService: Send + Sync + 'static, TRequest: Send + Sync + 'static>
    RequestBuilderWithInputStream<TService, TRequest>
{
    pub fn new(input_contract: Vec<TRequest>, channel: RentedChannel<TService>) -> Self {
        Self {
            input_contract,
            channel,
        }
    }

    pub async fn execute<
        TResponse,
        TExecutor: RequestWithInputAsStreamGrpcExecutor<TService, TRequest, TResponse> + Send + Sync + 'static,
    >(
        mut self,
        grpc_executor: &TExecutor,
    ) -> Result<TResponse, GrpcReadError>
    where
        TResponse: Send + Sync + 'static,
    {
        self.channel
            .execute_input_as_stream_with_timeout(self.input_contract, grpc_executor)
            .await
    }
}
