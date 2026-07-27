use tokio::sync::Mutex;

pub enum RequestAsStream<TItem: Clone> {
    NotInitialized {
        items: Vec<TItem>,
        has_end_of_stream: bool,
    },
    Initialized(Option<tokio::sync::mpsc::Sender<TItem>>),
}

impl<TItem: Clone> Default for RequestAsStream<TItem> {
    fn default() -> Self {
        Self::NotInitialized {
            items: vec![],
            has_end_of_stream: false,
        }
    }
}

pub enum StreamedRequestInner<TItem: Clone> {
    AsVec(Vec<TItem>),
    AsStream(Mutex<RequestAsStream<TItem>>),
}

impl<TItem: Clone> StreamedRequestInner<TItem> {
    pub async fn send(&self, item: TItem) -> Result<(), String> {
        match self {
            StreamedRequestInner::AsVec(_) => {
                return Err(
                    "Can not enqueue new item to send to GRPC mode since it in Vector Mode"
                        .to_string(),
                );
            }
            StreamedRequestInner::AsStream(inner) => {
                let mut write_access = inner.lock().await;

                match &mut *write_access {
                    RequestAsStream::NotInitialized {
                        items,
                        has_end_of_stream,
                    } => {
                        if *has_end_of_stream {
                            return Err("StreamedRequestInner is ended".to_string());
                        }
                        items.push(item);
                    }
                    RequestAsStream::Initialized(sender) => match sender {
                        Some(sender) => {
                            if let Err(err) = sender.send(item).await {
                                return Err(format!(
                                    "Can not send grpc item to stream. Err: {}",
                                    err
                                ));
                            }
                        }
                        None => {
                            return Err("StreamedRequestInner is ended".to_string());
                        }
                    },
                }
            }
        }

        Ok(())
    }

    pub async fn set_sender(&self, sender: tokio::sync::mpsc::Sender<TItem>) {
        match self {
            StreamedRequestInner::AsVec(items) => {
                for itm in items {
                    let err = sender.send(itm.clone()).await;

                    if let Err(err) = err {
                        println!("Can not send grpc item to stream #1. Err: {}", err);
                        return;
                    }
                }
            }
            StreamedRequestInner::AsStream(mutex) => {
                let mut write_access = mutex.lock().await;

                match &mut *write_access {
                    RequestAsStream::NotInitialized {
                        items,
                        has_end_of_stream,
                    } => {
                        if *has_end_of_stream {
                            // Producer is done and the whole payload is still here - send a copy of
                            // it and keep the items, so a retry sends exactly the same stream.
                            // State stays NotInitialized, which keeps the request retryable.
                            for itm in items.iter() {
                                let err = sender.send(itm.clone()).await;
                                if let Err(err) = err {
                                    println!("Can not send grpc item to stream #2. Err: {}", err);
                                    return;
                                }
                            }
                            return;
                        }

                        // Producer is still alive: items are handed over to the consumer and are
                        // not kept, so from now on the request can not be sent again.
                        for itm in items.drain(..) {
                            let err = sender.send(itm.clone()).await;
                            if let Err(err) = err {
                                println!("Can not send grpc item to stream #2. Err: {}", err);
                                return;
                            }
                        }
                    }
                    RequestAsStream::Initialized(existing_sender) => {
                        // Re-binding the consumer: the previous one is dropped, which closes its
                        // stream. Whatever was already sent into it is gone.
                        *existing_sender = None;
                    }
                }

                *write_access = RequestAsStream::Initialized(Some(sender));
            }
        }
    }

    /// Tells whether the same request can be sent one more time.
    ///
    /// Vector mode keeps the items, so a retry sends exactly the same payload. Stream mode keeps
    /// them only while no consumer is bound yet, or while the producer has finished before the
    /// first attempt - once a live producer is bound to a consumer, the items are handed over and
    /// a retry would send a stream with the items of the failed attempt missing.
    pub async fn can_be_retried(&self) -> bool {
        match self {
            StreamedRequestInner::AsVec(_) => true,
            StreamedRequestInner::AsStream(inner) => {
                let read_access = inner.lock().await;
                matches!(&*read_access, RequestAsStream::NotInitialized { .. })
            }
        }
    }

    pub async fn send_eof(&self) {
        match self {
            StreamedRequestInner::AsVec(_) => {}
            StreamedRequestInner::AsStream(items) => {
                let mut write_access = items.lock().await;
                match &mut *write_access {
                    RequestAsStream::NotInitialized {
                        has_end_of_stream, ..
                    } => *has_end_of_stream = true,
                    RequestAsStream::Initialized(sender) => {
                        sender.take();
                    }
                }
            }
        }
    }
}
