//! Action Dispatcher - handles action execution with history management

use crate::widget::actions::{ActionContext, ActionDispatcher as ActionsDispatcher, ActionResult, Clipboard, HistoryGroup, PlotAction};
use crate::Plot;
use crate::widget::state::PlotUiState;
use crate::widget::config::PlotViewConfig;

pub use crate::widget::actions::ActionDispatcher;

/// Extended dispatcher that integrates with the plot system
pub struct Dispatcher {
    inner: ActionDispatcher,
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self {
            inner: ActionDispatcher::new(),
        }
    }
}

/// Borrow pack carrying all action context parts.
struct CtxParts<'a> {
    plot: &'a mut Plot,
    state: &'a mut PlotUiState,
    clipboard: &'a mut Clipboard,
    config: &'a PlotViewConfig,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_max_history(mut self, max: usize) -> Self {
        self.inner = self.inner.with_max_history(max);
        self
    }
    
    pub fn with_group_window(mut self, duration: std::time::Duration) -> Self {
        self.inner = self.inner.with_group_window(duration);
        self
    }
    
    pub fn execute<A: PlotAction + 'static>(&mut self, action: A, parts: CtxParts<'_>) -> ActionResult {
        let mut ctx = ActionContext {
            plot: parts.plot,
            state: parts.state,
            clipboard: parts.clipboard,
            history: None,
            config: parts.config,
        };
        self.inner.execute(std::sync::Arc::new(action), &mut ctx)
    }
    
    pub fn undo(&mut self, parts: CtxParts<'_>) -> Option<ActionResult> {
        let mut ctx = ActionContext {
            plot: parts.plot,
            state: parts.state,
            clipboard: parts.clipboard,
            history: None,
            config: parts.config,
        };
        self.inner.undo(&mut ctx)
    }
    
    pub fn redo(&mut self, parts: CtxParts<'_>) -> Option<ActionResult> {
        let mut ctx = ActionContext {
            plot: parts.plot,
            state: parts.state,
            clipboard: parts.clipboard,
            history: None,
            config: parts.config,
        };
        self.inner.redo(&mut ctx)
    }
    
    pub fn can_undo(&self) -> bool {
        self.inner.can_undo()
    }
    
    pub fn can_redo(&self) -> bool {
        self.inner.can_redo()
    }
    
    pub fn history(&self) -> &[HistoryGroup] {
        self.inner.history()
    }
    
    pub fn redo_stack(&self) -> &[HistoryGroup] {
        self.inner.redo_stack()
    }
    
    pub fn clear(&mut self) {
        self.inner.clear()
    }
}
