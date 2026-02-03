//! Command pattern utilities for drbot.
//!
//! This crate provides:
//! - Command trait
//! - Command queue/history
//! - Undoable commands

use std::sync::Arc;
use thiserror::Error;

/// Command error types.
#[derive(Error, Debug)]
pub enum CommandError {
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Undo failed: {0}")]
    UndoFailed(String),

    #[error("No command to undo")]
    NothingToUndo,

    #[error("No command to redo")]
    NothingToRedo,
}

/// Result type for command operations.
pub type Result<T> = std::result::Result<T, CommandError>;

/// Command trait.
pub trait Command: Send + Sync {
    /// Execute the command.
    fn execute(&self) -> Result<()>;
}

/// Undoable command trait.
pub trait UndoableCommand: Command {
    /// Undo the command.
    fn undo(&self) -> Result<()>;

    /// Check if command can be undone.
    fn can_undo(&self) -> bool {
        true
    }
}

/// Function-based command.
pub struct FnCommand<F: Fn() -> Result<()> + Send + Sync> {
    func: F,
}

impl<F: Fn() -> Result<()> + Send + Sync> FnCommand<F> {
    /// Create new function command.
    pub fn new(func: F) -> Self {
        Self { func }
    }
}

impl<F: Fn() -> Result<()> + Send + Sync> Command for FnCommand<F> {
    fn execute(&self) -> Result<()> {
        (self.func)()
    }
}

/// Function-based undoable command.
pub struct FnUndoableCommand<E, U>
where
    E: Fn() -> Result<()> + Send + Sync,
    U: Fn() -> Result<()> + Send + Sync,
{
    execute_fn: E,
    undo_fn: U,
}

impl<E, U> FnUndoableCommand<E, U>
where
    E: Fn() -> Result<()> + Send + Sync,
    U: Fn() -> Result<()> + Send + Sync,
{
    /// Create new undoable command.
    pub fn new(execute_fn: E, undo_fn: U) -> Self {
        Self {
            execute_fn,
            undo_fn,
        }
    }
}

impl<E, U> Command for FnUndoableCommand<E, U>
where
    E: Fn() -> Result<()> + Send + Sync,
    U: Fn() -> Result<()> + Send + Sync,
{
    fn execute(&self) -> Result<()> {
        (self.execute_fn)()
    }
}

impl<E, U> UndoableCommand for FnUndoableCommand<E, U>
where
    E: Fn() -> Result<()> + Send + Sync,
    U: Fn() -> Result<()> + Send + Sync,
{
    fn undo(&self) -> Result<()> {
        (self.undo_fn)()
    }
}

/// Command queue for sequential execution.
pub struct CommandQueue {
    commands: std::sync::Mutex<Vec<Arc<dyn Command>>>,
}

