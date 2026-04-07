use crate::alloc::{AllocOp, AllocOpMap, AllocTally};
use crate::counter::{AnyCounter, BytesFormat, KnownCounterKind};
use crate::painter::Painter;
use crate::stats::{Stats, StatsSet};
use crate::time::FineDuration;
use crate::util::fmt::{format_bytes, format_f64};
use json::JsonValue;
use std::collections::BTreeMap;

#[derive(Clone, Default, Debug)]
pub(crate) struct JsonPainter {
    debug: bool,
    final_print: bool,
    minify: bool,
    parents: Vec<String>,
    /// Top level
    data: BTreeMap<String, BenchmarkNode>,
}

impl JsonPainter {
    const SIGNIFICANT_FIGURES: usize = 4;

    pub fn new() -> Self {
        Self {
            debug: false,
            final_print: true,
            minify: false,
            ..Default::default()
        }
    }
    pub fn finalize(&self) {
        self.flush_to_disk(true);
    }

    fn flush_to_disk(&self, print_path: bool) {
        if !self.final_print || self.data.is_empty() {
            return;
        }

        // TODO(2025-06-27): What to do if the function fails?
        let first = self.data.iter().next().unwrap().0.as_str();
        let path = std::env::var("DIVAN_JSON_PATH")
            .unwrap_or_else(|_| format!("{first}.json"));
        let mut merged = Self::load_existing_json(&path);
        let current = JsonValue::try_from(self).unwrap();
        Self::merge_json_value(&mut merged, current);

        let json = if self.minify {
            json::stringify(merged)
        } else {
            json::stringify_pretty(merged, 3)
        };

        std::fs::write(&path, json.as_str()).unwrap();
        if print_path {
            let path = std::fs::canonicalize(path).unwrap();
            let path = path.to_str().unwrap();
            println!("{path}");
        }
    }

