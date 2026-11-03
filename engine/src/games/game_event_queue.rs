// Copyright Rob Gage 2026

use std::collections::VecDeque;

/// First-in, first-out queue of a game's events, handed to `Game` callbacks; the engine passes
/// every queued event to `Game::handle_game_event` after the callback returns
pub struct GameEventQueue<E> {
    /// Events waiting to be handled in insertion order
    events: VecDeque<E>,
}

impl<E> GameEventQueue<E> {
    /// Creates an empty event queue
    pub fn new() -> Self {
        GameEventQueue {
            events: VecDeque::new(),
        }
    }

    /// Appends an event to the end of the queue
    pub fn push(&mut self, event: E) {
        self.events.push_back(event);
    }

    /// Returns the number of queued events
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Returns whether the queue contains no events
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Removes and returns the first queued event
    pub fn pop(&mut self) -> Option<E> {
        self.events.pop_front()
    }
}

impl<E> Default for GameEventQueue<E> {
    /// Creates an empty event queue
    fn default() -> Self {
        Self::new()
    }
}
