use anyhow::{Context, Result};

use crate::asset::{CategoryValue, Money, Rate, Tx};
use crate::lookup_table::LookupTable;
use crate::tax::TaxPolicy;
use crate::time::{Frequency, Time};

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct FlowName(pub String);

#[derive(Debug)]
pub struct Flow {
    pub name: FlowName,
    pub description: String,
    pub start: Time,
    pub end: Time,
    pub frequency: Frequency,
    pub value: Box<dyn FlowValue>,
    pub tax_policy: Box<dyn TaxPolicy>,
}

impl Flow {
    pub fn calculate_transaction(&self, category: &CategoryValue, time: &Time) -> Result<Tx> {
        let gross = self
            .value
            .value_at(&time, self, category)
            .context("Failed to get value for flow")?;
        let (net, tax_tx) = self.tax_policy.calculate_tax(gross);

        Ok(Tx {
            time: time.clone(),
            amount: net,
            tax_tx,
        })
    }
}

pub trait FlowValue: std::fmt::Debug {
    fn applies_at(&self, time: &Time, flow: &Flow) -> bool {
        if time < &flow.start || time >= &flow.end {
            false
        } else {
            (time - &flow.start).even_freq(&flow.frequency)
        }
    }

    fn value_at(&self, time: &Time, flow: &Flow, category: &CategoryValue) -> Result<Money>;
}

#[derive(Debug)]
pub struct FixedFlow {
    pub value: Money,
}

impl FlowValue for FixedFlow {
    fn value_at(&self, _: &Time, _: &Flow, _: &CategoryValue) -> Result<Money> {
        Ok(self.value)
    }
}

#[derive(Debug)]
pub struct RateFlow {
    pub rate: Rate,
}

impl FlowValue for RateFlow {
    fn value_at(&self, _: &Time, _: &Flow, category: &CategoryValue) -> Result<Money> {
        Ok(category.value().at_rate(self.rate))
    }
}

#[derive(Debug)]
pub struct TableFlow {
    pub table: LookupTable<Time, Money>,
}

impl FlowValue for TableFlow {
    fn value_at(&self, time: &Time, _: &Flow, _: &CategoryValue) -> Result<Money> {
        self.table
            .value_at(time)
            .context("failed to get rate from table")
    }
}

#[derive(Debug)]
pub struct RateTableFlow {
    pub table: LookupTable<Time, Rate>,
}

