use std::{
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use tokio_stream::wrappers::ReceiverStream;

use super::StreamedRequestInner;

// Producer
pub struct StreamedRequestConsumer<TItem: Clone> {
    inner: Arc<StreamedRequestInner<TItem>>,

    index: AtomicUsize,
}

impl<TItem: Clone> StreamedRequestConsumer<TItem> {
    pub fn new(inner: Arc<StreamedRequestInner<TItem>>) -> Self {
        let mut stream = ReceiverStream::new(rx);
        Self {
            inner,
            index: AtomicUsize::new(0),
        }
    }
    pub async fn get_next(&self) -> Option<TItem> {
        let next_index = self
            .index
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.receive(next_index).await
    }
}
