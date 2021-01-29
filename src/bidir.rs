use std::sync::mpsc::{self, Receiver, RecvError, Sender, TryRecvError};

use mpsc::SendError;

pub struct BidirChannel<M> {
    sender: Sender<M>,
    receiver: Receiver<M>,
}

impl<M> BidirChannel<M> {
    pub fn new_pair() -> (Self, Self) {
        let (f_snd, s_rcv) = mpsc::channel();
        let (s_snd, f_rcv) = mpsc::channel();
        (
            Self {
                sender: f_snd,
                receiver: f_rcv,
            },
            Self {
                sender: s_snd,
                receiver: s_rcv,
            },
        )
    }

    pub fn send(&self, message: M) -> Result<(), SendError<M>> {
        self.sender.send(message)
    }
    pub fn recv(&self) -> Result<M, RecvError> {
        self.receiver.recv()
    }

    pub fn try_recv(&self) -> Result<M, TryRecvError> {
        self.receiver.try_recv()
    }
}
