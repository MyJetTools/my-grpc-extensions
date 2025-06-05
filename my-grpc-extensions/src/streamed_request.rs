use std::sync::Arc;

use super::*;

pub struct StreamedRequest<TItem: Send + Sync + 'static + Clone> {
    inner: Arc<StreamedRequestInner<TItem>>,
    channel_size: usize,
}

impl<TItem: Send + Sync + 'static + Clone> StreamedRequest<TItem> {
    pub fn new_as_vec(data: Vec<TItem>) -> Self {
        Self {
            inner: Arc::new(StreamedRequestInner::AsVec(data)),
            channel_size: 1000,
        }
    }

    pub fn new_as_stream(data: Vec<TItem>) -> Self {
        Self {
            inner: Arc::new(StreamedRequestInner::AsVec(data)),
            channel_size: 1000,
        }
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
