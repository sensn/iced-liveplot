//! Keybinding System for iced-liveplot
//!
//! Provides configurable keyboard shortcuts for standard actions.

use iced::keyboard::{Key, Modifiers};
use iced::keyboard::key::Named;
use std::collections::HashMap;

/// Standard actions that can be bound to keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardAction {
    // Editing
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    Delete,
    SelectAll,
    DeselectAll,
    
    // View
    ZoomIn,
    ZoomOut,
    ZoomFit,
    ResetView,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    
    // Pins/Markers
    AddPin,
    RemovePin,
    TogglePin,
    
    // Grid/Visual
    ToggleGrid,
    ToggleMinorGrid,
    
    // Series navigation
    NextSeries,
    PrevSeries,
    
    // Tools
    ToolSelect,
    ToolPen,
    ToolEraser,
    ToolMove,
    ToolScale,
    ToolStretch,
    
    // Snap/Grid
    ToggleSnap,
}

/// A key chord (key + modifiers)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl Chord {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
    
    pub fn ctrl(key: Key) -> Self {
        Self::new(key, Modifiers::CTRL)
    }
    
    pub fn shift(key: Key) -> Self {
        Self::new(key, Modifiers::SHIFT)
    }
    
    pub fn alt(key: Key) -> Self {
        Self::new(key, Modifiers::ALT)
    }
    
    pub fn ctrl_shift(key: Key) -> Self {
        Self::new(key, Modifiers::CTRL | Modifiers::SHIFT)
    }
}

/// Key bindings configuration
#[derive(Debug, Clone)]
pub struct KeyBindings {
    /// Single key bindings (no modifiers or single modifier)
    single_keys: HashMap<Key, StandardAction>,
    /// Chord bindings (multiple modifiers)
    chords: HashMap<Chord, StandardAction>,
}

impl KeyBindings {
    pub fn new() -> Self {
        Self {
            single_keys: HashMap::new(),
            chords: HashMap::new(),
        }
    }
    
    /// Bind a key to an action
    pub fn bind(&mut self, key: Key, modifiers: Modifiers, action: StandardAction) {
        if modifiers == Modifiers::empty() || 
           modifiers == Modifiers::CTRL || 
           modifiers == Modifiers::SHIFT || 
           modifiers == Modifiers::ALT {
            // Single key or single modifier - use single_keys
            self.single_keys.insert(key, action);
        } else {
            // Multiple modifiers - use chords
            self.chords.insert(Chord::new(key, modifiers), action);
        }
    }
    
    /// Bind a chord to an action
    pub fn bind_chord(&mut self, chord: Chord, action: StandardAction) {
        self.chords.insert(chord, action);
    }
    
    /// Look up an action for a key press
    pub fn lookup(&self, key: Key, modifiers: Modifiers) -> Option<StandardAction> {
        // First check chords (more specific)
        let chord = Chord::new(key.clone(), modifiers);
        if let Some(action) = self.chords.get(&chord) {
            return Some(*action);
        }
        
        // Then check single keys
        if let Some(action) = self.single_keys.get(&key) {
            // Verify modifiers match for single keys
            if modifiers == Modifiers::empty() || 
               modifiers == Modifiers::CTRL || 
               modifiers == Modifiers::SHIFT || 
               modifiers == Modifiers::ALT {
                return Some(*action);
            }
        }
        
        None
    }
    
    /// Get all bindings for display/help
    pub fn all_bindings(&self) -> Vec<(Key, Modifiers, StandardAction)> {
        let mut bindings = Vec::new();
        
        for (key, action) in &self.single_keys {
            bindings.push((key.clone(), Modifiers::empty(), *action));
        }
        
        for (chord, action) in &self.chords {
            bindings.push((chord.key.clone(), chord.modifiers, *action));
        }
        
        bindings
    }
    
    /// Clear all bindings
    pub fn clear(&mut self) {
        self.single_keys.clear();
        self.chords.clear();
    }
    
