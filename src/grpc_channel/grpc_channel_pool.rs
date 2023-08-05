use my_logger::LogEventCtx;
use tonic::transport::Channel;

pub struct GrpcChannelPool {
    pub channel: Option<(Channel, String)>,
}

impl GrpcChannelPool {
    pub fn new() -> Self {
        Self { channel: None }
    }

    pub fn set(&mut self, service_name: &'static str, host: String, channel: Channel) {
        self.channel = Some((channel, host.clone()));

        my_logger::LOGGER.write_info(
            "GrpcChannelPool::set",
            "GRPC Connection is established",
            LogEventCtx::new()
                .add("GrpcClient", service_name)
                .add("Host".to_string(), host),
        );
    }

    pub fn rent(&mut self) -> Option<Channel> {
        let channel = self.channel.as_ref()?;
        Some(channel.0.clone())
    }
    pub fn disconnect_channel(&mut self) -> Option<String> {
        let result = self.channel.take();

        if let Some(result) = result {
            return Some(result.1);
        }
        None
    }
}
