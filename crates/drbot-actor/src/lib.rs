//! Actor pattern utilities for drbot.
//!
//! This crate provides:
//! - Actor trait
//! - Message passing
//! - Actor system basics

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use thiserror::Error;

/// Actor error types.
#[derive(Error, Debug)]
pub enum ActorError {
    #[error("Actor stopped")]
    Stopped,

    #[error("Mailbox full")]
    MailboxFull,

    #[error("Send failed")]
    SendFailed,
}

/// Result type for actor operations.
pub type Result<T> = std::result::Result<T, ActorError>;

/// Message trait.
pub trait Message: Send + 'static {}

/// Blanket implementation for all Send types.
impl<T: Send + 'static> Message for T {}

/// Actor trait.
pub trait Actor: Send + 'static {
    /// Message type.
    type Msg: Message;

    /// Handle message.
    fn receive(&mut self, msg: Self::Msg);

    /// Called when actor starts.
    fn on_start(&mut self) {}

    /// Called when actor stops.
    fn on_stop(&mut self) {}
}

/// Mailbox for actor messages.
pub struct Mailbox<M> {
    messages: Mutex<VecDeque<M>>,
    cond: Condvar,
    closed: Mutex<bool>,
}

impl<M> Mailbox<M> {
    /// Create new mailbox.
    pub fn new() -> Self {
        Self {
            messages: Mutex::new(VecDeque::new()),
            cond: Condvar::new(),
            closed: Mutex::new(false),
        }
    }

    /// Send message to mailbox.
    pub fn send(&self, msg: M) -> Result<()> {
        if *self.closed.lock().unwrap() {
            return Err(ActorError::Stopped);
        }
        self.messages.lock().unwrap().push_back(msg);
        self.cond.notify_one();
        Ok(())
    }

    /// Receive message (blocking).
    pub fn receive(&self) -> Option<M> {
        let mut messages = self.messages.lock().unwrap();
        loop {
            if *self.closed.lock().unwrap() && messages.is_empty() {
                return None;
            }
            if let Some(msg) = messages.pop_front() {
                return Some(msg);
            }
            messages = self.cond.wait(messages).unwrap();
        }
    }

    /// Try to receive message (non-blocking).
    pub fn try_receive(&self) -> Option<M> {
        self.messages.lock().unwrap().pop_front()
    }

    /// Close mailbox.
    pub fn close(&self) {
        *self.closed.lock().unwrap() = true;
        self.cond.notify_all();
    }

    /// Check if closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap()
    }

    /// Get message count.
    pub fn len(&self) -> usize {
        self.messages.lock().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.messages.lock().unwrap().is_empty()
    }
}

impl<M> Default for Mailbox<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// Actor reference for sending messages.
pub struct ActorRef<M> {
    mailbox: Arc<Mailbox<M>>,
}

impl<M> ActorRef<M> {
    /// Send message to actor.
    pub fn send(&self, msg: M) -> Result<()> {
        self.mailbox.send(msg)
    }

    /// Check if actor is alive.
    pub fn is_alive(&self) -> bool {
        !self.mailbox.is_closed()
    }
}

impl<M> Clone for ActorRef<M> {
    fn clone(&self) -> Self {
        Self {
            mailbox: self.mailbox.clone(),
        }
    }
}

/// Actor handle for controlling actor.
pub struct ActorHandle<M> {
    mailbox: Arc<Mailbox<M>>,
    thread: Option<JoinHandle<()>>,
}

impl<M: Message> ActorHandle<M> {
    /// Spawn new actor.
    pub fn spawn<A: Actor<Msg = M>>(mut actor: A) -> (ActorRef<M>, Self) {
        let mailbox = Arc::new(Mailbox::new());
        let mailbox_clone = mailbox.clone();

        let thread = thread::spawn(move || {
            actor.on_start();
            while let Some(msg) = mailbox_clone.receive() {
                actor.receive(msg);
            }
            actor.on_stop();
        });

        let actor_ref = ActorRef {
            mailbox: mailbox.clone(),
        };

        let handle = Self {
            mailbox,
            thread: Some(thread),
        };

        (actor_ref, handle)
    }

    /// Get actor reference.
    pub fn actor_ref(&self) -> ActorRef<M> {
        ActorRef {
            mailbox: self.mailbox.clone(),
        }
    }

    /// Stop actor.
    pub fn stop(&mut self) {
        self.mailbox.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    /// Check if running.
    pub fn is_running(&self) -> bool {
        !self.mailbox.is_closed()
    }
}

impl<M> Drop for ActorHandle<M> {
    fn drop(&mut self) {
        self.mailbox.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Simple actor that processes messages with a function.
pub struct FnActor<M, F: FnMut(M) + Send + 'static> {
    handler: F,
    _marker: std::marker::PhantomData<M>,
}

impl<M: Message, F: FnMut(M) + Send + 'static> FnActor<M, F> {
    /// Create new function actor.
    pub fn new(handler: F) -> Self {
        Self {
            handler,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<M: Message, F: FnMut(M) + Send + 'static> Actor for FnActor<M, F> {
    type Msg = M;

    fn receive(&mut self, msg: Self::Msg) {
        (self.handler)(msg);
    }
}

/// Create actor from function.
pub fn actor<M: Message, F: FnMut(M) + Send + 'static>(
    handler: F,
) -> (ActorRef<M>, ActorHandle<M>) {
    ActorHandle::spawn(FnActor::new(handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn test_mailbox() {
        let mailbox = Mailbox::new();

        mailbox.send(42).unwrap();
        mailbox.send(43).unwrap();

        assert_eq!(mailbox.len(), 2);
        assert_eq!(mailbox.try_receive(), Some(42));
        assert_eq!(mailbox.try_receive(), Some(43));
        assert_eq!(mailbox.try_receive(), None);
    }

    #[test]
    fn test_actor() {
        let counter = Arc::new(AtomicI32::new(0));
        let c = counter.clone();

        let (actor_ref, mut handle) = actor(move |n: i32| {
            c.fetch_add(n, Ordering::SeqCst);
        });

        actor_ref.send(10).unwrap();
        actor_ref.send(20).unwrap();
        actor_ref.send(12).unwrap();

        handle.stop();

        assert_eq!(counter.load(Ordering::SeqCst), 42);
    }
}
