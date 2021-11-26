use anyhow::{anyhow, Context, Result};

use crate::time::{TimeNext, TimeRange};

type Ranges<T, V> = Vec<(TimeRange<T>, V)>;

#[derive(Debug, Clone)]
pub struct LookupTable<T: TimeNext, V> {
    ranges: Ranges<T, V>,
}

impl<T: TimeNext + std::cmp::Ord + std::fmt::Debug, V: Clone + std::fmt::Debug> LookupTable<T, V> {
    pub fn new(ranges: Ranges<T, V>) -> Result<Self> {
        let out = Self {
            ranges: Self::validate_contiguous_ranges(ranges)
                .context("Failed to validate ranges were contiguious")?,
        };
        Ok(out)
    }

    pub fn range(&self) -> TimeRange<T> {
        let mut iter = self.ranges.iter();
        // We validated there is at least 1 element on construction
        let first = iter.next().unwrap();
        let start = &first.0.start;
        let mut end = &first.0.end;

        for time in iter {
            end = &time.0.end;
        }

        TimeRange {
            start: start.clone(),
            end: end.clone(),
        }
    }

    pub fn value_at(&self, time: &T) -> Result<V> {
        for (t, value) in &self.ranges {
            if &t.start <= time && &t.end > time {
                return Ok(value.clone());
            }
        }

        Err(anyhow!(
            "Time {:?} was not within our range {:?}",
            time,
            self.range()
        ))
    }

