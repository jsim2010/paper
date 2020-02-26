//! Implements traits for sending and receiving messages.
use {
    core::fmt::Debug,
    crossbeam_channel::{Receiver, RecvError, SendError, Sender},
    std::sync::mpsc::{Receiver as StdReceiver, TryRecvError},
    thiserror::Error,
};

/// An error while consuming a record.
#[derive(Clone, Copy, Debug, Error)]
#[error("unable to consume; all producers were cut")]
pub struct ConsumeError;

impl From<RecvError> for ConsumeError {
    fn from(_: RecvError) -> Self {
        Self
    }
}

///  Retrieves records that have been produced by a [`Producer`].
pub(crate) trait Consumer: Debug {
    /// The type that is being consumed.
    type Record;

    /// Returns if a record is available.
    fn can_consume(&self) -> bool;
    /// Blocks the current thread until a record is available.
    fn consume(&self) -> Result<Self::Record, ConsumeError>;

    /// Consumes a record if one is available.
    fn optional_consume(&self) -> Result<Option<Self::Record>, ConsumeError> {
        if self.can_consume() {
            self.consume().map(Some)
        } else {
            Ok(None)
        }
    }

    /// Returns an [`Iterator`] that blocks the current thread until `self` can consume a record.
    fn records(&self) -> RecordsIter<'_, Self::Record>
    where
        Self: Sized,
    {
        RecordsIter { consumer: self }
    }
}

/// Adds records that can be consumed by a [`Consumer`].
pub trait Producer<'a> {
    /// The type that is being produced.
    type Good;
    /// The possible error type.
    type Error;

    /// Produces `good` on the respective queue.
    fn produce(&'a self, good: Self::Good) -> Result<(), Self::Error>;
}

/// Maps a [`crossbeam_channel::Sender`] to a [`Consumer`].
#[derive(Debug)]
pub(crate) struct CrossbeamConsumer<T> {
    /// The consumer.
    rx: Receiver<T>,
}

impl<T: Debug> Consumer for CrossbeamConsumer<T> {
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

/// Maps a [`crossbeam_channel::Receiver`] to a [`Producer<`].
#[derive(Debug)]
pub(crate) struct CrossbeamProducer<T> {
    /// The producer.
    tx: Sender<T>,
}

impl<'a, T> Producer<'a> for CrossbeamProducer<T> {
    type Good = T;
    type Error = SendError<T>;

    fn produce(&self, record: Self::Good) -> Result<(), Self::Error> {
        self.tx.send(record)
    }
}

impl<T> From<Sender<T>> for CrossbeamProducer<T> {
    fn from(value: Sender<T>) -> Self {
        Self { tx: value }
    }
}

/// A queue where a consumer yields records in the order a producer produces them.
#[derive(Debug)]
pub struct Queue<T> {
    /// The producer.
    producer: CrossbeamProducer<T>,
    /// The consumer.
    consumer: CrossbeamConsumer<T>,
}

impl<T> Queue<T> {
    /// Creates a new [`Queue`].
    pub(crate) fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();

        Self {
            producer: tx.into(),
            consumer: rx.into(),
        }
    }
}

impl<T: Debug> Consumer for Queue<T> {
    type Record = T;

    fn can_consume(&self) -> bool {
        self.consumer.can_consume()
    }

    fn consume(&self) -> Result<T, ConsumeError> {
        self.consumer.consume()
    }
}

impl<'a, T> Producer<'a> for Queue<T> {
    type Good = T;
    type Error = SendError<T>;

    fn produce(&self, good: Self::Good) -> Result<(), Self::Error> {
        self.producer.produce(good)
    }
}

/// Maps a [`Receiver`] to a [`Consumer`].
#[derive(Debug)]
pub(crate) struct StdConsumer<T> {
    /// The [`Receiver`].
    std_rx: StdReceiver<T>,
    /// The queue to hold records from `std_rx`.
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

impl<T: Debug> Consumer for StdConsumer<T> {
    type Record = T;

    fn can_consume(&self) -> bool {
        self.queue.can_consume()
            || match self.std_rx.try_recv() {
                Err(TryRecvError::Disconnected) => true,
                Err(TryRecvError::Empty) => false,
                Ok(record) => {
                    // Failure is okay since this only happens when queue is being dropped.
                    let _ = self.queue.produce(record);
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

/// An [`Iterator`] that yields the next consumed record.
pub(crate) struct RecordsIter<'a, T> {
    /// The [`Consumer`] that yields records.
    consumer: &'a dyn Consumer<Record = T>,
}

impl<T> Iterator for RecordsIter<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.consumer.consume().ok()
    }
}
