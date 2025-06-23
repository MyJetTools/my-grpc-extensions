use std::sync::Arc;

use tokio::sync::Mutex;

use super::*;

pub struct StreamedRequest<TItem: Send + Sync + 'static + Clone> {
    inner: Arc<StreamedRequestInner<TItem>>,
    channel_size: usize,
}

impl<TItem: Send + Sync + 'static + Clone> StreamedRequest<TItem> {
    pub fn new_as_vec(data: Vec<TItem>) -> Self {
        Self {
            inner: Arc::new(StreamedRequestInner::AsVec(data)),
            channel_size: 1024,
        }
    }

    pub fn new_as_stream() -> Self {
        let inner = StreamedRequestInner::AsStream(Mutex::new(RequestAsStream::default()));
        Self {
            inner: Arc::new(inner),
            channel_size: 1024,
        }
    }

    pub fn set_channel_size(mut self, value: usize) -> Self {
        self.channel_size = value;
        self
    }
    pub fn get_producer(&self) -> StreamedRequestProducer<TItem> {
        StreamedRequestProducer {
            inner: self.inner.clone(),
        }
    }

    pub fn get_consumer(&self) -> tokio_stream::wrappers::ReceiverStream<TItem> {
        let (tx, rx) = tokio::sync::mpsc::channel(self.channel_size);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            inner.set_sender(tx).await;
        });

        tokio_stream::wrappers::ReceiverStream::new(rx)
    }
}

impl<TItem: Send + Sync + 'static + Clone> Into<StreamedRequest<TItem>> for Vec<TItem> {
    fn into(self) -> StreamedRequest<TItem> {
        StreamedRequest::new_as_vec(self)
    }
}
