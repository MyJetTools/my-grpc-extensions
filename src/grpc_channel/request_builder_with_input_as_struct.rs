use futures::Future;

use crate::{GrpcReadError, RentedChannel};

pub struct RequestBuilderWithInputAsStruct<TService: Send + Sync + 'static, TInputContract> {
    input_contract: TInputContract,
    channel: RentedChannel<TService>,
}

impl<TService: Send + Sync + 'static, TInputContract>
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
        get_future: impl Fn(&mut TService, TInputContract) -> TFuture,
    ) -> Result<TResponse, GrpcReadError>
    where
        TResponse: Send + Sync + 'static,
    {
        let mut service = self.channel.get_service();
        let future = get_future(&mut service, self.input_contract);

        let response: Result<TResponse, GrpcReadError> =
            self.channel.execute_with_timeout(future).await;
        match response {
            Ok(result) => Ok(result),
            Err(err) => {
                self.channel.drop_channel_if_needed(&err).await;
                Err(err)
            }
        }
    }
}