    /// Merge another KeyBindings into this one (other takes precedence)
    pub fn merge(&mut self, other: KeyBindings) {
        self.single_keys.extend(other.single_keys);
        self.chords.extend(other.chords);
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut kb = Self::new();
        
        // Standard editing shortcuts
        kb.bind_chord(Chord::ctrl(Key::Character("z".into())), StandardAction::Undo);
        kb.bind_chord(Chord::ctrl_shift(Key::Character("z".into())), StandardAction::Redo);
        kb.bind_chord(Chord::ctrl(Key::Character("y".into())), StandardAction::Redo);
        kb.bind_chord(Chord::ctrl(Key::Character("c".into())), StandardAction::Copy);
        kb.bind_chord(Chord::ctrl(Key::Character("x".into())), StandardAction::Cut);
        kb.bind_chord(Chord::ctrl(Key::Character("v".into())), StandardAction::Paste);
        kb.bind(Key::Named(Named::Delete), Modifiers::empty(), StandardAction::Delete);
        kb.bind_chord(Chord::ctrl(Key::Character("a".into())), StandardAction::SelectAll);
        kb.bind_chord(Chord::ctrl_shift(Key::Character("a".into())), StandardAction::DeselectAll);
        
        // View shortcuts
        kb.bind_chord(Chord::ctrl(Key::Character("=".into())), StandardAction::ZoomIn);
        kb.bind_chord(Chord::ctrl(Key::Character("-".into())), StandardAction::ZoomOut);
        kb.bind_chord(Chord::ctrl(Key::Character("0".into())), StandardAction::ZoomFit);
        kb.bind(Key::Character("r".into()), Modifiers::CTRL, StandardAction::ResetView);
        
        // Pan shortcuts (arrow keys)
        kb.bind(Key::Named(Named::ArrowLeft), Modifiers::empty(), StandardAction::PanLeft);
        kb.bind(Key::Named(Named::ArrowRight), Modifiers::empty(), StandardAction::PanRight);
        kb.bind(Key::Named(Named::ArrowUp), Modifiers::empty(), StandardAction::PanUp);
        kb.bind(Key::Named(Named::ArrowDown), Modifiers::empty(), StandardAction::PanDown);
        
        // Pin shortcuts
        kb.bind(Key::Character("p".into()), Modifiers::empty(), StandardAction::AddPin);
        kb.bind(Key::Character("p".into()), Modifiers::SHIFT, StandardAction::RemovePin);
        kb.bind(Key::Character("m".into()), Modifiers::empty(), StandardAction::TogglePin);
        
        // Grid shortcuts
        kb.bind(Key::Character("g".into()), Modifiers::empty(), StandardAction::ToggleGrid);
        kb.bind(Key::Character("g".into()), Modifiers::SHIFT, StandardAction::ToggleMinorGrid);
        
        // Series navigation
        kb.bind(Key::Character("j".into()), Modifiers::CTRL, StandardAction::NextSeries);
        kb.bind(Key::Character("k".into()), Modifiers::CTRL, StandardAction::PrevSeries);
        
        // Tool shortcuts
        kb.bind(Key::Character("v".into()), Modifiers::empty(), StandardAction::ToolSelect);
        kb.bind(Key::Character("b".into()), Modifiers::empty(), StandardAction::ToolPen);
        kb.bind(Key::Character("e".into()), Modifiers::empty(), StandardAction::ToolEraser);
        kb.bind(Key::Character("v".into()), Modifiers::CTRL, StandardAction::ToolMove);
        
        // Snap/Grid
        kb.bind(Key::Character("s".into()), Modifiers::CTRL, StandardAction::ToggleSnap);
        kb
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_bindings() {
        let kb = KeyBindings::default();
        assert_eq!(kb.lookup(Key::Character("z".into()), Modifiers::CTRL), Some(StandardAction::Undo));
        assert_eq!(kb.lookup(Key::Character("y".into()), Modifiers::CTRL), Some(StandardAction::Redo));
        assert_eq!(kb.lookup(Key::Character("c".into()), Modifiers::CTRL), Some(StandardAction::Copy));
        assert_eq!(kb.lookup(Key::Character("v".into()), Modifiers::CTRL), Some(StandardAction::Paste));
        assert_eq!(kb.lookup(Key::Delete, Modifiers::empty()), Some(StandardAction::Delete));
    }
    
    #[test]
    fn test_custom_binding() {
        let mut kb = KeyBindings::new();
        kb.bind(Key::KeyQ, Modifiers::CTRL, StandardAction::Quit);
        
        // Note: Quit isn't in StandardAction, this is just testing the mechanism
        kb.bind(Key::KeyQ, Modifiers::CTRL, StandardAction::Undo);
        assert_eq!(kb.lookup(Key::KeyQ, Modifiers::CTRL), Some(StandardAction::Undo));
    }
}