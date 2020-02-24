use {
    crossbeam_channel::{Sender, Receiver, SendError, RecvError},
    std::sync::mpsc::{Receiver as StdReceiver, TryRecvError as StdTryRecvError},
    thiserror::Error,
};

#[derive(Clone, Copy, Debug, Error)]
#[error("unable to consume; all producers were cut")]
pub struct ConsumeError;

impl From<RecvError> for ConsumeError {
    fn from(_: RecvError) -> Self {
        Self
    }
}

#[derive(Clone, Copy, Debug, Error)]
#[error("unable to produce; all consumers were cut")]
pub(crate) struct ProduceError;

impl<T> From<SendError<T>> for ProduceError {
    fn from(_: SendError<T>) -> Self {
        Self
    }
}

pub(crate) trait Consumer {
    type Record;

    fn can_consume(&self) -> bool;
    fn consume(&self) -> Result<Self::Record, ConsumeError>;

    fn optional_consume(&self) -> Result<Option<Self::Record>, ConsumeError> {
        if self.can_consume() {
            self.consume().map(|record| Some(record))
        } else {
            Ok(None)
        }
    }

    fn records(&self) -> RecordsIter<'_, Self::Record>
    where Self: Sized,
    {
        RecordsIter {
            consumer: self,
        }
    }
}

#[derive(Debug)]
pub(crate) struct CrossbeamConsumer<T> {
    rx: Receiver<T>,
}

impl<T> Consumer for CrossbeamConsumer<T> {
    type Record = T;

    fn can_consume(&self) -> bool {
        !self.rx.is_empty()
    }

    fn consume(&self) -> Result<Self::Record, ConsumeError> {
        Ok(self.rx.recv()?)
    }
}

impl<T> From<Receiver<T>> for CrossbeamConsumer<T> {
    fn from(value: Receiver<T>) -> Self {
        Self { rx: value }
    }
}

#[derive(Debug)]
pub(crate) struct CrossbeamProducer<T> {
    tx: Sender<T>,
}

impl<T> CrossbeamProducer<T> {
    fn produce(&self, record: T) -> Result<(), ProduceError> {
        Ok(self.tx.send(record)?)
    }
}

impl<T> From<Sender<T>> for CrossbeamProducer<T> {
    fn from(value: Sender<T>) -> Self {
        Self { tx: value }
    }
}

#[derive(Debug)]
pub(crate) struct Queue<T> {
    producer: CrossbeamProducer<T>,
    consumer: CrossbeamConsumer<T>,
}

impl<T> Queue<T> {
    pub(crate) fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();

        Self {
            producer: tx.into(),
            consumer: rx.into(),
        }
    }

    pub(crate) fn produce(&self, record: T) -> Result<(), ProduceError> {
        self.producer.produce(record)
    }
}

impl<T> Consumer for Queue<T> {
    type Record = T;

    fn can_consume(&self) -> bool {
        self.consumer.can_consume()
    }

    fn consume(&self) -> Result<T, ConsumeError> {
        self.consumer.consume()
    }
}

#[derive(Debug)]
pub(crate) struct StdConsumer<T> {
    std_rx: StdReceiver<T>,
    queue: Queue<T>,
}

impl<T> From<StdReceiver<T>> for StdConsumer<T> {
    fn from(value: StdReceiver<T>) -> Self {
        Self {
            std_rx: value,
            queue: Queue::new(),
        }
    }
}

impl<T> Consumer for StdConsumer<T> {
    type Record = T;

    fn can_consume(&self) -> bool {
        self.queue.can_consume() || match self.std_rx.try_recv() {
            Err(StdTryRecvError::Disconnected) => true,
            Err(StdTryRecvError::Empty) => false,
            Ok(record) => {
                self.queue.produce(record).unwrap();
                true
            }
        }
    }

    fn consume(&self) -> Result<Self::Record, ConsumeError> {
        if self.queue.can_consume() {
            self.queue.consume()
        } else {
            self.std_rx.recv().map_err(|_| ConsumeError)
        }
    }
}

pub(crate) struct RecordsIter<'a, E> {
    consumer: &'a dyn Consumer<Record=E>,
}

impl<E> Iterator for RecordsIter<'_, E> {
    type Item = E;

    fn next(&mut self) -> Option<Self::Item> {
        self.consumer.consume().ok()
    }
}