impl CommandQueue {
    /// Create new command queue.
    pub fn new() -> Self {
        Self {
            commands: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Add command to queue.
    pub fn add(&self, command: Arc<dyn Command>) {
        self.commands.lock().unwrap().push(command);
    }

    /// Execute all commands.
    pub fn execute_all(&self) -> Result<()> {
        let commands = self.commands.lock().unwrap();
        for cmd in commands.iter() {
            cmd.execute()?;
        }
        Ok(())
    }

    /// Execute and clear queue.
    pub fn flush(&self) -> Result<()> {
        let result = self.execute_all();
        self.clear();
        result
    }

    /// Get queue length.
    pub fn len(&self) -> usize {
        self.commands.lock().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.commands.lock().unwrap().is_empty()
    }

    /// Clear queue.
    pub fn clear(&self) {
        self.commands.lock().unwrap().clear();
    }
}

impl Default for CommandQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Command history for undo/redo support.
pub struct CommandHistory {
    executed: std::sync::Mutex<Vec<Arc<dyn UndoableCommand>>>,
    undone: std::sync::Mutex<Vec<Arc<dyn UndoableCommand>>>,
    max_history: usize,
}

impl CommandHistory {
    /// Create new command history.
    pub fn new(max_history: usize) -> Self {
        Self {
            executed: std::sync::Mutex::new(Vec::new()),
            undone: std::sync::Mutex::new(Vec::new()),
            max_history,
        }
    }

    /// Execute command and add to history.
    pub fn execute(&self, command: Arc<dyn UndoableCommand>) -> Result<()> {
        command.execute()?;

        let mut executed = self.executed.lock().unwrap();
        executed.push(command);

        // Trim history if needed
        while executed.len() > self.max_history {
            executed.remove(0);
        }

        // Clear redo stack
        self.undone.lock().unwrap().clear();

        Ok(())
    }

    /// Undo last command.
    pub fn undo(&self) -> Result<()> {
        let command = {
            let mut executed = self.executed.lock().unwrap();
            executed.pop().ok_or(CommandError::NothingToUndo)?
        };

        command.undo()?;
        self.undone.lock().unwrap().push(command);
        Ok(())
    }

    /// Redo last undone command.
    pub fn redo(&self) -> Result<()> {
        let command = {
            let mut undone = self.undone.lock().unwrap();
            undone.pop().ok_or(CommandError::NothingToRedo)?
        };

        command.execute()?;
        self.executed.lock().unwrap().push(command);
        Ok(())
    }

    /// Check if can undo.
    pub fn can_undo(&self) -> bool {
        !self.executed.lock().unwrap().is_empty()
    }

    /// Check if can redo.
    pub fn can_redo(&self) -> bool {
        !self.undone.lock().unwrap().is_empty()
    }

    /// Get history length.
    pub fn history_len(&self) -> usize {
        self.executed.lock().unwrap().len()
    }

    /// Get redo stack length.
    pub fn redo_len(&self) -> usize {
        self.undone.lock().unwrap().len()
    }

    /// Clear all history.
    pub fn clear(&self) {
        self.executed.lock().unwrap().clear();
        self.undone.lock().unwrap().clear();
    }
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Composite command that groups multiple commands.
pub struct CompositeCommand {
    commands: Vec<Arc<dyn Command>>,
}

impl CompositeCommand {
    /// Create new composite command.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Add command to composite.
    pub fn add(&mut self, command: Arc<dyn Command>) {
        self.commands.push(command);
    }

    /// Create from commands.
    pub fn from_commands(commands: Vec<Arc<dyn Command>>) -> Self {
        Self { commands }
    }
}

impl Default for CompositeCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for CompositeCommand {
    fn execute(&self) -> Result<()> {
        for cmd in &self.commands {
            cmd.execute()?;
        }
        Ok(())
    }
}

/// Helper to create command.
pub fn command<F>(func: F) -> Arc<dyn Command>
where
    F: Fn() -> Result<()> + Send + Sync + 'static,
{
    Arc::new(FnCommand::new(func))
}

/// Helper to create undoable command.
pub fn undoable_command<E, U>(execute_fn: E, undo_fn: U) -> Arc<dyn UndoableCommand>
where
    E: Fn() -> Result<()> + Send + Sync + 'static,
    U: Fn() -> Result<()> + Send + Sync + 'static,
{
    Arc::new(FnUndoableCommand::new(execute_fn, undo_fn))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn test_fn_command() {
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let cmd = command(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        cmd.execute().unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_command_queue() {
        let counter = Arc::new(AtomicI32::new(0));

        let queue = CommandQueue::new();
        for i in 1..=3 {
            let c = counter.clone();
            queue.add(command(move || {
                c.fetch_add(i, Ordering::SeqCst);
                Ok(())
            }));
        }

        queue.execute_all().unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 6); // 1 + 2 + 3
    }

    #[test]
    fn test_command_history() {
        let value = Arc::new(AtomicI32::new(0));
        let history = CommandHistory::new(10);

        // Execute: set to 10
        let v = value.clone();
        history
            .execute(undoable_command(
                move || {
                    v.store(10, Ordering::SeqCst);
                    Ok(())
                },
                {
                    let v = value.clone();
                    move || {
                        v.store(0, Ordering::SeqCst);
                        Ok(())
                    }
                },
            ))
            .unwrap();

        assert_eq!(value.load(Ordering::SeqCst), 10);

        // Undo
        history.undo().unwrap();
        assert_eq!(value.load(Ordering::SeqCst), 0);

        // Redo
        history.redo().unwrap();
        assert_eq!(value.load(Ordering::SeqCst), 10);
    }
}