    fn load_existing_json(path: &str) -> JsonValue {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| json::parse(&contents).ok())
            .filter(JsonValue::is_object)
            .unwrap_or_else(JsonValue::new_object)
    }

    fn merge_json_value(dst: &mut JsonValue, src: JsonValue) {
        match (dst, src) {
            (JsonValue::Object(dst_obj), JsonValue::Object(src_obj)) => {
                let src = JsonValue::Object(src_obj);
                for (key, value) in src.entries() {
                    if let Some(existing) = dst_obj.get_mut(key) {
                        Self::merge_json_value(existing, value.clone());
                    } else {
                        dst_obj.insert(key, value.clone());
                    }
                }
            }
            (dst_value, src_value) => *dst_value = src_value,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum BenchmarkNode {
    // BTreeMap so the entries are always in the same order (alphabetical).
    BenchmarkParent(BTreeMap<String, BenchmarkNode>),
    // `BenchmarkChild(Option::None)` implies that the test was skipped / was empty
    BenchmarkChild(Option<Box<SerializableStats>>),
}

#[derive(Clone, Default, Debug)]
pub(crate) struct SerializableStats {
    // FixedMeasurements
    sample_count: u32,
    iter_count: u64,
    time: StatisticalMeasurements,
    // OptionalMeasurements
    // // Counters
    bytes: Option<StatisticalMeasurements>,
    chars: Option<StatisticalMeasurements>,
    cycles: Option<StatisticalMeasurements>,
    items: Option<StatisticalMeasurements>,
    // // AllocMeasurements
    max_alloc: Option<CountSizeMeasurement>,
    alloc: Option<CountSizeMeasurement>,
    dealloc: Option<CountSizeMeasurement>,
    grow: Option<CountSizeMeasurement>,
    shrink: Option<CountSizeMeasurement>,
}

#[derive(Clone, Default, Debug)]
pub(crate) struct StatisticalMeasurements {
    fastest: String,
    slowest: String,
    median: String,
    mean: String,
}

#[derive(Clone, Default, Debug)]
pub(crate) struct CountSizeMeasurement {
    count: StatisticalMeasurements,
    size: StatisticalMeasurements,
}

impl JsonPainter {
    pub fn padding(&self) -> String {
        (0..self.parents.len()).map(|_| "---|").collect()
    }

    fn get_and_init_last_parent(&mut self, line: u32) -> &mut BenchmarkNode {
        let pad = self.padding();
        let root = self.parents.first().expect("`get_and_init_last_parent` expects names to be pushed to `self.parents` before it gets called").clone();

        // special case for the root
        let root = self
            .data
            .entry(root.clone())
            .or_insert_with(|| {
                if self.debug {
                    println!(
                        "{pad}({line})[{}]Root doesn't exist, creating root: {root}",
                        line!(),
                    );
                }
                BenchmarkNode::BenchmarkParent(BTreeMap::new())
            });

        let mut x = root;

        for path in self.parents.iter().skip(1) {
            x = if let BenchmarkNode::BenchmarkParent(parent) = x {
                parent.entry(path.clone()).or_insert_with(|| {
                    if self.debug {
                        println!(
                            "{pad}({line})[{}]creating parent: {path}",
                            line!(),
                        );
                    }
                    BenchmarkNode::BenchmarkParent(BTreeMap::new())
                })
            } else {
                panic!("Tried accessing a child as parent")
            };
        }

        x
    }

    fn get_known_counter_stat(
        stats: &Stats,
        counter: KnownCounterKind,
        bytes_format: BytesFormat,
        time: &StatsSet<FineDuration>,
    ) -> Option<StatisticalMeasurements> {
        stats.get_counts(counter).map(|val| StatisticalMeasurements {
            fastest: AnyCounter::known(counter, val.fastest)
                .display_throughput(time.fastest, bytes_format)
                .to_string(),
            slowest: AnyCounter::known(counter, val.slowest)
                .display_throughput(time.slowest, bytes_format)
                .to_string(),
            median: AnyCounter::known(counter, val.median)
                .display_throughput(time.median, bytes_format)
                .to_string(),
            mean: AnyCounter::known(counter, val.mean)
                .display_throughput(time.mean, bytes_format)
                .to_string(),
        })
    }

    fn get_alloc_op_stat(
        alloc_tallies: &AllocOpMap<AllocTally<StatsSet<f64>>>,
        op: AllocOp,
        bytes_format: BytesFormat,
    ) -> Option<CountSizeMeasurement> {
        let x = alloc_tallies.get(op);

        if x.is_zero() {
            None
        } else {
            Some(CountSizeMeasurement {
                count: StatisticalMeasurements {
                    fastest: format_f64(
                        x.count.fastest,
                        Self::SIGNIFICANT_FIGURES,
                    ),
                    slowest: format_f64(
                        x.count.slowest,
                        Self::SIGNIFICANT_FIGURES,
                    ),
                    mean: format_f64(x.count.mean, Self::SIGNIFICANT_FIGURES),
                    median: format_f64(
                        x.count.median,
                        Self::SIGNIFICANT_FIGURES,
                    ),
                },
                size: StatisticalMeasurements {
                    fastest: format_bytes(
                        x.size.fastest,
                        Self::SIGNIFICANT_FIGURES,
                        bytes_format,
                    ),
                    slowest: format_bytes(
                        x.size.slowest,
                        Self::SIGNIFICANT_FIGURES,
                        bytes_format,
                    ),
                    median: format_bytes(
                        x.size.median,
                        Self::SIGNIFICANT_FIGURES,
                        bytes_format,
                    ),
                    mean: format_bytes(
                        x.size.mean,
                        Self::SIGNIFICANT_FIGURES,
                        bytes_format,
                    ),
                },
            })
        }
    }

    fn get_max_alloc_op_stat(
        max_alloc_tally: &AllocTally<StatsSet<f64>>,
        bytes_format: BytesFormat,
    ) -> Option<CountSizeMeasurement> {
        if max_alloc_tally.is_zero() {
            return None;
        }

        let x = max_alloc_tally;

        Some(CountSizeMeasurement {
            count: StatisticalMeasurements {
                fastest: format_f64(x.count.fastest, Self::SIGNIFICANT_FIGURES),
                slowest: format_f64(x.count.slowest, Self::SIGNIFICANT_FIGURES),
                mean: format_f64(x.count.mean, Self::SIGNIFICANT_FIGURES),
                median: format_f64(x.count.median, Self::SIGNIFICANT_FIGURES),
            },
            size: StatisticalMeasurements {
                fastest: format_bytes(
                    x.size.fastest,
                    Self::SIGNIFICANT_FIGURES,
                    bytes_format,
                ),
                slowest: format_bytes(
                    x.size.slowest,
                    Self::SIGNIFICANT_FIGURES,
                    bytes_format,
                ),
                median: format_bytes(
                    x.size.median,
                    Self::SIGNIFICANT_FIGURES,
                    bytes_format,
                ),
                mean: format_bytes(
                    x.size.mean,
                    Self::SIGNIFICANT_FIGURES,
                    bytes_format,
                ),
            },
        })
    }
}

impl Painter for JsonPainter {
    fn start_parent(&mut self, name: &str, is_last: bool) {
        let pad = self.padding();
        if self.debug {
            println!(
                "{pad}[{}]start_parent(name: {name:?}, is_last: {is_last})",
                line!(),
            );
        }
        self.parents.push(name.to_string());

        _ = self.get_and_init_last_parent(line!());
    }

    fn finish_parent(&mut self) {
        let parent = self
            .parents
            .pop()
            .expect("finish_parent called more than start_parent");
        let pad = self.padding();
        if self.debug {
            println!("{pad}[{}]finish_parent[{parent}]", line!());
        }
        if self.parents.is_empty() {
            self.finalize();
        }
    }

    fn ignore_leaf(&mut self, name: &str, is_last: bool) {
        let pad = self.padding();
        if self.debug {
            println!(
                "{pad}[{}]ignore_leaf(name: {name:?}, is_last: {is_last})",
                line!(),
            );
        }

        // get access to whatever is the last btreemap
        let x = match self.get_and_init_last_parent(line!()) {
            BenchmarkNode::BenchmarkParent(map) => map,
            BenchmarkNode::BenchmarkChild(_) => {
                panic!("Tried to insert into a BenchmarkChild/Leaf")
            }
        };

        x.insert(name.to_string(), BenchmarkNode::BenchmarkChild(None));
        self.flush_to_disk(false);

        if self.debug {
            println!(
                "{pad}[{}]Path: {}",
                line!(),
                self.parents
                    .iter()
                    .map(|x| format!("{x} > "))
                    .chain(std::iter::once(name.to_string()))
                    .collect::<String>()
            );
        }
    }

    fn start_leaf(&mut self, name: &str, is_last: bool) {
        let pad = self.padding();
        if self.debug {
            println!(
                "{pad}[{}]start_leaf(name: {name:?}, is_last: {is_last})",
                line!(),
            );
        }
        self.parents.push(name.to_string());

        let x = self.get_and_init_last_parent(line!());

        // TODO(2025-06-27): leaf can be created here ~kat

        _ = x;
    }

    fn finish_empty_leaf(&mut self) {
        let parent = self.parents.pop().expect(
            "finish_empty_leaf/finish_leaf called more than start_leaf",
        );
        let pad = self.padding();
        if self.debug {
            println!("{pad}[{}]finish_empty_leaf()[{parent}]", line!());
        }
        let last = self.get_and_init_last_parent(line!());
        let parent = if let BenchmarkNode::BenchmarkParent(map) = last {
            map.get_mut(&parent)
                .expect("get_and_init_last_parent should create the parent, we're just retrieving it")
        } else {
            panic!("tried to add a leaf to another leaf")
        };

        *parent = BenchmarkNode::BenchmarkChild(None);
        self.flush_to_disk(false);
    }

    fn finish_leaf(
        &mut self,
        is_last: bool,
        stats: &Stats,
        bytes_format: BytesFormat,
    ) {
        let parent = self.parents.pop().expect("get_and_init_last_parent should create the parent, we're just retrieving it");
        let pad = self.padding();
        if self.debug {
            println!(
                "{pad}[{}]finish_leaf(is_last: {is_last}, bytes_format: {}, stats: {{...}})[{parent}]", line!(), match bytes_format {
                    BytesFormat::Decimal => "Decimal",
                    BytesFormat::Binary => "Binary",
                }
            );
        }
        let last = self.get_and_init_last_parent(line!());
        let parent = if let BenchmarkNode::BenchmarkParent(map) = last {
            map.get_mut(&parent).expect("get_and_init_last_parent should create the parent, we're just retrieving it")
        } else {
            panic!("tried to add a leaf to another leaf")
        };

        let Stats {
            sample_count,
            iter_count,
            time,
            max_alloc: _,
            alloc_tallies: _,
            counts: _,
        } = stats;

        let StatsSet { fastest, slowest, median, mean } = time;

        let stats = SerializableStats {
            sample_count: *sample_count,
            iter_count: *iter_count,
            time: StatisticalMeasurements {
                fastest: fastest.to_string(),
                slowest: slowest.to_string(),
                median: median.to_string(),
                mean: mean.to_string(),
            },
            bytes: Self::get_known_counter_stat(
                stats,
                KnownCounterKind::Bytes,
                bytes_format,
                time,
            ),
            chars: Self::get_known_counter_stat(
                stats,
                KnownCounterKind::Chars,
                bytes_format,
                time,
            ),
            cycles: Self::get_known_counter_stat(
                stats,
                KnownCounterKind::Cycles,
                bytes_format,
                time,
            ),
            items: Self::get_known_counter_stat(
                stats,
                KnownCounterKind::Items,
                bytes_format,
                time,
            ),
            max_alloc: Self::get_max_alloc_op_stat(
                &stats.max_alloc,
                bytes_format,
            ),
            alloc: Self::get_alloc_op_stat(
                &stats.alloc_tallies,
                AllocOp::Alloc,
                bytes_format,
            ),
            dealloc: Self::get_alloc_op_stat(
                &stats.alloc_tallies,
                AllocOp::Dealloc,
                bytes_format,
            ),
            grow: Self::get_alloc_op_stat(
                &stats.alloc_tallies,
                AllocOp::Grow,
                bytes_format,
            ),
            shrink: Self::get_alloc_op_stat(
                &stats.alloc_tallies,
                AllocOp::Shrink,
                bytes_format,
            ),
        };

        *parent = BenchmarkNode::BenchmarkChild(Some(Box::new(stats)));
        self.flush_to_disk(false);
    }
}

// new-type so we don't have to clone the map
struct Json<'a>(&'a BTreeMap<String, BenchmarkNode>);

impl TryFrom<Json<'_>> for JsonValue {
    type Error = json::Error;

    fn try_from(value: Json) -> Result<Self, Self::Error> {
        let mut o = Self::new_object();

        for (key, value) in value.0 {
            o.insert(key, Self::try_from(value)?)?;
        }

        Ok(o)
    }
}

