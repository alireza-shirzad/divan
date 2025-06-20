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
