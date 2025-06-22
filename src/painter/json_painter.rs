use crate::alloc::{AllocOp, AllocTally};
use crate::counter::{AnyCounter, BytesFormat, KnownCounterKind};
use crate::painter::tree_painter::{TreeColumn, TreeColumnData};
use crate::painter::Painter;
use crate::stats::Stats;
use crate::util::fmt::{format_bytes, format_f64};
use std::collections::HashMap;
use std::fmt::Write;

pub(crate) struct JsonPainter {
    depth: usize,
    // TODO(2025-06-19): does this leak memory when I call buf.clear()? ~kat
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
        // TODO(2025-06-19): should these stay? ~kat
        // Sanity checks

        // destructuring to make sure reset doesn't get out of sync with the struct
        // compiler will complain if a new field is added
        Self { depth: _, buf: _, is_last: _, parents: _, first: _ } = *self;

        // make sure the all the brackets are closed,
        // `1` because we start with the top level brackets
        assert_eq!(self.depth, 1);
        assert!(self.buf.ends_with('}'),);
        // make sure there's nothing left to pop in `is_last`
        assert!(
            self.is_last.is_empty(),
            "contents of is_last: {:?}",
            self.is_last
        );
        // make sure there's no parents left
        assert!(
            self.parents.is_empty(),
            "contents of parents: {:?}",
            self.parents
        );
        // make sure at least one entry was inserted
        assert!(self.first.is_some());

        // TODO(2025-06-22):
        //  what to do with the final output? ~kat
        //  I'd imagine if someone wants JSON then
        //  they'd expect it to be as a file somewhere.
        //  For now, store it in the cwd
        //  (examples folder if you're running the examples)
        let first = self.first.clone().unwrap();
        let path = format!("{first}_{}-lines.json", self.buf.lines().count());
        std::fs::write(&path, &self.buf).unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        let path = path.to_str().unwrap();
        println!("{path}");
        // println!("{}", self.buf);

        self.reset_unchecked();
    }

    fn reset_unchecked(&mut self) {
        // destructuring to make sure reset_unchecked doesn't get out of sync with the struct
        // compiler will complain if a new field is added
        Self { depth: _, buf: _, is_last: _, parents: _, first: _ } = *self;

        self.depth = 1;
        self.buf.clear();
        self.buf.push_str("{\n");
        self.is_last.clear();
        self.parents.clear();
        self.first = None;
    }
}

fn pad_by(n: usize) -> String {
    "\t".repeat(n)
}

