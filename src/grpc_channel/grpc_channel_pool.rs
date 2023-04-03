use tonic::transport::Channel;

pub struct GrpcChannelPool {
    pub channels: Vec<Channel>,

    max_amount_to_keep_in_pool: usize,
}

impl GrpcChannelPool {
    pub fn new(max_amount_to_keep_in_pool: usize) -> Self {
        Self {
            channels: Vec::new(),
            max_amount_to_keep_in_pool,
        }
    }

    pub fn push_channel_back(&mut self, channel: Channel) {
        if self.channels.len() < self.max_amount_to_keep_in_pool {
            self.channels.push(channel);
        }
    }

    pub fn rent(&mut self) -> Option<Channel> {
        self.channels.pop()
    }
}