impl FlowValue for RateTableFlow {
    fn value_at(&self, time: &Time, _: &Flow, category: &CategoryValue) -> Result<Money> {
        Ok(category.value().at_rate(
            self.table
                .value_at(time)
                .context("failed to get rate from table")?,
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;

    use crate::asset::{Asset, AssetName, Category, CategoryName};
    use crate::tax::{TaxPolicy, TaxTx};
    use crate::time::{Month, Time, TimeNext, TimeRange, Year};

    #[derive(Debug)]
    struct MockTax {}
    impl TaxPolicy for MockTax {
        fn calculate_tax(&self, gross: Money) -> (Money, TaxTx) {
            (
                // We subtract one to assert that this gets called and it's outcome is
                // applied correctly
                gross - Money::from_dollars(1),
                // We also base our taxable income and withholding off the input for the same
                // reason
                TaxTx {
                    taxable_income: gross,
                    tax_withheld: gross - Money::from_dollars(10),
                },
            )
        }

        fn tax_withheld(&self, _: Money) -> TaxTx {
            panic!("Not implement for mock");
        }
    }

    fn test_flow() -> Flow {
        Flow {
            name: FlowName("test".to_string()),
            description: "A unit test flow".to_string(),
            start: Time {
                year: Year(2021),
                month: Month::July,
            },
            end: Time {
                year: Year(2022),
                month: Month::July,
            },
            frequency: Frequency::Monthly,
            value: Box::new(FixedFlow {
                value: Money::from_dollars(123),
            }),
            tax_policy: Box::new(MockTax {}),
        }
    }

    fn test_value<F: FlowValue>(
        fv: &F,
        test_flow: &Flow,
        time: &Time,
        asset_value: Money,
        expected_value: Money,
    ) -> Result<()> {
        assert_eq!(
            (
                &time,
                asset_value,
                fv.value_at(
                    &time,
                    &test_flow,
                    &Category::from_assets(
                        CategoryName("unittest".to_string()),
                        vec![Asset {
                            name: AssetName("unit test asset".to_string()),
                            value: asset_value,
                        }]
                    )
                    .value(),
                )
                .unwrap()
            ),
            (&time, asset_value, expected_value),
        );
        Ok(())
    }

    enum TestType {
        // This only varies by time not value
        ByTime(Vec<(Time, Money)>),
        // This only varies by asset value not time
        ByValue(Vec<(Money, Money)>),
        // Varies by both asset value and time
        ByBoth(Vec<(Time, Money, Money)>),
    }

    fn verify_value_at<F: FlowValue>(fv: &F, test_flow: &Flow, test: TestType) -> Result<()> {
        // Test variance by time
        match test {
            TestType::ByTime(cases) => {
                for (time, expected_value) in cases {
                    test_value(fv, test_flow, &time, Money::from_dollars(0), expected_value)?;
                    test_value(
                        fv,
                        test_flow,
                        &time,
                        Money::from_dollars(200),
                        expected_value,
                    )?;
                }
            }
            TestType::ByValue(cases) => {
                for (asset_value, expected_value) in cases {
                    test_value(fv, test_flow, &test_flow.start, asset_value, expected_value)?;
                    test_value(
                        fv,
                        test_flow,
                        &test_flow.start.next().next().next(),
                        asset_value,
                        expected_value,
                    )?;
                }
            }
            TestType::ByBoth(cases) => {
                for (time, asset_value, expected_value) in cases {
                    test_value(fv, test_flow, &time, asset_value, expected_value)?;
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_flow_basics() -> Result<()> {
        let f = test_flow();

        let out = f
            .calculate_transaction(
                &Category::from_assets(CategoryName("unittest".to_string()), vec![]).value(),
                &f.start,
            )
            .unwrap();

        assert_eq!(out.time, f.start);
        assert_eq!(out.amount, Money::from_dollars(123 - 1));
        assert_eq!(out.tax_tx.taxable_income, Money::from_dollars(123));
        assert_eq!(out.tax_tx.tax_withheld, Money::from_dollars(113));

        Ok(())
    }

    fn test_applies_at<T: FlowValue>(fv: &T) -> Result<()> {
        let mut f = test_flow();

        let pre_start = Time {
            year: Year(2021),
            month: Month::January,
        };
        let start = pre_start.next();
        let end = Time {
            year: Year(2022),
            month: Month::July,
        };

        f.start = start.clone();
        f.end = end.clone();
        f.frequency = Frequency::Monthly;

        // Start is always inclusive
        assert_eq!(fv.applies_at(&start, &f), true);
        // end is always exclusive
        assert_eq!(fv.applies_at(&end, &f), false);

        // Before start and after end shouldn't apply either
        assert_eq!(fv.applies_at(&pre_start, &f), false);
        assert_eq!(fv.applies_at(&end.next(), &f), false);

        // For monthly everything should apply
        assert_eq!(fv.applies_at(&start.next(), &f), true);
        assert_eq!(fv.applies_at(&start.next().next(), &f), true);

        f.frequency = Frequency::Quarterly;
        assert_eq!(fv.applies_at(&start, &f), true);
        assert_eq!(fv.applies_at(&start.next(), &f), false);
        assert_eq!(fv.applies_at(&start.next().next(), &f), false);
        assert_eq!(fv.applies_at(&start.next().next().next(), &f), true);

        f.frequency = Frequency::Yearly;
        assert_eq!(fv.applies_at(&start, &f), true);
        assert_eq!(fv.applies_at(&start.next(), &f), false);
        assert_eq!(fv.applies_at(&start.next().next().next(), &f), false);
        assert_eq!(
            fv.applies_at(
                &Time {
                    year: start.year.next(),
                    month: start.month
                },
                &f
            ),
            true
        );

        Ok(())
    }

    #[test]
    fn test_flow_value_generic() -> Result<()> {
        #[derive(Debug)]
        struct Test {}
        impl FlowValue for Test {
            fn value_at(&self, _: &Time, _: &Flow, _: &CategoryValue) -> Result<Money> {
                panic!("Not implement for mock");
            }
        }

        test_applies_at(&Test {})
    }

    #[test]
    fn test_fixed_flow() -> Result<()> {
        let fv = FixedFlow {
            value: Money::from_dollars(100),
        };

        let test_flow = test_flow();
        verify_value_at(
            &fv,
            &test_flow,
            TestType::ByTime(vec![
                (test_flow.start.clone(), Money::from_dollars(100)),
                (
                    Time {
                        year: Year(2021),
                        month: Month::August,
                    },
                    Money::from_dollars(100),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::September,
                    },
                    Money::from_dollars(100),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::December,
                    },
                    Money::from_dollars(100),
                ),
            ]),
        )?;

        test_applies_at(&fv)
    }

    #[test]
    fn test_rate_flow() -> Result<()> {
        let fv = RateFlow {
            rate: Rate::from_percent(5),
        };

        let test_flow = test_flow();
        verify_value_at(
            &fv,
            &test_flow,
            TestType::ByValue(vec![
                (Money::from_dollars(0), Money::from_dollars(0)),
                (Money::from_dollars(1), Money::from_cents(5)),
                (Money::from_dollars(200), Money::from_dollars(10)),
            ]),
        )?;

        let fv = RateFlow {
            // Test a much smaller monthly rate
            rate: Rate::from_percent(8) / 12,
        };

        verify_value_at(
            &fv,
            &test_flow,
            TestType::ByValue(vec![
                (Money::from_dollars(0), Money::from_dollars(0)),
                (Money::from_dollars(10), Money::from_cents(6)),
                (Money::from_dollars(200), Money::from_cents(133)),
            ]),
        )?;

        test_applies_at(&fv)
    }

    #[test]
    fn test_table_flow() -> Result<()> {
        let fv = TableFlow {
            table: LookupTable::new(vec![
                (
                    TimeRange {
                        start: Time {
                            year: Year(2021),
                            month: Month::July,
                        },
                        end: Time {
                            year: Year(2021),
                            month: Month::September,
                        },
                    },
                    Money::from_dollars(10),
                ),
                (
                    TimeRange {
                        start: Time {
                            year: Year(2021),
                            month: Month::September,
                        },
                        end: Time {
                            year: Year(2021),
                            month: Month::November,
                        },
                    },
                    Money::from_dollars(20),
                ),
                (
                    TimeRange {
                        start: Time {
                            year: Year(2021),
                            month: Month::November,
                        },
                        end: Time {
                            year: Year(2025),
                            month: Month::January,
                        },
                    },
                    Money::from_dollars(30),
                ),
            ])
            .unwrap(),
        };

        let test_flow = test_flow();
        verify_value_at(
            &fv,
            &test_flow,
            TestType::ByTime(vec![
                (test_flow.start.clone(), Money::from_dollars(10)),
                (
                    Time {
                        year: Year(2021),
                        month: Month::August,
                    },
                    Money::from_dollars(10),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::September,
                    },
                    Money::from_dollars(20),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::December,
                    },
                    Money::from_dollars(30),
                ),
            ]),
        )?;

        test_applies_at(&fv)
    }

    #[test]
    fn test_rate_table_flow() -> Result<()> {
        let fv = RateTableFlow {
            table: LookupTable::new(vec![
                (
                    TimeRange {
                        start: Time {
                            year: Year(2021),
                            month: Month::July,
                        },
                        end: Time {
                            year: Year(2021),
                            month: Month::September,
                        },
                    },
                    Rate::from_percent(50),
                ),
                (
                    TimeRange {
                        start: Time {
                            year: Year(2021),
                            month: Month::September,
                        },
                        end: Time {
                            year: Year(2021),
                            month: Month::November,
                        },
                    },
                    Rate::from_percent(100),
                ),
                (
                    TimeRange {
                        start: Time {
                            year: Year(2021),
                            month: Month::November,
                        },
                        end: Time {
                            year: Year(2025),
                            month: Month::January,
                        },
                    },
                    Rate::from_percent(0),
                ),
            ])
            .unwrap(),
        };

        let test_flow = test_flow();
        verify_value_at(
            &fv,
            &test_flow,
            TestType::ByBoth(vec![
                (
                    Time {
                        year: Year(2021),
                        month: Month::August,
                    },
                    Money::from_dollars(0),
                    Money::from_dollars(0),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::August,
                    },
                    Money::from_dollars(20),
                    Money::from_dollars(10),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::August,
                    },
                    Money::from_dollars(30),
                    Money::from_dollars(15),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::September,
                    },
                    Money::from_dollars(0),
                    Money::from_dollars(0),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::September,
                    },
                    Money::from_dollars(50),
                    Money::from_dollars(50),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::September,
                    },
                    Money::from_dollars(20),
                    Money::from_dollars(20),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::December,
                    },
                    Money::from_dollars(0),
                    Money::from_dollars(0),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::December,
                    },
                    Money::from_dollars(100),
                    Money::from_dollars(0),
                ),
                (
                    Time {
                        year: Year(2021),
                        month: Month::December,
                    },
                    Money::from_dollars(150),
                    Money::from_dollars(0),
                ),
            ]),
        )?;

        test_applies_at(&fv)
    }
}
