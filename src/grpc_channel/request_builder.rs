use crate::{GrpcReadError, RentedChannel, RequestResponseGrpcExecutor};

pub struct RequestBuilder<TService: Send + Sync + 'static, TRequest: Send + Sync + 'static> {
    input_contract: TRequest,
    channel: RentedChannel<TService>,
}

impl<TService: Send + Sync + 'static, TRequest: Send + Sync + 'static>
    RequestBuilder<TService, TRequest>
{
    pub fn new(input_contract: TRequest, channel: RentedChannel<TService>) -> Self {
        Self {
            input_contract,
            channel,
        }
    }

    pub async fn execute<
        TResponse,
        TExecutor: RequestResponseGrpcExecutor<TService, TRequest, TResponse> + Send + Sync + 'static,
    >(
        mut self,
        grpc_executor: &TExecutor,
    ) -> Result<TResponse, GrpcReadError>
    where
        TResponse: Send + Sync + 'static,
    {
        self.channel
            .execute_with_timeout(self.input_contract, grpc_executor)
            .await
    }
}
