use futures::Future;

use crate::{GrpcReadError, RentedChannel, RequestResponseGrpcExecutor};

pub struct RequestBuilderWithInputAsStruct<
    TService: Send + Sync + 'static,
    TRequest: Send + Sync + 'static,
> {
    input_contract: TRequest,
    channel: RentedChannel<TService>,
}

impl<TService: Send + Sync + 'static, TRequest: Send + Sync + 'static>
    RequestBuilderWithInputAsStruct<TService, TRequest>
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
        let service = self.channel.get_service();
        self.channel
            .execute_with_timeout_2(service, self.input_contract, grpc_executor)
            .await
    }
}
