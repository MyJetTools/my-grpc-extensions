use std::sync::Arc;

use super::*;

pub struct StreamedRequest<TItem: Send + Sync + 'static + Clone> {
    inner: Arc<StreamedRequestInner<TItem>>,
}

impl<TItem: Send + Sync + 'static + Clone> StreamedRequest<TItem> {
    pub fn new_as_vec(data: Vec<TItem>) -> Self {
        Self {
            inner: Arc::new(StreamedRequestInner::AsVec(data)),
        }
    }

    pub fn new_as_stream(data: Vec<TItem>) -> Self {
        Self {
            inner: Arc::new(StreamedRequestInner::AsVec(data)),
        }
    }

    pub fn get_producer(&self) -> StreamedRequestProducer<TItem> {
        StreamedRequestProducer {
            inner: self.inner.clone(),
        }
    }

    pub fn get_consumer(&self) -> StreamedRequestConsumer<TItem> {
        StreamedRequestConsumer::new(self.inner.clone())
    }
}
