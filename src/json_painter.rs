use crate::counter::BytesFormat;
use crate::stats::Stats;
use std::fmt::Write;

pub(crate) struct JsonPainter {
    depth: usize,
    // TODO(2025-06-19): does this leak memory when I call buf.clear()? ~ kat
    pub buf: String,
    is_last: Vec<bool>,
    parents: Vec<String>,
    first: Option<String>,
}

impl JsonPainter {
    pub fn new() -> Self {
        Self {
            depth: 1,
            buf: String::from("{\n"),
            is_last: vec![],
            parents: vec![],
            first: None,
        }
    }

    fn reset(&mut self) {
        Self { depth: _, buf: _, is_last: _, parents: _, first: _ } = *self; // destructuring to make sure reset doesn't get out of sync with new

        // depth: 1,
        // TODO(2025-06-19): should this be an assert? ~ kat
        assert_eq!(self.depth, 1);
        self.depth = 1;
        // buf: String::from("{\n"),
        self.buf.clear();
        self.buf.push_str("{\n");
        // is_last: vec![],
        // TODO(2025-06-19): should this be an assert? ~ kat
        assert_eq!(
            self.is_last.len(),
            0,
            "contents of is_last: {:?}",
            self.is_last
        );
        // parents: vec![],
        // TODO(2025-06-19): should this be an assert? ~ kat
        assert_eq!(
            self.parents.len(),
            0,
            "contents of parents: {:?}",
            self.parents
        );
        // first: None,
        // TODO(2025-06-19): should this be an assert? ~ kat
        assert!(self.first.is_some());
        self.first = None;
    }
}

fn pad_by(n: usize) -> String {
    "\t".repeat(n)
}