    fn validate_contiguous_ranges(mut ranges: Ranges<T, V>) -> Result<Ranges<T, V>> {
        if ranges.is_empty() {
            return Err(anyhow!("Got empty ranges, which isn't allowed"));
        }
        ranges.sort_unstable_by_key(|(r, _)| r.start.clone());

        let mut prev: Option<&T> = None;
        for (i, (range, _)) in itertools::enumerate(ranges.iter()) {
            if range.start > range.end {
                return Err(anyhow!("Table entry {} has end > start", i));
            } else if range.start == range.end {
                return Err(anyhow!("Table entry {} has empty range (end == start)", i));
            }

            if let Some(prev) = prev {
                if prev != &range.start {
                    return Err(anyhow!(
                        "Table has non-contiguious range. {} starts at {:?} but previous entry ends at {:?}",
                        i, range.start, prev
                    ));
                }
            }
            prev = Some(&range.end);
        }
        Ok(ranges)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;

    use crate::time::Year;

    #[test]
    fn test_validation() -> Result<()> {
        LookupTable::new(vec![(
            TimeRange {
                start: Year(1),
                end: Year(10),
            },
            1 as i64,
        )])
        .context("incorrectly rejected single entry range")?;

        LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(1),
                    end: Year(2),
                },
                1 as i64,
            ),
            (
                TimeRange {
                    start: Year(2),
                    end: Year(3),
                },
                2 as i64,
            ),
        ])
        .context("incorrectly rejected ordered 2 single year entries")?;

        LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(1),
                    end: Year(5),
                },
                1 as i64,
            ),
            (
                TimeRange {
                    start: Year(5),
                    end: Year(10),
                },
                2 as i64,
            ),
        ])
        .context("incorrectly rejected ordered 2 multi-year entries")?;

        LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(1),
                    end: Year(2),
                },
                1 as i64,
            ),
            (
                TimeRange {
                    start: Year(2),
                    end: Year(5),
                },
                2 as i64,
            ),
            (
                TimeRange {
                    start: Year(5),
                    end: Year(10),
                },
                3 as i64,
            ),
        ])
        .context("incorrectly rejected ordered 3 varied length entries")?;

        LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(2),
                    end: Year(3),
                },
                2 as i64,
            ),
            (
                TimeRange {
                    start: Year(1),
                    end: Year(2),
                },
                1 as i64,
            ),
        ])
        .context("incorrectly rejected unordered 2 single year entries")?;

        LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(5),
                    end: Year(10),
                },
                2 as i64,
            ),
            (
                TimeRange {
                    start: Year(1),
                    end: Year(5),
                },
                1 as i64,
            ),
        ])
        .context("incorrectly rejected unordered 2 multi-year entries")?;

        LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(2),
                    end: Year(5),
                },
                2 as i64,
            ),
            (
                TimeRange {
                    start: Year(1),
                    end: Year(2),
                },
                1 as i64,
            ),
            (
                TimeRange {
                    start: Year(5),
                    end: Year(10),
                },
                3 as i64,
            ),
        ])
        .context("incorrectly rejected unordered 3 varied length entries")?;

        assert!(LookupTable::<Year, u64>::new(vec![]).is_err());

        assert!(LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(1),
                    end: Year(2)
                },
                1 as i64
            ),
            (
                TimeRange {
                    start: Year(3),
                    end: Year(4)
                },
                2 as i64
            ),
        ])
        .is_err());

        assert!(LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(1),
                    end: Year(10)
                },
                1 as i64
            ),
            (
                TimeRange {
                    start: Year(3),
                    end: Year(4)
                },
                2 as i64
            ),
        ])
        .is_err());

        assert!(LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(1),
                    end: Year(5)
                },
                1 as i64
            ),
            (
                TimeRange {
                    start: Year(4),
                    end: Year(6)
                },
                2 as i64
            ),
            (
                TimeRange {
                    start: Year(5),
                    end: Year(10)
                },
                2 as i64
            ),
        ])
        .is_err());

        Ok(())
    }

    #[test]
    fn test_range() -> Result<()> {
        assert_eq!(
            LookupTable::new(vec![(
                TimeRange {
                    start: Year(1),
                    end: Year(10)
                },
                1 as i64
            ),])
            .unwrap()
            .range(),
            TimeRange {
                start: Year(1),
                end: Year(10)
            }
        );

        assert_eq!(
            LookupTable::new(vec![
                (
                    TimeRange {
                        start: Year(2),
                        end: Year(5)
                    },
                    2 as i64
                ),
                (
                    TimeRange {
                        start: Year(1),
                        end: Year(2)
                    },
                    1 as i64
                ),
            ])
            .unwrap()
            .range(),
            TimeRange {
                start: Year(1),
                end: Year(5)
            }
        );

        assert_eq!(
            LookupTable::new(vec![
                (
                    TimeRange {
                        start: Year(5),
                        end: Year(12)
                    },
                    2 as i64
                ),
                (
                    TimeRange {
                        start: Year(1),
                        end: Year(5)
                    },
                    1 as i64
                ),
            ])
            .unwrap()
            .range(),
            TimeRange {
                start: Year(1),
                end: Year(12)
            }
        );

        assert_eq!(
            LookupTable::new(vec![
                (
                    TimeRange {
                        start: Year(2),
                        end: Year(5)
                    },
                    2 as i64
                ),
                (
                    TimeRange {
                        start: Year(1),
                        end: Year(2)
                    },
                    1 as i64
                ),
                (
                    TimeRange {
                        start: Year(5),
                        end: Year(13)
                    },
                    3 as i64
                ),
            ])
            .unwrap()
            .range(),
            TimeRange {
                start: Year(1),
                end: Year(13)
            }
        );

        Ok(())
    }

    #[test]
    fn test_value_at() -> Result<()> {
        let r = LookupTable::new(vec![
            (
                TimeRange {
                    start: Year(2),
                    end: Year(5),
                },
                2 as i64,
            ),
            (
                TimeRange {
                    start: Year(1),
                    end: Year(2),
                },
                1 as i64,
            ),
            (
                TimeRange {
                    start: Year(5),
                    end: Year(13),
                },
                3 as i64,
            ),
        ])
        .unwrap();

        assert!(r.value_at(&Year(0)).is_err());
        assert_eq!(r.value_at(&Year(1)).unwrap(), 1);
        assert_eq!(r.value_at(&Year(2)).unwrap(), 2);
        assert_eq!(r.value_at(&Year(3)).unwrap(), 2);
        assert_eq!(r.value_at(&Year(4)).unwrap(), 2);
        assert_eq!(r.value_at(&Year(5)).unwrap(), 3);
        assert_eq!(r.value_at(&Year(6)).unwrap(), 3);
        assert_eq!(r.value_at(&Year(8)).unwrap(), 3);
        assert_eq!(r.value_at(&Year(12)).unwrap(), 3);
        assert!(r.value_at(&Year(13)).is_err());

        Ok(())
    }
}
