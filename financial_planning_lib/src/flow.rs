use anyhow::{Context, Result};

use crate::asset::{CategoryValue, Money, Rate, Tx};
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

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;

    use crate::asset::{Asset, AssetName, Category, CategoryName};
    use crate::tax::{TaxPolicy, TaxTx};
    use crate::time::{Month, Time, TimeNext, Year};

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
        assert_eq!(
            fv.value_at(
                &test_flow.start,
                &test_flow,
                &Category::from_assets(CategoryName("unittest".to_string()), vec![]).value(),
            )
            .unwrap(),
            Money::from_dollars(100),
        );

        // Should not depend on time
        assert_eq!(
            fv.value_at(
                &test_flow.start.next(),
                &test_flow,
                &Category::from_assets(CategoryName("unittest".to_string()), vec![]).value(),
            )
            .unwrap(),
            Money::from_dollars(100),
        );

        // Or on asset value
        assert_eq!(
            fv.value_at(
                &test_flow.start.next(),
                &test_flow,
                &Category::from_assets(
                    CategoryName("unittest".to_string()),
                    vec![Asset {
                        name: AssetName("unit test asset".to_string()),
                        value: Money::from_dollars(500)
                    }]
                )
                .value(),
            )
            .unwrap(),
            Money::from_dollars(100),
        );

        test_applies_at(&fv)
    }

    #[test]
    fn test_rate_flow() -> Result<()> {
        let fv = RateFlow {
            rate: Rate::from_percent(5),
        };

        let test_flow = test_flow();
        assert_eq!(
            fv.value_at(
                &test_flow.start,
                &test_flow,
                &Category::from_assets(CategoryName("unittest".to_string()), vec![]).value(),
            )
            .unwrap(),
            Money::from_dollars(0),
        );

        // Should not depend on time
        assert_eq!(
            fv.value_at(
                &test_flow.start.next(),
                &test_flow,
                &Category::from_assets(CategoryName("unittest".to_string()), vec![]).value(),
            )
            .unwrap(),
            Money::from_dollars(0),
        );

        // But does depend on asset value
        assert_eq!(
            fv.value_at(
                &test_flow.start.next(),
                &test_flow,
                &Category::from_assets(
                    CategoryName("unittest".to_string()),
                    vec![Asset {
                        name: AssetName("unit test asset".to_string()),
                        value: Money::from_dollars(200)
                    }]
                )
                .value(),
            )
            .unwrap(),
            Money::from_dollars(10),
        );

        test_applies_at(&fv)
    }
}
