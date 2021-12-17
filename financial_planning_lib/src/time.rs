use strum_macros::EnumString;

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum Month {
    January,
    February,
    March,
    April,
    May,
    June,
    July,
    August,
    September,
    October,
    November,
    December,
}

impl Month {
    fn num(&self) -> u32 {
        match self {
            Self::January => 0,
            Self::February => 1,
            Self::March => 2,
            Self::April => 3,
            Self::May => 4,
            Self::June => 5,
            Self::July => 6,
            Self::August => 7,
            Self::September => 8,
            Self::October => 9,
            Self::November => 10,
            Self::December => 11,
        }
    }
}

impl TimeNext for Month {
    fn next(&self) -> Month {
        match self {
            Self::January => Self::February,
            Self::February => Self::March,
            Self::March => Self::April,
            Self::April => Self::May,
            Self::May => Self::June,
            Self::June => Self::July,
            Self::July => Self::August,
            Self::August => Self::September,
            Self::September => Self::October,
            Self::October => Self::November,
            Self::November => Self::December,
            Self::December => Self::January,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Year(pub u32);

impl Year {
    pub fn months(&self) -> Vec<Time> {
        TimeRange {
            // Iterator is inclusive of start
            start: Time {
                year: self.clone(),
                month: Month::January,
            },
            // Iterator is exclusive of end
            end: Time {
                year: Year(self.0 + 1),
                month: Month::January,
            },
        }
        .into_iter()
        .collect()
    }
}

impl TimeNext for Year {
    fn next(&self) -> Year {
        Year(self.0 + 1)
    }
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct Time {
    pub year: Year,
    pub month: Month,
}

impl TimeNext for Time {
    fn next(&self) -> Self {
        Self {
            year: match self.month {
                Month::December => Year(self.year.0 + 1),
                _ => self.year.clone(),
            },
            month: self.month.next(),
        }
    }
}

impl core::ops::Sub for &Time {
    type Output = Months;

    fn sub(self, rhs: Self) -> Self::Output {
        Months(
            i64::from(self.year.0 * 12 + self.month.num())
                - i64::from(rhs.year.0 * 12 + rhs.month.num()),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Months(pub i64);

impl Months {
    pub fn even_freq(&self, freq: &Frequency) -> bool {
        match freq {
            Frequency::Monthly => true,
            Frequency::Quarterly => self.0 % 3 == 0,
            Frequency::Yearly => self.0 % 12 == 0,
        }
    }
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum Frequency {
    Monthly,
    Quarterly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct TimeRange<T: TimeNext> {
    pub start: T,
    pub end: T,
}

pub trait TimeNext: Clone + PartialOrd {
    fn next(&self) -> Self;
}

impl<'a, T: TimeNext> IntoIterator for &'a TimeRange<T> {
    type Item = T;
    type IntoIter = TimeRangeIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        TimeRangeIter {
            range: self,
            current: self.start.clone(),
        }
    }
}

pub struct TimeRangeIter<'a, T: TimeNext> {
    range: &'a TimeRange<T>,
    current: T,
}

impl<'a, T: TimeNext> Iterator for TimeRangeIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.range.end {
            let out = self.current.clone();
            self.current = self.current.next();
            Some(out)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_month_basics() -> Result<()> {
        assert_eq!(Month::January, Month::January);
        assert_ne!(Month::January, Month::July);

        assert_eq!(Month::January.num(), 0);
        assert_eq!(Month::July.num(), 6);
        assert_eq!(Month::December.num(), 11);

        assert_eq!(Month::January.next(), Month::February);
        assert_eq!(Month::July.next(), Month::August);
        assert_eq!(Month::December.next(), Month::January);

        Ok(())
    }

    #[test]
    fn test_year_basics() -> Result<()> {
        assert_eq!(Year(2021), Year(2021));
        assert_ne!(Year(2021), Year(2022));

        assert_eq!(
            Year(2021).months(),
            vec![
                Time {
                    year: Year(2021),
                    month: Month::January
                },
                Time {
                    year: Year(2021),
                    month: Month::February
                },
                Time {
                    year: Year(2021),
                    month: Month::March
                },
                Time {
                    year: Year(2021),
                    month: Month::April
                },
                Time {
                    year: Year(2021),
                    month: Month::May
                },
                Time {
                    year: Year(2021),
                    month: Month::June
                },
                Time {
                    year: Year(2021),
                    month: Month::July
                },
                Time {
                    year: Year(2021),
                    month: Month::August
                },
                Time {
                    year: Year(2021),
                    month: Month::September
                },
                Time {
                    year: Year(2021),
                    month: Month::October
                },
                Time {
                    year: Year(2021),
                    month: Month::November
                },
                Time {
                    year: Year(2021),
                    month: Month::December
                },
            ]
        );

        assert_eq!(Year(2021).next(), Year(2022));

        Ok(())
    }

    #[test]
    fn test_time_basics() -> Result<()> {
        assert_eq!(
            Time {
                year: Year(2021),
                month: Month::July
            },
            Time {
                year: Year(2021),
                month: Month::July
            },
        );
        assert_ne!(
            Time {
                year: Year(2021),
                month: Month::July
            },
            Time {
                year: Year(2022),
                month: Month::July
            },
        );
        assert_ne!(
            Time {
                year: Year(2021),
                month: Month::July
            },
            Time {
                year: Year(2021),
                month: Month::January
            },
        );

        assert_eq!(
            Time {
                year: Year(2021),
                month: Month::July
            }
            .next(),
            Time {
                year: Year(2021),
                month: Month::August
            },
        );
        assert_eq!(
            Time {
                year: Year(2021),
                month: Month::December
            }
            .next(),
            Time {
                year: Year(2022),
                month: Month::January
            }
        );

        Ok(())
    }

    #[test]
    fn test_time_ops() -> Result<()> {
        assert_eq!(
            &Time {
                year: Year(2021),
                month: Month::January
            } - &Time {
                year: Year(2021),
                month: Month::January
            },
            Months(0),
        );
        assert_eq!(
            &Time {
                year: Year(2021),
                month: Month::February
            } - &Time {
                year: Year(2021),
                month: Month::January
            },
            Months(1),
        );
        assert_eq!(
            &Time {
                year: Year(2021),
                month: Month::July
            } - &Time {
                year: Year(2021),
                month: Month::January
            },
            Months(6),
        );
        assert_eq!(
            &Time {
                year: Year(2022),
                month: Month::July
            } - &Time {
                year: Year(2021),
                month: Month::July
            },
            Months(12),
        );
        assert_eq!(
            &Time {
                year: Year(2021),
                month: Month::July
            } - &Time {
                year: Year(2022),
                month: Month::July
            },
            Months(-12),
        );
        assert_eq!(
            &Time {
                year: Year(2022),
                month: Month::January
            } - &Time {
                year: Year(2021),
                month: Month::December
            },
            Months(1),
        );
        assert_eq!(
            &Time {
                year: Year(2022),
                month: Month::March
            } - &Time {
                year: Year(2021),
                month: Month::December
            },
            Months(3),
        );

        Ok(())
    }

    #[test]
    fn test_months() -> Result<()> {
        assert_eq!(true, Months(0).even_freq(&Frequency::Monthly));
        assert_eq!(true, Months(0).even_freq(&Frequency::Quarterly));
        assert_eq!(true, Months(0).even_freq(&Frequency::Yearly));

        assert_eq!(true, Months(1).even_freq(&Frequency::Monthly));
        assert_eq!(false, Months(1).even_freq(&Frequency::Quarterly));
        assert_eq!(false, Months(1).even_freq(&Frequency::Yearly));

        assert_eq!(true, Months(3).even_freq(&Frequency::Monthly));
        assert_eq!(true, Months(3).even_freq(&Frequency::Quarterly));
        assert_eq!(false, Months(3).even_freq(&Frequency::Yearly));

        assert_eq!(true, Months(12).even_freq(&Frequency::Monthly));
        assert_eq!(true, Months(12).even_freq(&Frequency::Quarterly));
        assert_eq!(true, Months(12).even_freq(&Frequency::Yearly));

        Ok(())
    }

    #[test]
    fn test_time_range_year() -> Result<()> {
        let tr = TimeRange {
            start: Year(0),
            end: Year(5),
        };

        let items: Vec<Year> = tr.into_iter().collect();
        assert_eq!(items, vec![Year(0), Year(1), Year(2), Year(3), Year(4)]);

        let tr = TimeRange {
            start: Year(0),
            end: Year(0),
        };

        let items: Vec<Year> = tr.into_iter().collect();
        assert_eq!(items, vec![]);

        let tr = TimeRange {
            start: Year(10),
            end: Year(0),
        };

        let items: Vec<Year> = tr.into_iter().collect();
        assert_eq!(items, vec![]);

        Ok(())
    }

    #[test]
    fn test_time_range_month() -> Result<()> {
        let tr = TimeRange {
            start: Month::May,
            end: Month::August,
        };

        let items: Vec<Month> = tr.into_iter().collect();
        assert_eq!(items, vec![Month::May, Month::June, Month::July],);

        let tr = TimeRange {
            start: Month::June,
            end: Month::June,
        };

        let items: Vec<Month> = tr.into_iter().collect();
        assert_eq!(items, vec![]);

        let tr = TimeRange {
            start: Month::December,
            end: Month::January,
        };

        let items: Vec<Month> = tr.into_iter().collect();
        assert_eq!(items, vec![]);

        Ok(())
    }

    #[test]
    fn test_time_range_time() -> Result<()> {
        let tr = TimeRange {
            start: Time {
                year: Year(2021),
                month: Month::November,
            },
            end: Time {
                year: Year(2022),
                month: Month::March,
            },
        };

        let items: Vec<Time> = tr.into_iter().collect();
        assert_eq!(
            items,
            vec![
                Time {
                    year: Year(2021),
                    month: Month::November
                },
                Time {
                    year: Year(2021),
                    month: Month::December
                },
                Time {
                    year: Year(2022),
                    month: Month::January
                },
                Time {
                    year: Year(2022),
                    month: Month::February
                },
            ]
        );

        let tr = TimeRange {
            start: Time {
                year: Year(2021),
                month: Month::November,
            },
            end: Time {
                year: Year(2021),
                month: Month::November,
            },
        };

        let items: Vec<Time> = tr.into_iter().collect();
        assert_eq!(items, vec![]);

        let tr = TimeRange {
            start: Time {
                year: Year(2022),
                month: Month::March,
            },
            end: Time {
                year: Year(2021),
                month: Month::November,
            },
        };

        let items: Vec<Time> = tr.into_iter().collect();
        assert_eq!(items, vec![]);

        Ok(())
    }
}
