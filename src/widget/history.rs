//! History/Undo-Redo Manager for iced-liveplot
//!
//! Provides a robust undo/redo system with action grouping support.

use crate::widget::actions::{ActionContext, ActionDispatcher, HistoryGroup, PlotAction};
use crate::Plot;
use crate::widget::state::PlotUiState;
use crate::widget::config::PlotViewConfig;
use crate::widget::actions::Clipboard;

/// High-level history manager with additional features
pub struct History {
    dispatcher: ActionDispatcher,
    /// Callback for when history changes (for UI updates)
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self {
            dispatcher: ActionDispatcher::new(),
            on_change: None,
        }
    }
    
    pub fn with_max_history(mut self, max: usize) -> Self {
        self.dispatcher = self.dispatcher.with_max_history(max);
        self
    }
    
    pub fn with_group_window(mut self, duration: std::time::Duration) -> Self {
        self.dispatcher = self.dispatcher.with_group_window(duration);
        self
    }
    
    /// Set a callback to be called when history changes
    pub fn on_change<F>(mut self, f: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_change = Some(Box::new(f));
        self
    }
    
    /// Execute an action
    pub fn execute<A: PlotAction + 'static>(&mut self, action: A, plot: &mut Plot, state: &mut PlotUiState, config: &PlotViewConfig, clipboard: &mut Clipboard) -> crate::widget::actions::ActionResult {
        let mut ctx = ActionContext {
            plot,
            state: Some(state),
            clipboard,
            history: None,
            config,
        };
        let result = self.dispatcher.execute(std::sync::Arc::new(action), &mut ctx);
        
        // Notify change
        if let Some(ref cb) = self.on_change {
            cb();
        }
        
        result
    }
    
    /// Undo the last action
    pub fn undo(&mut self, plot: &mut Plot, state: &mut PlotUiState, config: &PlotViewConfig, clipboard: &mut Clipboard) -> Option<crate::widget::actions::ActionResult> {
        let mut ctx = ActionContext {
            plot,
            state: Some(state),
            clipboard,
            history: None,
            config,
        };
        let result = self.dispatcher.undo(&mut ctx);
        
        if result.is_some() {
            if let Some(ref cb) = self.on_change {
                cb();
            }
        }
        
        result
    }
    
    /// Redo the last undone action
    pub fn redo(&mut self, plot: &mut Plot, state: &mut PlotUiState, config: &PlotViewConfig, clipboard: &mut Clipboard) -> Option<crate::widget::actions::ActionResult> {
        let mut ctx = ActionContext {
            plot,
            state: Some(state),
            clipboard,
            history: None,
            config,
        };
        let result = self.dispatcher.redo(&mut ctx);
        
        if result.is_some() {
            if let Some(ref cb) = self.on_change {
                cb();
            }
        }
        
        result
    }
    
    pub fn can_undo(&self) -> bool {
        self.dispatcher.can_undo()
    }
    
    pub fn can_redo(&self) -> bool {
        self.dispatcher.can_redo()
    }
    
    pub fn history(&self) -> &[HistoryGroup] {
        self.dispatcher.history()
    }
    
    pub fn redo_stack(&self) -> &[HistoryGroup] {
        self.dispatcher.redo_stack()
    }
    
    pub fn clear(&mut self) {
        self.dispatcher.clear();
        if let Some(ref cb) = self.on_change {
            cb();
        }
    }
    
    /// Get a description of the last action for UI display
    pub fn last_action_description(&self) -> Option<String> {
        self.dispatcher.history().last().map(|g| g.description.clone())
    }
    
    /// Get a description of the next redo action for UI display
    pub fn next_redo_description(&self) -> Option<String> {
        self.dispatcher.redo_stack().last().map(|g| g.description.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_history_basic() {
        let mut history = History::new();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}