impl TryFrom<&JsonPainter> for JsonValue {
    type Error = json::Error;
    fn try_from(value: &JsonPainter) -> Result<Self, Self::Error> {
        Self::try_from(Json(&value.data))
    }
}

impl TryFrom<&BenchmarkNode> for JsonValue {
    type Error = json::Error;
    fn try_from(value: &BenchmarkNode) -> Result<Self, Self::Error> {
        Ok(match value {
            BenchmarkNode::BenchmarkChild(None) => Self::Null,
            BenchmarkNode::BenchmarkParent(parent) => {
                Self::try_from(Json(parent))?
            }
            BenchmarkNode::BenchmarkChild(Some(child)) => {
                Self::try_from(child.as_ref())?
            }
        })
    }
}

impl TryFrom<&SerializableStats> for JsonValue {
    type Error = json::Error;

    fn try_from(value: &SerializableStats) -> Result<Self, Self::Error> {
        let SerializableStats {
            sample_count,
            iter_count,
            time,
            bytes,
            chars,
            cycles,
            items,
            max_alloc,
            alloc,
            dealloc,
            grow,
            shrink,
        } = value;

        let mut o = Self::new_object();

        o.insert("sample_count", *sample_count)?;
        o.insert("iter_count", *iter_count)?;

        o.insert("time", Self::try_from(time)?)?;

        if let Some(bytes) = bytes {
            o.insert("bytes", Self::try_from(bytes)?)?;
        }
        if let Some(chars) = chars {
            o.insert("chars", Self::try_from(chars)?)?;
        }
        if let Some(cycles) = cycles {
            o.insert("cycles", Self::try_from(cycles)?)?;
        }
        if let Some(items) = items {
            o.insert("items", Self::try_from(items)?)?;
        }
        if let Some(max_alloc) = max_alloc {
            o.insert("max_alloc", Self::try_from(max_alloc)?)?;
        }
        if let Some(alloc) = alloc {
            o.insert("alloc", Self::try_from(alloc)?)?;
        }
        if let Some(dealloc) = dealloc {
            o.insert("dealloc", Self::try_from(dealloc)?)?;
        }
        if let Some(grow) = grow {
            o.insert("grow", Self::try_from(grow)?)?;
        }
        if let Some(shrink) = shrink {
            o.insert("shrink", Self::try_from(shrink)?)?;
        }

        Ok(o)
    }
}

impl TryFrom<&StatisticalMeasurements> for JsonValue {
    type Error = json::Error;
    fn try_from(value: &StatisticalMeasurements) -> Result<Self, Self::Error> {
        let StatisticalMeasurements { fastest, slowest, median, mean } = value;

        let mut o = Self::new_object();

        o.insert("fastest", fastest.as_str())?;
        o.insert("slowest", slowest.as_str())?;
        o.insert("median", median.as_str())?;
        o.insert("mean", mean.as_str())?;

        Ok(o)
    }
}

impl TryFrom<&CountSizeMeasurement> for JsonValue {
    type Error = json::Error;
    fn try_from(value: &CountSizeMeasurement) -> Result<Self, Self::Error> {
        let CountSizeMeasurement { count, size } = value;

        let mut o = Self::new_object();

        o.insert("count", Self::try_from(count)?)?;
        o.insert("size", Self::try_from(size)?)?;

        Ok(o)
    }
}
