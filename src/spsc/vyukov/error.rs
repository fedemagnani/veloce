#[derive(Debug)]
pub enum TrySendErr<T> {
    Full(T),
    Disconnected(T),
}

/// Thrown on disconnected channel
#[derive(Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}
