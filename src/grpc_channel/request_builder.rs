use crate::{GrpcReadError, RentedChannel, RequestBuilderWithRetries, RequestResponseGrpcExecutor};

pub struct RequestBuilder<TService: Send + Sync + 'static, TRequest: Clone + Send + Sync + 'static>
{
    input_contract: TRequest,
    channel: RentedChannel<TService>,
}

impl<TService: Send + Sync + 'static, TRequest: Clone + Send + Sync + 'static>
    RequestBuilder<TService, TRequest>
{
    pub fn new(input_contract: TRequest, channel: RentedChannel<TService>) -> Self {
        Self {
            input_contract,
            channel,
        }
    }

    pub fn with_retries(
        self,
        attempts_amount: usize,
    ) -> RequestBuilderWithRetries<TService, TRequest> {
        RequestBuilderWithRetries::new(self.input_contract, self.channel, attempts_amount)
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
            .execute(self.input_contract, grpc_executor)
            .await
    }
}
