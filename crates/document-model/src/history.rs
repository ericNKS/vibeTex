use vibetex_core::TextDelta;

/// A multi-level undo/redo history storing atomic inverse deltas.
#[derive(Debug, Default, Clone)]
pub struct HistoryStack {
    undo_stack: Vec<TextDelta>,
    redo_stack: Vec<TextDelta>,
    max_depth: usize,
}

impl HistoryStack {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: max_depth.max(10),
        }
    }

    pub fn push(&mut self, inverse_delta: TextDelta) {
        if self.undo_stack.len() >= self.max_depth {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(inverse_delta);
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn pop_undo(&mut self) -> Option<TextDelta> {
        self.undo_stack.pop()
    }

    pub fn push_redo(&mut self, inverse_delta: TextDelta) {
        self.redo_stack.push(inverse_delta);
    }

    pub fn pop_redo(&mut self) -> Option<TextDelta> {
        self.redo_stack.pop()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}
