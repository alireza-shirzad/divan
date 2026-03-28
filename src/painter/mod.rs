pub(crate) mod json_painter;
pub(crate) mod tree_painter;

use crate::counter::BytesFormat;
use crate::stats::Stats;

pub(crate) trait Painter {
    /// Enter a parent node.
    fn start_parent(&mut self, name: &str, is_last: bool);
    /// Exit the current parent node.
    fn finish_parent(&mut self);
    /// Indicate that the next child node was ignored.
    ///
    /// This semantically combines start/finish operations.
    fn ignore_leaf(&mut self, name: &str, is_last: bool);
    /// Enter a leaf node.
    fn start_leaf(&mut self, name: &str, is_last: bool);
    /// Exit the current leaf node.
    fn finish_empty_leaf(&mut self);
    /// Exit the current leaf node, emitting statistics.
    fn finish_leaf(
        &mut self,
        is_last: bool,
        stats: &Stats,
        bytes_format: BytesFormat,
    );
}

pub(crate) struct CombinedPainter<A, B> {
    first: A,
    second: B,
}

impl<A, B> CombinedPainter<A, B> {
    pub(crate) fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

impl<A: Painter, B: Painter> Painter for CombinedPainter<A, B> {
    fn start_parent(&mut self, name: &str, is_last: bool) {
        self.first.start_parent(name, is_last);
        self.second.start_parent(name, is_last);
    }

    fn finish_parent(&mut self) {
        self.first.finish_parent();
        self.second.finish_parent();
    }

    fn ignore_leaf(&mut self, name: &str, is_last: bool) {
        self.first.ignore_leaf(name, is_last);
        self.second.ignore_leaf(name, is_last);
    }

    fn start_leaf(&mut self, name: &str, is_last: bool) {
        self.first.start_leaf(name, is_last);
        self.second.start_leaf(name, is_last);
    }

    fn finish_empty_leaf(&mut self) {
        self.first.finish_empty_leaf();
        self.second.finish_empty_leaf();
    }

    fn finish_leaf(
        &mut self,
        is_last: bool,
        stats: &Stats,
        bytes_format: BytesFormat,
    ) {
        self.first.finish_leaf(is_last, stats, bytes_format);
        self.second.finish_leaf(is_last, stats, bytes_format);
    }
}
