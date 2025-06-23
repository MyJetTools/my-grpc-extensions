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
    pub async fn send(&self, item: TItem) {
        match self {
            StreamedRequestInner::AsVec(_) => {
                panic!("Can not enqueue new item to send to GRPC mode since it in Vector Mode");
            }
            StreamedRequestInner::AsStream(inner) => {
                let mut write_access = inner.lock().await;

                match &mut *write_access {
                    RequestAsStream::NotInitialized {
                        items,
                        has_end_of_stream,
                    } => {
                        if *has_end_of_stream {
                            println!("StreamedRequestInner is ended");
                            panic!("StreamedRequestInner is ended");
                        }
                        items.push(item);
                    }
                    RequestAsStream::Initialized(sender) => match sender {
                        Some(sender) => {
                            let err = sender.send(item).await;

                            if let Err(err) = err {
                                println!("Can not send grpc item to stream #1. Err: {}", err);
                                return;
                            }
                        }
                        None => {
                            println!("StreamedRequestInner is ended");
                            panic!("StreamedRequestInner is ended");
                        }
                    },
                }
            }
        }
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
                        for itm in items.drain(..) {
                            let err = sender.send(itm.clone()).await;
                            if let Err(err) = err {
                                println!("Can not send grpc item to stream #2. Err: {}", err);
                                return;
                            }
                        }
                        if *has_end_of_stream {
                            return;
                        }
                    }
                    RequestAsStream::Initialized(_) => {
                        panic!("Somehow stream is set for a second time")
                    }
                }

                *write_access = RequestAsStream::Initialized(Some(sender));
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
