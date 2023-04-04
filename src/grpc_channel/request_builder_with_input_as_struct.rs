use futures::Future;

use crate::{GrpcReadError, RentedChannel, RequestResponseGrpcExecutor};

pub struct RequestBuilderWithInputAsStruct<
    TService: Send + Sync + 'static,
    TInputContract: Send + Sync + 'static,
> {
    input_contract: TInputContract,
    channel: RentedChannel<TService>,
}

impl<TService: Send + Sync + 'static, TInputContract: Send + Sync + 'static>
    RequestBuilderWithInputAsStruct<TService, TInputContract>
{
    pub fn new(input_contract: TInputContract, channel: RentedChannel<TService>) -> Self {
        Self {
            input_contract,
            channel,
        }
    }

    pub async fn execute<TResponse, TFuture: Future<Output = Result<TResponse, tonic::Status>>>(
        mut self,
        grpc_executor: impl RequestResponseGrpcExecutor<TService, TInputContract, TResponse>
            + Send
            + Sync
            + 'static,
    ) -> Result<TResponse, GrpcReadError>
    where
        TResponse: Send + Sync + 'static,
    {
        let service = self.channel.get_service();
        self.channel
            .execute_with_timeout_2(service, self.input_contract, grpc_executor)
            .await
    }
}