impl Painter for JsonPainter {
    fn start_parent(&mut self, name: &str, is_last: bool) {
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
    fn finish_parent(&mut self) {
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
                3. my code has issues
                Possible solution:
                if the tests are a problem then add new \"finalize\" function
                which will return the buffer and reset the internal states,
                but this probably breaks the API

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
    fn ignore_leaf(&mut self, _name: &str, _is_last: bool) {
        // TODO(2025-06-19): Do nothing? ~kat
    }
    fn start_leaf(&mut self, name: &str, is_last: bool) {
        let pad = pad_by(self.depth);
        self.is_last.push(is_last);

        if let Err(e) = writeln!(&mut self.buf, r#"{pad}"{name}": {{"#) {
            println!("Err in start_leaf: {e:?}");
            return;
        }

        self.depth += 1;
    }
    fn finish_empty_leaf(&mut self) {
        // TODO(2025-06-19): Do nothing? ~kat
    }
    #[allow(clippy::too_many_lines)]
    fn finish_leaf(
        &mut self,
        _is_last: bool,
        stats: &Stats,
        bytes_format: BytesFormat,
    ) {
        let tab = pad_by(1);
        let pad = pad_by(self.depth);
        let Some(is_last) = self.is_last.pop() else {
            println!("Err popping value from is_last in finish_leaf");
            return;
        };

        // taken from tree_painter.rs to make sure the stats calculation doesn't diverge with minimal changes
        // TODO(2025-06-22): extract these functions to a module so all painters can share the calculations ~kat
        let max_alloc_counts = if stats.max_alloc.size.is_zero() {
            None
        } else {
            Some(TreeColumn::ALL.map(|column| {
                let Some(&max_alloc_count) =
                    column.get_stat(&stats.max_alloc.count)
                else {
                    return (column, String::new());
                };

                (column, format_f64(max_alloc_count, 4))
            }))
        };

        // taken from tree_painter.rs to make sure the stats calculation doesn't diverge with minimal changes
        // TODO(2025-06-22): extract these functions to a module so all painters can share the calculations ~kat
        let max_alloc_sizes = if stats.max_alloc.size.is_zero() {
            None
        } else {
            Some(TreeColumn::ALL.map(|column| {
                let Some(&max_alloc_size) =
                    column.get_stat(&stats.max_alloc.size)
                else {
                    return (column, String::new());
                };

                (column, format_bytes(max_alloc_size, 4, bytes_format))
            }))
        };

        // taken from tree_painter.rs to make sure the stats calculation doesn't diverge with minimal changes
        // TODO(2025-06-22): extract these functions to a module so all painters can share the calculations ~kat
        let serialized_alloc_tallies = AllocOp::ALL.map(|op| {
            let tally = stats.alloc_tallies.get(op);

            if tally.is_zero() {
                return None;
            }

            let column_tallies = TreeColumn::ALL.map(|column| {
                let tally = AllocTally {
                    count: column.get_stat(&tally.count).copied()?,
                    size: column.get_stat(&tally.size).copied()?,
                };

                Some((column, tally))
            });

            Some(AllocTally {
                count: column_tallies.map(|tally| {
                    if let Some((column, tally)) = tally {
                        Some((
                            column,
                            format_bytes(tally.count, 4, bytes_format),
                        ))
                    } else {
                        None
                    }
                }),
                size: column_tallies.map(|tally| {
                    if let Some((column, tally)) = tally {
                        Some((
                            column,
                            format_bytes(tally.size, 4, bytes_format),
                        ))
                    } else {
                        None
                    }
                }),
            })
        });

        // taken from tree_painter.rs to make sure the stats calculation doesn't diverge with minimal changes
        // TODO(2025-06-22): extract these functions to a module so all painters can share the calculations ~kat
        let serialized_counters = KnownCounterKind::ALL.map(|counter_kind| {
            let counter_stats = stats.get_counts(counter_kind);

            TreeColumn::ALL.map(|column| {
                let count = *column.get_stat(counter_stats?)?;
                let time = *column.get_stat(&stats.time)?;

                Some((
                    counter_kind,
                    column,
                    AnyCounter::known(counter_kind, count)
                        .display_throughput(time, bytes_format)
                        .to_string(),
                ))
            })
        });

        // TODO(2025-06-19):
        //  I wanted to avoid including heavy dependencies like serde
        //  I'm using https://docs.rs/json/latest/json/ to make sure
        //  the manually created json is valid... by parsing it >.>
        //  This feels unmaintainable, what to do?
        //  ~kat
        let mut lines = vec![
            // stats:{
            //  sample_count: {},
            //  iter_count: {},
            //  time: {
            //      fastest: {},
            //      slowest: {},
            //      median: {},
            //      mean: {}
            //  } // comma will be added by the next entry if it exists
            r#""stats": {"#.to_string(),
            format!(r#"{tab}"sample_count": {},"#, stats.sample_count),
            format!(r#"{tab}"iter_count": {},"#, stats.iter_count),
            format!(r#"{tab}"time": {{"#),
            format!(r#"{tab}{tab}"fastest": "{}","#, stats.time.fastest),
            format!(r#"{tab}{tab}"slowest": "{}","#, stats.time.slowest),
            format!(r#"{tab}{tab}"median": "{}","#, stats.time.median),
            format!(r#"{tab}{tab}"mean": "{}""#, stats.time.mean),
            format!(r"{tab}}}"),
        ];

        if max_alloc_counts.is_some() || max_alloc_sizes.is_some() {
            add_comma_if_required(&mut lines);
            // stats.max_alloc
            lines.push(format!(r#"{tab}"max_alloc": {{"#));

            if let Some(max_alloc_counts) = max_alloc_counts {
                add_comma_if_required(&mut lines);

                // stats.max_alloc.count
                lines.push(format!(r#"{tab}{tab}"count": {{"#));

                let counts = max_alloc_counts
                    .iter()
                    .filter(|(_, x)| !x.is_empty())
                    .collect::<Vec<_>>();

                for (col, val) in &counts {
                    add_comma_if_required(&mut lines);
                    lines.push(format!(
                        r#"{tab}{tab}{tab}"{}": "{val}""#,
                        col.name(),
                    ));
                }

                // end count
                lines.push(format!("{tab}{tab}}}"));
            }

            if let Some(max_alloc_sizes) = max_alloc_sizes {
                add_comma_if_required(&mut lines);

                // stats.max_alloc.sizes
                lines.push(format!(r#"{tab}{tab}"sizes": {{"#));

                let sizes = max_alloc_sizes
                    .iter()
                    .filter(|(_, x)| !x.is_empty())
                    .collect::<Vec<_>>();

                for (col, s) in &sizes {
                    add_comma_if_required(&mut lines);
                    lines.push(format!(
                        r#"{tab}{tab}{tab}"{}": "{s}""#,
                        col.name(),
                    ));
                }

                // end sizes
                lines.push(format!("{tab}{tab}}}"));
            }

            // end max_alloc
            lines.push(format!("{tab}}}"));
        }
        for alloc_tally in serialized_alloc_tallies.iter().flatten() {
            let AllocTally { count, size } = alloc_tally;

            // make sure at least one entry exists
            // stats.alloc
            if count
                .iter()
                .zip(size.iter())
                .any(|(a, b)| a.is_some() || b.is_some())
            {
                add_comma_if_required(&mut lines);

                // TODO(2025-06-22):
                //  hash, sort, string have duplicate "alloc"s,
                //  and they can have different information,
                //  im not sure why dups exist
                //  and idk why the json crate isn't throwing an error
                //  The second alloc is dealloc?
                //  `[AllocOp::Alloc, AllocOp::Dealloc, AllocOp::Grow, AllocOp::Shrink]`
                //  ~kat
                lines.push(format!(r#"{tab}"alloc": {{"#));

                // stats.alloc.count
                if count.iter().any(Option::is_some) {
                    add_comma_if_required(&mut lines);
                    lines.push(format!(r#"{tab}{tab}"count": {{"#));

                    let count = count.iter().flatten().collect::<Vec<_>>();
                    for (column, val) in &count {
                        add_comma_if_required(&mut lines);
                        lines.push(format!(
                            r#"{tab}{tab}{tab}"{}": "{val}""#,
                            column.name(),
                        ));
                    }

                    // end count
                    lines.push(format!("{tab}{tab}}}"));
                }

                // stats.alloc.size
                if size.iter().any(Option::is_some) {
                    add_comma_if_required(&mut lines);

                    lines.push(format!(r#"{tab}{tab}"size": {{"#));

                    let size = size.iter().flatten().collect::<Vec<_>>();
                    for (column, val) in &size {
                        add_comma_if_required(&mut lines);
                        lines.push(format!(
                            r#"{tab}{tab}{tab}"{}": "{val}""#,
                            column.name(),
                        ));
                    }

                    // end size
                    lines.push(format!("{tab}{tab}}}"));
                }

                // // Write time stats with iter and sample counts.
                // TreeColumnData::from_fn(|column| -> String {
                //     let stat: &dyn ToString = match column {
                //         TreeColumn::Fastest => &stats.time.fastest,
                //         TreeColumn::Slowest => &stats.time.slowest,
                //         TreeColumn::Median => &stats.time.median,
                //         TreeColumn::Mean => &stats.time.mean,
                //         TreeColumn::Samples => &stats.sample_count,
                //         TreeColumn::Iters => &stats.iter_count,
                //     };
                //     stat.to_string()
                // })
                // .as_ref::<str>()
                // .write(buf, &mut self.column_widths);
                //
                // println!("{buf}");

                // // Write counter stats.
                // let counter_stats = serialized_counters.map(TreeColumnData);
                // for counter_kind in KnownCounterKind::ALL {
                //     let counter_stats =
                //         counter_stats[counter_kind as usize].as_ref::<str>();
                //
                //     // Skip empty rows.
                //     if counter_stats.0.iter().all(|s| s.is_empty()) {
                //         continue;
                //     }
                //
                //     prep_buffer(buf, &mut self.max_name_span);
                //
                //     counter_stats.write(buf, &mut self.column_widths);
                //     println!("{buf}");
                // }

                // // Write max allocated bytes.
                // if serialized_max_alloc_counts.is_some()
                //     || serialized_max_alloc_sizes.is_some()
                // {
                //     prep_buffer(buf, &mut self.max_name_span);
                //
                //     TreeColumnData::from_first("max alloc:")
                //         .write(buf, &mut self.column_widths);
                //     println!("{buf}");
                //
                //     for serialized in [
                //         serialized_max_alloc_counts.as_ref(),
                //         serialized_max_alloc_sizes.as_ref(),
                //     ]
                //         .into_iter()
                //         .flatten()
                //     {
                //         prep_buffer(buf, &mut self.max_name_span);
                //
                //         TreeColumnData::from_fn(|column| {
                //             serialized[column as usize].as_str()
                //         })
                //             .write(buf, &mut self.column_widths);
                //
                //         println!("{buf}");
                //     }
                // }

                // // Write allocation tallies.
                // for op in
                //     [AllocOp::Alloc, AllocOp::Dealloc, AllocOp::Grow, AllocOp::Shrink]
                // {
                //     let Some(tallies) = &serialized_alloc_tallies[op as usize] else {
                //         continue;
                //     };
                //
                //     prep_buffer(buf, &mut self.max_name_span);
                //
                //     TreeColumnData::from_first(op.prefix())
                //         .write(buf, &mut self.column_widths);
                //     println!("{buf}");
                //
                //     for value in tallies.as_array() {
                //         prep_buffer(buf, &mut self.max_name_span);
                //
                //         TreeColumnData::from_fn(|column| {
                //             value[column as usize].as_str()
                //         })
                //             .write(buf, &mut self.column_widths);
                //
                //         println!("{buf}");
                //     }
                // }

                // end alloc
                lines.push(format!("{tab}}}"));
            }
        }

        if serialized_counters.iter().any(|x| x.iter().any(Option::is_some)) {
            let counters = serialized_counters
                .into_iter()
                .flatten()
                .flatten()
                .map(|(x, a, b)| (x.to_string(), (a.name(), b)))
                .fold(HashMap::new(), |mut map: HashMap<_, Vec<_>>, (k, v)| {
                    map.entry(k).or_default().push(v);

                    map
                });

            for (counter, values) in counters {
                add_comma_if_required(&mut lines);

                // stats.{counter}
                lines.push(format!(r#"{tab}"{counter}": {{"#));

                for (column, value) in values {
                    add_comma_if_required(&mut lines);
                    lines.push(format!(r#"{tab}{tab}"{column}": "{value}""#));
                }

                // end {counter}
                lines.push(format!("{tab}}}"));
            }
        }

        // end stats
        lines.push(String::from("}"));

        let lines = lines.iter().map(|s| format!("{pad}{s}\n"));

        self.buf.extend(lines);

        self.depth -= 1;
        let pad = pad_by(self.depth);
        if let Err(e) =
            writeln!(&mut self.buf, "{pad}}}{}", if is_last { "" } else { "," })
        {
            println!("Err in finish leaf: {e:?}");
        }
    }
}

fn add_comma_if_required(lines: &mut [String]) {
    let len = lines.len() - 1;

    let last =
        lines.get_mut(len).expect("Lines should have at least one entry");
    if last.ends_with('{') {
    } else if !last.ends_with(',') {
        last.push(',');
    }
}