impl JsonPainter {
    /// Enter a parent node.
    pub fn start_parent(&mut self, name: &str, is_last: bool) {
        if self.first.is_none() {
            self.first = Some(name.into());
        }

        let pad = pad_by(self.depth);
        self.is_last.push(is_last);

        if let Err(e) = writeln!(&mut self.buf, r#"{pad}"{name}": {{"#) {
            println!("Err in start_parent: {e} ");
            return;
        }

        self.parents.push(name.into());

        self.depth += 1;
    }
    /// Exit the current parent node.
    pub fn finish_parent(&mut self) {
        self.depth -= 1;
        let pad = pad_by(self.depth);
        let Some(is_last) = self.is_last.pop() else {
            println!("Err popping from the stack is_last, it's empty");
            return;
        };
        let ended = self.parents.pop().unwrap();

        if let Err(e) = writeln!(
            &mut self.buf,
            r"{pad}}}{}",
            if is_last || self.depth == 1 { "" } else { "," }
        ) {
            println!("Err in finish_parent: {e}");
            return;
        }

        if self.depth == 1 {
            assert_eq!(
                Some(ended),
                self.first,
                "Current theories:
                1. the buffer is being reused
                2. race condition
                Possible solution:
                if the tests are a problem then add new \"finalize\" function which will return the buffer and reset the internal states, but this probably breaks the API
                json (not yet parsed) buffer: {}",
                self.buf
            );

            self.buf.push('}');

            if let Err(e) = json::parse(self.buf.as_str()) {
                println!("{}", self.buf);
                panic!("Error parsing the output into json: {e:?}");
            }

            self.reset();
        }
    }
    /// Indicate that the next child node was ignored.
    ///
    /// This semantically combines start/finish operations.
    pub fn ignore_leaf(&mut self, name: &str, is_last: bool) {
        // TODO(2025-06-19): Do nothing? ~ kat
    }
    /// Enter a leaf node.
    pub fn start_leaf(&mut self, name: &str, is_last: bool) {
        let pad = pad_by(self.depth);
        self.is_last.push(is_last);

        if let Err(e) =
            writeln!(&mut self.buf, r#"{pad}"{name}_start_leaf": {{"#)
        {
            println!("Err in start_leaf: {e:?}");
            return;
        }

        self.depth += 1;
    }
    /// Exit the current leaf node.
    pub fn finish_empty_leaf(&mut self) {
        // TODO(2025-06-19): Do nothing? ~ kat
    }
    /// Exit the current leaf node, emitting statistics.
    pub fn finish_leaf(
        &mut self,
        is_last: bool,
        stats: &Stats,
        bytes_format: BytesFormat,
    ) {
        let tab = pad_by(1);
        let pad = pad_by(self.depth);
        let Some(is_last) = self.is_last.pop() else {
            println!("Err popping value from is_last in finish_leaf");
            return;
        };

        // I wanted to avoid including heavy dependencies like serde
        // I'm using https://docs.rs/json/latest/json/ to make sure
        // the manually created json is valid... by parsing it >.>
        // Should I change this code to use that crate for generating the json instead?
        // ~ kat
        let lines = [
            // stats: {
            r#""stats": {"#.to_string(),
            // {tab}sample_count: 1,
            format!(r#"{tab}"sample_count": {},"#, stats.sample_count),
            // {tab}iter_count: 50,
            format!(r#"{tab}"iter_count": {},"#, stats.iter_count),
            // {tab}time: StatsSet {
            format!(r#"{tab}"time": {{"#),
            // {tab}{tab}fastest: 1.77 ns,
            format!(r#"{tab}{tab}"fastest": "{}","#, stats.time.fastest),
            // {tab}{tab}slowest: 1.77 ns,
            format!(r#"{tab}{tab}"slowest": "{}","#, stats.time.slowest),
            // {tab}{tab}median: 1.77 ns,
            format!(r#"{tab}{tab}"median": "{}","#, stats.time.median),
            // {tab}{tab}mean: 1.77 ns
            format!(r#"{tab}{tab}"mean": "{}""#, stats.time.mean),
            // {tab}{tab}},
            format!(r"{tab}}},"),
            // {tab}max_alloc: AllocTally {
            format!(r#"{tab}"max_alloc": {{"#),
            // {tab}{tab}count: StatsSet {
            format!(r#"{tab}{tab}"count": {{"#),
            // {tab}{tab}{tab}fastest: 0.0,
            format!(
                r#"{tab}{tab}{tab}"fastest": "{}","#,
                stats.max_alloc.count.fastest
            ),
            // {tab}{tab}{tab}slowest: 0.0,
            format!(
                r#"{tab}{tab}{tab}"slowest": "{}","#,
                stats.max_alloc.count.slowest
            ),
            // {tab}{tab}{tab}median: 0.0,
            format!(
                r#"{tab}{tab}{tab}"median": "{}","#,
                stats.max_alloc.count.median
            ),
            // {tab}{tab}{tab}mean: 0.0
            format!(
                r#"{tab}{tab}{tab}"mean": "{}""#,
                stats.max_alloc.count.mean
            ),
            // {tab}{tab}},
            format!("{tab}{tab}}},"),
            // {tab}{tab}size: StatsSet {
            format!(r#"{tab}{tab}"size": {{"#),
            // {tab}{tab}{tab}fastest: 0.0,
            format!(
                r#"{tab}{tab}{tab}"fastest": "{}","#,
                stats.max_alloc.size.fastest
            ),
            // {tab}{tab}{tab}slowest: 0.0,
            format!(
                r#"{tab}{tab}{tab}"slowest": "{}","#,
                stats.max_alloc.size.slowest
            ),
            // {tab}{tab}{tab}median: 0.0,
            format!(
                r#"{tab}{tab}{tab}"median": "{}","#,
                stats.max_alloc.size.median
            ),
            // {tab}{tab}{tab}mean: 0.0
            format!(
                r#"{tab}{tab}{tab}"mean": "{}""#,
                stats.max_alloc.size.mean
            ),
            // {tab}{tab}}
            format!("{tab}{tab}}}"),
            // {tab}},
            format!("{tab}}},"),
            // {tab}alloc_tallies: {
            format!(r#"{tab}"alloc_tallies": {{"#),
            // TODO ~ kat
            format!(r#"{tab}{tab}"todo":"TODO""#),
            // {tab}{tab}"grow": AllocTally {
            // {tab}{tab}{tab}count: StatsSet {
            // {tab}{tab}{tab}{tab}fastest: 0.0,
            // {tab}{tab}{tab}{tab}slowest: 0.0,
            // {tab}{tab}{tab}{tab}median: 0.0,
            // {tab}{tab}{tab}{tab}mean: 0.0
            // {tab}{tab}{tab}},
            // {tab}{tab}{tab}size: StatsSet {
            // {tab}{tab}{tab}{tab}fastest: 0.0,
            // {tab}{tab}{tab}{tab}slowest: 0.0,
            // {tab}{tab}{tab}{tab}median: 0.0,
            // {tab}{tab}{tab}{tab}mean: 0.0
            // {tab}{tab}{tab}}
            // {tab}{tab}},
            // {tab}{tab}"shrink": AllocTally {
            // {tab}{tab}{tab}count: StatsSet {
            // {tab}{tab}{tab}{tab}fastest: 0.0,
            // {tab}{tab}{tab}{tab}slowest: 0.0,
            // {tab}{tab}{tab}{tab}median: 0.0,
            // {tab}{tab}{tab}{tab}mean: 0.0
            // {tab}{tab}{tab}},
            // {tab}{tab}{tab}size: StatsSet {
            // {tab}{tab}{tab}{tab}fastest: 0.0,
            // {tab}{tab}{tab}{tab}slowest: 0.0,
            // {tab}{tab}{tab}{tab}median: 0.0,
            // {tab}{tab}{tab}{tab}mean: 0.0
            // {tab}{tab}{tab}}
            // {tab}{tab}},
            // {tab}{tab}"alloc": AllocTally {
            // {tab}{tab}{tab}count: StatsSet {
            // {tab}{tab}{tab}{tab}fastest: 0.0,
            // {tab}{tab}{tab}{tab}slowest: 0.0,
            // {tab}{tab}{tab}{tab}median: 0.0,
            // {tab}{tab}{tab}{tab}mean: 0.0
            // {tab}{tab}{tab}},
            // {tab}{tab}{tab}size: StatsSet {
            // {tab}{tab}{tab}{tab}fastest: 0.0,
            // {tab}{tab}{tab}{tab}slowest: 0.0,
            // {tab}{tab}{tab}{tab}median: 0.0,
            // {tab}{tab}{tab}{tab}mean: 0.0
            // {tab}{tab}{tab}}
            // {tab}{tab}},
            // {tab}{tab}"dealloc": AllocTally {
            // {tab}{tab}{tab}count: StatsSet {
            // {tab}{tab}{tab}{tab}fastest: 0.0,
            // {tab}{tab}{tab}{tab}slowest: 0.0,
            // {tab}{tab}{tab}{tab}median: 0.0,
            // {tab}{tab}{tab}{tab}mean: 0.0
            // {tab}{tab}{tab}},
            // {tab}{tab}{tab}size: StatsSet {
            // {tab}{tab}{tab}{tab}fastest: 0.0,
            // {tab}{tab}{tab}{tab}slowest: 0.0,
            // {tab}{tab}{tab}{tab}median: 0.0,
            // {tab}{tab}{tab}{tab}mean: 0.0
            // {tab}{tab}{tab}}
            // {tab}{tab}}
            // {tab}},
            format!("{tab}}},"),
            // {tab}counts: [
            format!(r#"{tab}"counts": ["#),
            // TODO ~ kat
            format!(r#"{tab}{tab}"TODO""#),
            // {tab}{tab}None,
            // {tab}{tab}None,
            // {tab}{tab}None,
            // {tab}{tab}None
            format!("{tab}]"),
            // {tab}],
            "}".to_string(),
            // }
        ];

        for line in lines {
            if let Err(e) = writeln!(&mut self.buf, "{pad}{line}") {
                println!("Err in finish leaf with line: {line}\nError: {e:?}");
            }
        }

        self.depth -= 1;
        let pad = pad_by(self.depth);
        if let Err(e) =
            writeln!(&mut self.buf, "{pad}}}{}", if is_last { "" } else { "," })
        {
            println!("Err in finish leaf: {e:?}");
        }
    }
}
